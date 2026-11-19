//! `jagprox login`: the launcher's browser login (a one-shot callback server on
//! 127.0.0.1), or email + password for terminals without a browser.

use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

use jagprox_core::api::{Backend, WEB_BASE_URL};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::session;

/// A login callback is only accepted for this long, as in the launcher.
const LOGIN_WINDOW: Duration = Duration::from_secs(10 * 60);

const SUCCESS_PAGE: &str = "<!DOCTYPE html>
<html>
<head><title>Authentication Success</title></head>
<body>
    <h1>Login Successful!</h1>
    <p>You can now close this browser window and return to your terminal.</p>
    <script>window.close();</script>
</body>
</html>";

fn random_state() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("no system randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Ports 8080-8090 like the launcher, then any free port (the website accepts any).
async fn bind_callback() -> Result<TcpListener, String> {
    for port in (8080..=8090).chain(std::iter::once(0)) {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            return Ok(listener);
        }
    }
    Err("could not open a local port for the login callback".into())
}

async fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Reads one request and returns its target (`/auth-callback?...`).
async fn read_target(stream: &mut TcpStream) -> Option<String> {
    let mut buf = vec![0u8; 8192];
    let mut len = 0;
    loop {
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf[len..]))
            .await
            .ok()?
            .ok()?;
        if n == 0 {
            break;
        }
        len += n;
        if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") || len == buf.len() {
            break;
        }
    }
    let head = std::str::from_utf8(&buf[..len]).ok()?;
    let mut parts = head.lines().next()?.split(' ');
    (parts.next()? == "GET").then(|| parts.next().map(str::to_string))?
}

pub async fn browser_login() -> Result<String, String> {
    let listener = bind_callback().await?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = random_state()?;
    let callback = format!("http://127.0.0.1:{port}/auth-callback");

    let mut url = reqwest::Url::parse(&format!("{WEB_BASE_URL}/login")).expect("valid URL");
    url.query_pairs_mut()
        .append_pair("redirect_uri", &callback)
        .append_pair("state", &state);

    println!("Opening your browser to log in to JagProx...");
    println!("If it doesn't open, visit:\n  {url}\n");
    println!("(No browser on this machine, e.g. over SSH? Use `jagprox login --email` instead.)");
    let _ = open::that_detached(url.as_str());

    let deadline = Instant::now() + LOGIN_WINDOW;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(accepted) = tokio::time::timeout(remaining, listener.accept()).await else {
            return Err("login timed out after 10 minutes".into());
        };
        let Ok((mut stream, _)) = accepted else {
            continue;
        };
        let Some(target) = read_target(&mut stream).await else {
            continue;
        };
        let parsed = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok();
        let Some(parsed) = parsed.filter(|u| u.path() == "/auth-callback") else {
            respond(&mut stream, "404 Not Found", "text/plain", "Not Found").await;
            continue;
        };
        let param = |name: &str| {
            parsed
                .query_pairs()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.into_owned())
        };

        // Any web page can make the browser hit this URL, so only accept our state, once.
        if param("state").as_deref() != Some(state.as_str()) {
            respond(
                &mut stream,
                "403 Forbidden",
                "text/plain",
                "Login state mismatch. Please try logging in again.",
            )
            .await;
            continue;
        }
        match param("token").filter(|t| session::looks_like_jwt(t)) {
            Some(token) => {
                respond(&mut stream, "200 OK", "text/html", SUCCESS_PAGE).await;
                return Ok(token);
            }
            None => {
                respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    "Error: No valid token provided.",
                )
                .await;
            }
        }
    }
}

pub async fn email_login(email: Option<String>) -> Result<String, String> {
    let email = match email {
        Some(e) => e,
        None => {
            print!("Email: ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            std::io::stdin()
                .lock()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            line.trim().to_string()
        }
    };
    if email.is_empty() {
        return Err("no email given".into());
    }
    let password = tokio::task::spawn_blocking(|| rpassword::prompt_password("Password: "))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("could not read password: {e}"))?;
    Backend::cli_login(&email, &password)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn callback_server_rejects_wrong_state_and_accepts_the_right_one() {
        let listener = bind_callback().await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_target(&mut stream).await
        });
        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client
            .write_all(b"GET /auth-callback?token=a.b.c&state=xyz HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let target = server.await.unwrap().unwrap();
        assert_eq!(target, "/auth-callback?token=a.b.c&state=xyz");
    }
}
