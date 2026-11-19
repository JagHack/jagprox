//! The update path end to end against a local "GitHub Releases": signed manifest,
//! download with hash check, binary swap. Fixtures in tests/fixtures/release are signed
//! with a throwaway test key (test-key.pub), not the release key.

use std::path::{Path, PathBuf};

use jagprox_core::update::{self, Source, Version};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/release")
}

/// Serves the fixture folder at /download/v3.1.0/ and /latest/download/. `tamper`
/// flips a byte of every file served, like a corrupted or malicious mirror.
async fn serve(tamper: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                }
                let line = String::from_utf8_lossy(&request);
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let name = path
                    .strip_prefix("/download/v3.1.0/")
                    .or_else(|| path.strip_prefix("/latest/download/"));
                let body = name.and_then(|n| std::fs::read(fixtures().join(n)).ok());
                let response = match body {
                    Some(mut body) => {
                        if tamper && name.is_some_and(|n| n.ends_with(".tar.gz")) {
                            body[20] ^= 1;
                        }
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        r.extend(body);
                        r
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = socket.write_all(&response).await;
            });
        }
    });
    format!("http://{addr}")
}

fn test_source(base: String) -> Source {
    Source {
        base,
        public_key: std::fs::read_to_string(fixtures().join("test-key.pub"))
            .unwrap()
            .trim()
            .to_string(),
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jagprox-update-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn latest_and_pinned_releases_verify() {
    let source = test_source(serve(false).await);
    let latest = update::fetch(&source, None).await.unwrap();
    assert_eq!(latest.version, Version::parse("3.1.0").unwrap());
    assert!(latest.url.ends_with("/download/v3.1.0"));
    assert_eq!(latest.assets.len(), 2);

    let pinned = update::fetch(&source, Version::parse("3.1.0").as_ref())
        .await
        .unwrap();
    assert_eq!(pinned.version, latest.version);

    let missing = update::fetch(&source, Version::parse("3.0.5").as_ref()).await;
    assert_eq!(missing.unwrap_err(), "there is no JagProx release v3.0.5");

    let electron = update::fetch(&source, Version::parse("2.6.1").as_ref()).await;
    assert!(electron.unwrap_err().contains("old Electron app"));
}

#[tokio::test]
async fn a_manifest_signed_by_another_key_is_refused() {
    let mut source = test_source(serve(false).await);
    // The real release key didn't sign the fixtures.
    source.public_key = Source::official().public_key;
    let err = update::fetch(&source, None).await.unwrap_err();
    assert!(
        err.contains("doesn't match the JagProx release key"),
        "{err}"
    );
}

#[test]
fn an_edited_manifest_is_refused() {
    let manifest = std::fs::read(fixtures().join("jagprox-release.json")).unwrap();
    let signature =
        std::fs::read_to_string(fixtures().join("jagprox-release.json.minisig")).unwrap();
    let key = test_source(String::new()).public_key;
    assert!(update::parse_manifest(&manifest, &signature, &key, "").is_ok());
    let edited = String::from_utf8(manifest)
        .unwrap()
        .replace("3.1.0", "9.9.9");
    let err = update::parse_manifest(edited.as_bytes(), &signature, &key, "").unwrap_err();
    assert!(err.contains("signature"), "{err}");
}

#[cfg(unix)]
#[tokio::test]
async fn download_checks_the_hash_and_replaces_the_binary() {
    let source = test_source(serve(false).await);
    let release = update::fetch(&source, None).await.unwrap();
    let asset = release.asset("cli-linux-x86_64").unwrap().clone();

    let dir = temp_dir("ok");
    let mut seen = Vec::new();
    let file = update::download(&release, &asset, &dir.join("dl"), |done, total| {
        seen.push((done, total))
    })
    .await
    .unwrap();
    assert_eq!(seen.first(), Some(&(0, asset.size)));
    assert_eq!(seen.last(), Some(&(asset.size, asset.size)));

    let exe = dir.join("bin/jagprox");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "old").unwrap();
    update::replace_binary(&file, &exe).unwrap();
    let output = std::process::Command::new(&exe).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "jagprox 3.1.0"
    );
    // Nothing left behind next to the binary.
    let leftovers: Vec<_> = std::fs::read_dir(exe.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, ["jagprox"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_corrupted_download_is_refused_and_removed() {
    let source = test_source(serve(true).await);
    let release = update::fetch(&source, None).await.unwrap();
    let asset = release.asset("cli-linux-x86_64").unwrap().clone();
    let dir = temp_dir("bad");
    let err = update::download(&release, &asset, &dir, |_, _| {})
        .await
        .unwrap_err();
    assert!(err.contains("doesn't match the signed release"), "{err}");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
