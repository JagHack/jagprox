//! The JagProx data folder, and where the JagProx login token lives: the OS keychain
//! (Windows Credential Manager, macOS Keychain, Secret Service on Linux), or a private
//! file when no keychain is available (headless servers, minimal desktops).
//! `JAGPROX_TOKEN` overrides both. Shared by the CLI and the launcher, so logging in
//! with one logs in the other.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

/// Folder name shared with the launcher, so existing settings carry over.
pub const DATA_DIR_NAME: &str = "jagprox-launcher";
const KEYRING_SERVICE: &str = "jagprox";
const KEYRING_USER: &str = "jwt";

pub fn data_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|d| d.join(DATA_DIR_NAME))
        .ok_or_else(|| "could not find your user config folder".into())
}

fn session_file() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("session.json"))
}

/// Same shape check as the launcher: three base64url parts.
pub fn looks_like_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

fn keyring_entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

/// Where a token was found or saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Env,
    Keychain,
    File,
}

impl Store {
    pub fn describe(self) -> &'static str {
        match self {
            Store::Env => "the JAGPROX_TOKEN environment variable",
            Store::Keychain => "your system keychain",
            Store::File => "a private file in your JagProx folder",
        }
    }
}

pub fn load() -> Option<(String, Store)> {
    if let Ok(token) = std::env::var("JAGPROX_TOKEN")
        && !token.trim().is_empty()
    {
        return Some((token.trim().to_string(), Store::Env));
    }
    if let Some(token) = keyring_entry()
        .and_then(|e| e.get_password().ok())
        .filter(|t| looks_like_jwt(t))
    {
        return Some((token, Store::Keychain));
    }
    let text = fs::read_to_string(session_file().ok()?).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("token")
        .and_then(Value::as_str)
        .filter(|t| looks_like_jwt(t))
        .map(|t| (t.to_string(), Store::File))
}

/// Saves to the keychain, or to a 0600 file if there is none.
pub fn save(token: &str) -> Result<Store, String> {
    if keyring_entry().is_some_and(|e| e.set_password(token).is_ok()) {
        let _ = remove_file();
        return Ok(Store::Keychain);
    }
    let path = session_file()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    write_private(&path, &json!({ "token": token }).to_string())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Store::File)
}

/// Removes the token everywhere it might be. Returns whether anything was removed.
pub fn clear() -> bool {
    let from_keyring = keyring_entry().is_some_and(|e| e.delete_credential().is_ok());
    let from_file = remove_file();
    from_keyring || from_file
}

fn remove_file() -> bool {
    session_file().is_ok_and(|p| fs::remove_file(p).is_ok())
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(content.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    // %APPDATA% is already private to the user on Windows.
    fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_shape() {
        assert!(looks_like_jwt("aaa.bbb.ccc"));
        assert!(looks_like_jwt("eyJhbGciOiJIUzI1NiJ9.eyJpZCI6MX0.sig-_x"));
        assert!(!looks_like_jwt("aaa.bbb"));
        assert!(!looks_like_jwt("aaa..ccc"));
        assert!(!looks_like_jwt("a a.b.c"));
    }
}
