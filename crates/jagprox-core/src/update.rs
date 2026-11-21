//! Updates: finding a release, checking its signature and installing it the way this
//! copy of JagProx was installed. Shared by `jagprox update` / `jagprox version` and the
//! launcher's Check for Updates button.
//!
//! Every release carries `jagprox-release.json` (its version plus the SHA-256 of each
//! file) and `jagprox-release.json.minisig`, signed with the JagProx release key. Only
//! files listed in a manifest that verifies against the key built in here are installed.

use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::install::Install;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
pub const MANIFEST: &str = "jagprox-release.json";
/// minisign public key of the JagProx release key.
const PUBLIC_KEY: &str = "RWSNUpaZxh9X3cHOwcdbO4mOCcOK7CC+d4O242TQx+FujzhW9H1YQxAc";
const RELEASES: &str = "https://github.com/JagHack/jagprox/releases";
const RELEASES_API: &str = "https://api.github.com/repos/JagHack/jagprox/releases?per_page=30";
/// Major versions before this are the old Electron app, which can't be installed
/// this way. (3.0.0 pre-releases count as 3.)
const FIRST_MAJOR: u64 = 3;

/// A version number: `3.1.0`, `v3.1.0`, `3.1.0-beta.2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) if !pre.is_empty() => (core, Some(pre.to_string())),
            Some(_) => return None,
            None => (text, None),
        };
        let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
        let version = Version {
            major: parts.next()??,
            minor: parts.next()??,
            patch: parts.next()??,
            pre,
        };
        parts.next().is_none().then_some(version)
    }

    pub fn current() -> Self {
        Self::parse(CURRENT).expect("the crate version is a valid version")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_pre(a, b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Semver pre-release order: numeric parts compare as numbers, numbers sort first.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut a = a.split('.');
    let mut b = b.split('.');
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Where releases come from and the key they must be signed with.
#[derive(Debug, Clone)]
pub struct Source {
    pub base: String,
    /// GitHub's release list (JSON), used to find pre-releases.
    pub api: String,
    pub public_key: String,
}

impl Source {
    /// GitHub Releases with the JagProx key. Debug builds can point elsewhere with
    /// `JAGPROX_RELEASES_URL` / `JAGPROX_RELEASES_API` / `JAGPROX_RELEASE_KEY` for
    /// testing; release builds can't.
    pub fn official() -> Self {
        let mut source = Source {
            base: RELEASES.to_string(),
            api: RELEASES_API.to_string(),
            public_key: PUBLIC_KEY.to_string(),
        };
        if cfg!(debug_assertions) {
            if let Ok(base) = std::env::var("JAGPROX_RELEASES_URL") {
                source.base = base;
            }
            if let Ok(api) = std::env::var("JAGPROX_RELEASES_API") {
                source.api = api;
            }
            if let Ok(key) = std::env::var("JAGPROX_RELEASE_KEY") {
                source.public_key = key;
            }
        }
        source
    }
}

/// One downloadable file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// What it is: `cli-linux-x86_64`, `appimage-x86_64`, `deb-x86_64`, `rpm-x86_64`,
    /// `windows-setup-x86_64`, `macos-app-universal`, ...
    pub kind: String,
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct Release {
    pub version: Version,
    pub assets: Vec<Asset>,
    /// Folder the assets download from.
    pub url: String,
}

impl Release {
    pub fn asset(&self, kind: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.kind == kind)
    }
}

/// Checks `version` can be installed at all (JagProx 3 or newer).
pub fn check_installable(version: &Version) -> Result<(), String> {
    if version.major < FIRST_MAJOR {
        return Err(format!(
            "v{version} is the old Electron app and can't be installed with this command; \
             it can install JagProx {FIRST_MAJOR} and newer"
        ));
    }
    Ok(())
}

/// The newest JagProx 3+ version on the releases page, pre-releases included
/// (GitHub's "latest" skips them).
pub async fn newest_version(source: &Source) -> Result<Version, String> {
    let body = get(&source.api).await.map_err(|e| {
        if e == "not found" {
            "could not list the JagProx releases".to_string()
        } else {
            e
        }
    })?;
    let releases: Value =
        serde_json::from_slice(&body).map_err(|_| "the release list is unreadable")?;
    releases
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r.get("draft").and_then(Value::as_bool) != Some(true))
        .filter_map(|r| {
            r.get("tag_name")
                .and_then(Value::as_str)
                .and_then(Version::parse)
        })
        .filter(|v| v.major >= FIRST_MAJOR)
        .max()
        .ok_or_else(|| "there is no JagProx 3 release yet".into())
}

/// The release `jagprox update` should look at: the latest full release, or for
/// someone already on a pre-release, the newest release of any kind.
pub async fn fetch_newest(source: &Source, current: &Version) -> Result<Release, String> {
    if current.pre.is_some() {
        let newest = newest_version(source).await?;
        fetch(source, Some(&newest)).await
    } else {
        fetch(source, None).await
    }
}

async fn get(url: &str) -> Result<Vec<u8>, String> {
    let response = crate::http::client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("could not reach {url}: {}", e.without_url()))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("not found".into());
    }
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("download of {url} failed: {}", e.without_url()))
}

/// Fetches and verifies the manifest of the latest release, or of `version`.
pub async fn fetch(source: &Source, version: Option<&Version>) -> Result<Release, String> {
    let folder = match version {
        Some(v) => {
            check_installable(v)?;
            format!("{}/download/v{v}", source.base)
        }
        None => format!("{}/latest/download", source.base),
    };
    let manifest = get(&format!("{folder}/{MANIFEST}")).await.map_err(|e| {
        if e == "not found" {
            match version {
                Some(v) => format!("there is no JagProx release v{v}"),
                None => "the latest JagProx release has no update information yet".into(),
            }
        } else {
            e
        }
    })?;
    let signature = get(&format!("{folder}/{MANIFEST}.minisig"))
        .await
        .map_err(|e| {
            if e == "not found" {
                "the release is not signed, so it won't be installed".into()
            } else {
                e
            }
        })?;
    let signature =
        String::from_utf8(signature).map_err(|_| "the release signature is unreadable")?;
    let release = parse_manifest(&manifest, &signature, &source.public_key, &source.base)?;
    if let Some(v) = version
        && release.version != *v
    {
        return Err(format!(
            "the release for v{v} describes v{} instead; not installing it",
            release.version
        ));
    }
    Ok(release)
}

/// Verifies the signature, then reads the manifest.
pub fn parse_manifest(
    manifest: &[u8],
    signature: &str,
    public_key: &str,
    base: &str,
) -> Result<Release, String> {
    let key = minisign_verify::PublicKey::from_base64(public_key)
        .map_err(|e| format!("bad release key: {e}"))?;
    let sig = minisign_verify::Signature::decode(signature)
        .map_err(|_| "the release signature is unreadable")?;
    key.verify(manifest, &sig, false).map_err(
        |_| "the release signature doesn't match the JagProx release key; not installing it",
    )?;

    let json: Value =
        serde_json::from_slice(manifest).map_err(|_| "the release information is unreadable")?;
    let version = json
        .get("version")
        .and_then(Value::as_str)
        .and_then(Version::parse)
        .ok_or("the release information has no version")?;
    let mut assets = Vec::new();
    for item in json
        .get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let field = |name: &str| item.get(name).and_then(Value::as_str).unwrap_or_default();
        let asset = Asset {
            kind: field("kind").to_string(),
            name: field("name").to_string(),
            sha256: field("sha256").to_ascii_lowercase(),
            size: item.get("size").and_then(Value::as_u64).unwrap_or(0),
        };
        let safe_name = !asset.name.is_empty()
            && !asset.name.starts_with('.')
            && !asset.name.contains(['/', '\\']);
        if asset.kind.is_empty() || !safe_name || asset.sha256.len() != 64 {
            return Err("the release information lists a malformed file".into());
        }
        assets.push(asset);
    }
    Ok(Release {
        url: format!("{base}/download/v{version}"),
        version,
        assets,
    })
}

/// Downloads `asset` into `dir` and checks its hash. `progress` gets (bytes, total).
pub async fn download(
    release: &Release,
    asset: &Asset,
    dir: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    // Earlier downloads are done with: a Windows installer can't delete itself, so
    // the previous one is still here. (One that is still running stays.)
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = fs::remove_file(entry.path());
        }
    }
    let url = format!("{}/{}", release.url, asset.name);
    let mut response = crate::http::client()
        .get(&url)
        .timeout(Duration::from_secs(30 * 60))
        .send()
        .await
        .map_err(|e| format!("could not download {}: {}", asset.name, e.without_url()))?;
    if !response.status().is_success() {
        return Err(format!(
            "downloading {} failed: {}",
            asset.name,
            response.status()
        ));
    }
    let total = response.content_length().unwrap_or(asset.size);
    let partial = dir.join(format!("{}.part", asset.name));
    let mut file = fs::File::create(&partial)
        .map_err(|e| format!("could not write {}: {e}", partial.display()))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    progress(0, total);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("downloading {} failed: {}", asset.name, e.without_url()))?
    {
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| format!("could not write {}: {e}", partial.display()))?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let hash = hex(&hasher.finalize());
    if hash != asset.sha256 {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "{} doesn't match the signed release (SHA-256 {hash}); not installing it",
            asset.name
        ));
    }
    let path = dir.join(&asset.name);
    fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where updates are downloaded to.
/// Named like the data folder: on Windows the cache folder is %LOCALAPPDATA%, where
/// "jagprox" would be the install folder itself (Windows ignores case).
pub fn download_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(crate::account::DATA_DIR_NAME)
        .join("updates")
}

/// The CPU name used in asset kinds (Rust's names: `x86_64`, `aarch64`).
pub fn arch() -> &'static str {
    std::env::consts::ARCH
}

/// How an update gets installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Swap the `jagprox` binary for the one in a `.tar.gz`.
    ReplaceBinary { asset: Asset, exe: PathBuf },
    /// Swap the AppImage file.
    ReplaceAppImage { asset: Asset, path: PathBuf },
    /// Install a .deb/.rpm with the system package manager (needs sudo).
    Package { asset: Asset, manager: PackageTool },
    /// Run the Windows installer.
    WindowsInstaller { asset: Asset },
    /// Swap the macOS app bundle for the one in a `.app.tar.gz`.
    ReplaceBundle { asset: Asset, bundle: PathBuf },
    /// Can't be done from here; the text says how.
    Manual(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageTool {
    Apt,
    Dnf,
    Zypper,
    Rpm,
}

impl PackageTool {
    /// The command that installs `file`; `downgrade` when going to an older version.
    pub fn command(self, file: &Path, downgrade: bool) -> Vec<String> {
        let file = file.display().to_string();
        let mut args: Vec<&str> = match (self, downgrade) {
            (PackageTool::Apt, false) => vec!["apt", "install"],
            (PackageTool::Apt, true) => vec!["apt", "install", "--allow-downgrades"],
            (PackageTool::Dnf, false) => vec!["dnf", "install"],
            (PackageTool::Dnf, true) => vec!["dnf", "downgrade"],
            (PackageTool::Zypper, false) => vec!["zypper", "install", "--allow-unsigned-rpm"],
            (PackageTool::Zypper, true) => {
                vec!["zypper", "install", "--allow-unsigned-rpm", "--oldpackage"]
            }
            (PackageTool::Rpm, false) => vec!["rpm", "-U"],
            (PackageTool::Rpm, true) => vec!["rpm", "-U", "--oldpackage"],
        };
        args.insert(0, "sudo");
        let mut command: Vec<String> = args.into_iter().map(String::from).collect();
        command.push(file);
        command
    }

    fn detect() -> Self {
        if which("dnf") {
            PackageTool::Dnf
        } else if which("zypper") {
            PackageTool::Zypper
        } else {
            PackageTool::Rpm
        }
    }
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

/// Picks how to install `release` for this copy of JagProx.
pub fn plan(install: &Install, release: &Release) -> Plan {
    let arch = arch();
    let need = |kind: String| {
        release.asset(&kind).cloned().ok_or_else(|| {
            format!(
                "v{} has no download for this system ({kind})",
                release.version
            )
        })
    };
    let result = match install {
        Install::Standalone { exe } => need(format!("cli-linux-{arch}")).map(|asset| {
            Plan::ReplaceBinary {
                asset,
                exe: exe.clone(),
            }
        }),
        Install::AppImage { path } => need(format!("appimage-{arch}")).map(|asset| {
            Plan::ReplaceAppImage {
                asset,
                path: path.clone(),
            }
        }),
        Install::Deb => need(format!("deb-{arch}")).map(|asset| Plan::Package {
            asset,
            manager: PackageTool::Apt,
        }),
        Install::Rpm => need(format!("rpm-{arch}")).map(|asset| Plan::Package {
            asset,
            manager: PackageTool::detect(),
        }),
        Install::Aur => Ok(Plan::Manual(
            "JagProx came from the AUR: update it with your AUR helper (for example `yay -Syu jagprox-bin`).".into(),
        )),
        Install::Nix => Ok(Plan::Manual(
            "JagProx came from Nix: update it the way you installed it (your channel, flake input or `nix profile upgrade`).".into(),
        )),
        Install::Windows { .. } => {
            need("windows-setup-x86_64".into()).map(|asset| Plan::WindowsInstaller { asset })
        }
        Install::MacApp { bundle } => {
            need("macos-app-universal".into()).map(|asset| Plan::ReplaceBundle {
                asset,
                bundle: bundle.clone(),
            })
        }
        Install::Dev => Ok(Plan::Manual(
            "This is a development build; update it with git and cargo.".into(),
        )),
        Install::Unknown => Ok(Plan::Manual(format!(
            "Download v{} from {RELEASES}/tag/v{}",
            release.version, release.version
        ))),
    };
    result.unwrap_or_else(Plan::Manual)
}

/// A temporary path in the same folder as `target`, so a rename can replace it.
#[cfg(unix)]
fn sibling_temp(target: &Path) -> Result<PathBuf, String> {
    let dir = target
        .parent()
        .ok_or_else(|| format!("{} has no folder", target.display()))?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    Ok(dir.join(format!(".{name}.update-{}", std::process::id())))
}

#[cfg(unix)]
fn write_error(target: &Path, e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        format!(
            "no permission to replace {} (try again with sudo)",
            target.display()
        )
    } else {
        format!("could not replace {}: {e}", target.display())
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

/// Extracts the `jagprox` binary from `archive` (.tar.gz) over `exe`.
#[cfg(unix)]
pub fn replace_binary(archive: &Path, exe: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let temp = sibling_temp(exe)?;
    let mut found = false;
    for entry in tar.entries().map_err(|e| format!("bad archive: {e}"))? {
        let mut entry = entry.map_err(|e| format!("bad archive: {e}"))?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        if path.file_name().is_some_and(|n| n == "jagprox") && entry.header().entry_type().is_file()
        {
            let mut out = fs::File::create(&temp).map_err(|e| write_error(exe, e))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| write_error(exe, e))?;
            found = true;
            break;
        }
    }
    if !found {
        return Err("the download has no jagprox binary in it".into());
    }
    let result = make_executable(&temp).and_then(|()| fs::rename(&temp, exe));
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(write_error(exe, e));
    }
    Ok(())
}

/// Puts the downloaded AppImage in place of `target`, keeping its name.
#[cfg(unix)]
pub fn replace_appimage(downloaded: &Path, target: &Path) -> Result<(), String> {
    let temp = sibling_temp(target)?;
    let result = fs::copy(downloaded, &temp)
        .and_then(|_| make_executable(&temp))
        .and_then(|()| fs::rename(&temp, target));
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(write_error(target, e));
    }
    let _ = fs::remove_file(downloaded);
    Ok(())
}

/// Swaps the app bundle at `bundle` for the one in `archive` (.app.tar.gz).
#[cfg(unix)]
pub fn replace_bundle(archive: &Path, bundle: &Path) -> Result<(), String> {
    let staging = sibling_temp(bundle)?;
    fs::create_dir_all(&staging).map_err(|e| write_error(bundle, e))?;
    let unpack = || -> Result<PathBuf, String> {
        let file = fs::File::open(archive).map_err(|e| e.to_string())?;
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        tar.set_preserve_permissions(true);
        tar.unpack(&staging)
            .map_err(|e| format!("could not unpack the update: {e}"))?;
        fs::read_dir(&staging)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "app"))
            .ok_or_else(|| "the download has no app in it".to_string())
    };
    let new_app = match unpack() {
        Ok(app) => app,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let old = staging.join("previous.app");
    fs::rename(bundle, &old).map_err(|e| write_error(bundle, e))?;
    if let Err(e) = fs::rename(&new_app, bundle) {
        let _ = fs::rename(&old, bundle);
        let _ = fs::remove_dir_all(&staging);
        return Err(write_error(bundle, e));
    }
    let _ = fs::remove_dir_all(&staging);
    Ok(())
}

/// Starts the Windows installer and returns; the caller should exit so the installer
/// can replace its files. `relaunch` starts the launcher when it's done. Our
/// installer.nsi takes `/S` (silent) and `--force-run` (start the launcher after a
/// silent install).
pub fn run_windows_installer(installer: &Path, relaunch: bool) -> Result<(), String> {
    let mut command = std::process::Command::new(installer);
    command.arg("/S");
    if relaunch {
        command.arg("--force-run");
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the installer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(v("v3.1.2"), v("3.1.2"));
        assert_eq!(v("3.0.0-alpha.0").to_string(), "3.0.0-alpha.0");
        assert!(Version::parse("3.1").is_none());
        assert!(Version::parse("3.1.0.4").is_none());
        assert!(Version::parse("3.1.0-").is_none());
        assert!(Version::parse("latest").is_none());
        let mut list = [
            v("3.0.0"),
            v("3.0.0-beta.11"),
            v("3.0.0-alpha.0"),
            v("2.6.1"),
            v("3.0.0-beta.2"),
            v("3.0.0-beta"),
            v("3.10.0"),
            v("3.2.0"),
        ];
        list.sort();
        let sorted: Vec<String> = list.iter().map(ToString::to_string).collect();
        assert_eq!(
            sorted,
            [
                "2.6.1",
                "3.0.0-alpha.0",
                "3.0.0-beta",
                "3.0.0-beta.2",
                "3.0.0-beta.11",
                "3.0.0",
                "3.2.0",
                "3.10.0"
            ]
        );
    }

    #[test]
    fn electron_versions_are_refused() {
        assert!(check_installable(&v("2.6.1")).is_err());
        assert!(check_installable(&v("2.99.0-beta.1")).is_err());
        assert!(check_installable(&v("3.0.0-alpha.2")).is_ok());
        assert!(check_installable(&v("3.0.0")).is_ok());
    }

    fn release(kinds: &[&str]) -> Release {
        Release {
            version: v("3.1.0"),
            url: "https://example.invalid/download/v3.1.0".into(),
            assets: kinds
                .iter()
                .map(|k| Asset {
                    kind: k.to_string(),
                    name: format!("{k}.bin"),
                    sha256: "0".repeat(64),
                    size: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn plans_follow_the_install_kind() {
        let arch = arch();
        let r = release(&[
            &format!("cli-linux-{arch}"),
            &format!("appimage-{arch}"),
            &format!("deb-{arch}"),
            "windows-setup-x86_64",
            "macos-app-universal",
        ]);
        let exe = PathBuf::from("/home/me/.local/bin/jagprox");
        assert!(matches!(
            plan(&Install::Standalone { exe: exe.clone() }, &r),
            Plan::ReplaceBinary { exe: e, .. } if e == exe
        ));
        assert!(matches!(
            plan(
                &Install::AppImage {
                    path: "/a/JagProx.AppImage".into()
                },
                &r
            ),
            Plan::ReplaceAppImage { .. }
        ));
        assert!(matches!(
            plan(&Install::Deb, &r),
            Plan::Package {
                manager: PackageTool::Apt,
                ..
            }
        ));
        assert!(matches!(
            plan(&Install::Windows { dir: "C:/x".into() }, &r),
            Plan::WindowsInstaller { .. }
        ));
        assert!(matches!(
            plan(
                &Install::MacApp {
                    bundle: "/Applications/JagProx.app".into()
                },
                &r
            ),
            Plan::ReplaceBundle { .. }
        ));
        assert!(matches!(plan(&Install::Aur, &r), Plan::Manual(m) if m.contains("AUR")));
        assert!(matches!(plan(&Install::Nix, &r), Plan::Manual(m) if m.contains("Nix")));
        // No rpm in this release.
        assert!(matches!(plan(&Install::Rpm, &r), Plan::Manual(m) if m.contains("no download")));
    }

    #[test]
    fn package_commands() {
        let file = Path::new("/tmp/jagprox.deb");
        assert_eq!(
            PackageTool::Apt.command(file, false),
            ["sudo", "apt", "install", "/tmp/jagprox.deb"]
        );
        assert_eq!(
            PackageTool::Dnf.command(file, true),
            ["sudo", "dnf", "downgrade", "/tmp/jagprox.deb"]
        );
    }
}
