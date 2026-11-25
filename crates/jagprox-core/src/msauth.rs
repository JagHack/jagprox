//! Microsoft sign-in for the Hypixel side of the proxy, and Mojang's session server.
//!
//! Device code → Microsoft token (login.live.com, the Minecraft Nintendo Switch
//! title, as node-minecraft-protocol uses) → Xbox Live user token → XSTS token →
//! Minecraft access token → profile. Accounts are cached by Minecraft name so the
//! proxy can pick the one matching whoever connects.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::http::client;

const CLIENT_ID: &str = "00000000441cc96b";
const SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";
/// Renew Minecraft tokens this long before they expire.
const EXPIRY_MARGIN: u64 = 10 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    Network(String),
    /// The user didn't enter the code in time.
    Expired,
    Declined,
    /// Microsoft/Xbox/Minecraft refused, with a readable reason.
    Rejected(String),
    NoMinecraft,
    /// No signed-in account for this Minecraft name.
    NoAccount(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Network(e) => write!(f, "network error: {e}"),
            AuthError::Expired => write!(f, "the sign-in code expired before it was used"),
            AuthError::Declined => write!(f, "the sign-in was declined"),
            AuthError::Rejected(e) => write!(f, "{e}"),
            AuthError::NoMinecraft => write!(
                f,
                "this Microsoft account doesn't own Minecraft: Java Edition"
            ),
            AuthError::NoAccount(name) => {
                write!(f, "no Microsoft account for {name} is signed in to JagProx")
            }
        }
    }
}

impl std::error::Error for AuthError {}

impl From<reqwest::Error> for AuthError {
    fn from(e: reqwest::Error) -> Self {
        AuthError::Network(e.without_url().to_string())
    }
}

type AuthResult<T> = Result<T, AuthError>;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn json_of(response: reqwest::Response) -> AuthResult<(u16, Value)> {
    let status = response.status().as_u16();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    Ok((status, body))
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

// ---------------------------------------------------------------------------
// Microsoft
// ---------------------------------------------------------------------------

/// What the user has to do to sign in.
#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub user_code: String,
    /// Usually https://www.microsoft.com/link
    pub verification_uri: String,
    device_code: String,
    interval: u64,
    expires_at: u64,
}

impl DeviceCode {
    /// A link that pre-fills the code.
    pub fn link(&self) -> String {
        format!("{}?otc={}", self.verification_uri, self.user_code)
    }
}

pub async fn start_device_code() -> AuthResult<DeviceCode> {
    let response = client()
        .post("https://login.live.com/oauth20_connect.srf")
        .form(&[
            ("scope", SCOPE),
            ("client_id", CLIENT_ID),
            ("response_type", "device_code"),
        ])
        .send()
        .await?;
    let (status, body) = json_of(response).await?;
    let field = |k| {
        text(&body, k)
            .ok_or_else(|| AuthError::Rejected(format!("Microsoft sign-in failed ({status})")))
    };
    Ok(DeviceCode {
        user_code: field("user_code")?,
        verification_uri: field("verification_uri")?,
        device_code: field("device_code")?,
        interval: body
            .get("interval")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .max(1),
        expires_at: now()
            + body
                .get("expires_in")
                .and_then(Value::as_u64)
                .unwrap_or(900),
    })
}

/// Microsoft refresh token, kept to sign in again without the user.
#[derive(Debug, Clone)]
struct MsToken {
    access_token: String,
    refresh_token: String,
}

fn ms_token(body: &Value) -> Option<MsToken> {
    Some(MsToken {
        access_token: text(body, "access_token")?,
        refresh_token: text(body, "refresh_token")?,
    })
}

/// Waits until the user entered the code (or it expires).
async fn poll_device_code(code: &DeviceCode) -> AuthResult<MsToken> {
    let mut interval = code.interval;
    while now() < code.expires_at {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let response = client()
            .post("https://login.live.com/oauth20_token.srf")
            .form(&[
                ("client_id", CLIENT_ID),
                ("device_code", code.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?;
        let (_, body) = json_of(response).await?;
        if let Some(token) = ms_token(&body) {
            return Ok(token);
        }
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("expired_token") => return Err(AuthError::Expired),
            Some("authorization_declined") => return Err(AuthError::Declined),
            Some(other) => {
                return Err(AuthError::Rejected(format!(
                    "Microsoft sign-in failed: {other}"
                )));
            }
            None => {}
        }
    }
    Err(AuthError::Expired)
}

async fn refresh_ms_token(refresh_token: &str) -> AuthResult<MsToken> {
    let response = client()
        .post("https://login.live.com/oauth20_token.srf")
        .form(&[
            ("scope", SCOPE),
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?;
    let (status, body) = json_of(response).await?;
    ms_token(&body).ok_or_else(|| {
        AuthError::Rejected(format!(
            "your Microsoft sign-in expired ({}), please sign in again",
            text(&body, "error").unwrap_or_else(|| status.to_string())
        ))
    })
}

// ---------------------------------------------------------------------------
// Xbox Live → Minecraft
// ---------------------------------------------------------------------------

/// Readable reasons for XSTS refusals (from prismarine-auth).
fn xbox_error(code: u64) -> String {
    match code {
        2148916227 => "Your account was banned by Xbox for violating one or more Community Standards for Xbox and is unable to be used.".into(),
        2148916229 => "Your account is restricted and your guardian has not given you permission to play online. Ask them to change it at https://account.microsoft.com/family/".into(),
        2148916233 => "Your account doesn't have an Xbox profile yet. Create one at https://signup.live.com/signup".into(),
        2148916234 => "Your account hasn't accepted Xbox's Terms of Service. Log in at https://www.xbox.com and accept them.".into(),
        2148916235 => "Xbox doesn't allow sign-ins from your account's region.".into(),
        2148916236 | 2148916237 => "Your account needs proof of age. Log in at https://login.live.com/login.srf and provide it.".into(),
        2148916238 => "The account is under 18 and must be added to a family by an adult.".into(),
        other => format!("Xbox Live refused the sign-in (error {other})."),
    }
}

/// A Minecraft session: what joining a server needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McSession {
    pub access_token: String,
    pub expires_at: u64,
    /// Undashed UUID.
    pub uuid: String,
    pub name: String,
}

async fn minecraft_login(rps_ticket: &str) -> AuthResult<McSession> {
    let response = client()
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Accept", "application/json")
        .header("x-xbl-contract-version", "1")
        .json(&json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": rps_ticket },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let (status, xbl) = json_of(response).await?;
    let xbl_token = text(&xbl, "Token")
        .ok_or_else(|| AuthError::Rejected(format!("Xbox Live sign-in failed ({status})")))?;
    let user_hash = xbl
        .pointer("/DisplayClaims/xui/0/uhs")
        .and_then(Value::as_str)
        .ok_or_else(|| AuthError::Rejected("Xbox Live returned no user hash".into()))?
        .to_string();

    let response = client()
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let (status, xsts) = json_of(response).await?;
    if let Some(code) = xsts.get("XErr").and_then(Value::as_u64) {
        return Err(AuthError::Rejected(xbox_error(code)));
    }
    let xsts_token = text(&xsts, "Token")
        .ok_or_else(|| AuthError::Rejected(format!("Xbox sign-in failed ({status})")))?;

    let response = client()
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .header("Accept", "application/json")
        .json(&json!({ "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}") }))
        .send()
        .await?;
    let (status, mc) = json_of(response).await?;
    let access_token = text(&mc, "access_token")
        .ok_or_else(|| AuthError::Rejected(format!("Minecraft sign-in failed ({status})")))?;
    let expires_at = now()
        + mc.get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(86400);

    let response = client()
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&access_token)
        .send()
        .await?;
    let (status, profile) = json_of(response).await?;
    if status == 404 {
        return Err(AuthError::NoMinecraft);
    }
    match (text(&profile, "id"), text(&profile, "name")) {
        (Some(uuid), Some(name)) => Ok(McSession {
            access_token,
            expires_at,
            uuid,
            name,
        }),
        _ => Err(AuthError::Rejected(format!(
            "could not load the Minecraft profile ({status})"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Session server
// ---------------------------------------------------------------------------

/// Tells Mojang we're joining a server (the upstream half of an online-mode login).
pub async fn session_join(session: &McSession, server_hash: &str) -> AuthResult<()> {
    let response = client()
        .post("https://sessionserver.mojang.com/session/minecraft/join")
        .json(&json!({ "accessToken": session.access_token, "selectedProfile": session.uuid, "serverId": server_hash }))
        .send()
        .await?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let (_, body) = json_of(response).await?;
    let reason = text(&body, "errorMessage")
        .or_else(|| text(&body, "error"))
        .unwrap_or_else(|| status.to_string());
    Err(AuthError::Rejected(format!(
        "Mojang rejected the join: {reason}"
    )))
}

/// A player's verified profile from `hasJoined`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinedProfile {
    /// Undashed UUID.
    pub uuid: String,
    pub name: String,
    pub properties: Vec<(String, String, Option<String>)>,
}

/// Checks that `username` really joined with `server_hash` (the proxy's online-mode
/// check of the connecting player). `None` means the check failed.
pub async fn has_joined(username: &str, server_hash: &str) -> AuthResult<Option<JoinedProfile>> {
    let response = client()
        .get("https://sessionserver.mojang.com/session/minecraft/hasJoined")
        .query(&[("username", username), ("serverId", server_hash)])
        .send()
        .await?;
    if response.status().as_u16() != 200 {
        return Ok(None);
    }
    let body: Value = response.json().await?;
    let (Some(uuid), Some(name)) = (text(&body, "id"), text(&body, "name")) else {
        return Ok(None);
    };
    let properties = body
        .get("properties")
        .and_then(Value::as_array)
        .map(|props| {
            props
                .iter()
                .filter_map(|p| Some((text(p, "name")?, text(p, "value")?, text(p, "signature"))))
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(JoinedProfile {
        uuid,
        name,
        properties,
    }))
}

// ---------------------------------------------------------------------------
// Cookie sign-in (import a browser login session, the way Lumiere does)
// ---------------------------------------------------------------------------

/// A desktop-browser User-Agent for the OAuth request; Microsoft's cookie flow treats
/// an unknown client more suspiciously, so we look like a browser.
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";

/// Microsoft login cookies parsed from a Netscape `cookies.txt` export.
fn parse_microsoft_cookies(contents: &str) -> std::collections::HashMap<String, String> {
    let mut cookies = std::collections::HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 7 {
            continue;
        }
        let domain = parts[0].to_ascii_lowercase();
        if !domain.contains("login.live.com")
            && domain != ".live.com"
            && domain != ".account.microsoft.com"
        {
            continue;
        }
        let (name, value) = (parts[5].trim(), parts[6].trim());
        if name.is_empty() || value.is_empty() || value == "Disabled" {
            continue;
        }
        cookies
            .entry(name.to_string())
            .or_insert_with(|| value.to_string());
    }
    cookies
}

/// The `Cookie:` header for the OAuth request, in the order Microsoft expects.
fn cookie_header(cookies: &std::collections::HashMap<String, String>, use_jsh: bool) -> String {
    let mut order: Vec<&str> = vec![
        if use_jsh { "JSH" } else { "JSHP" },
        "MSPAuth",
        "MSPBack",
        "MSPProf",
        "MSPRequ",
        "MSPSoftVis",
        "NAP",
        "OParams",
        "PPLState",
        "WLSSC",
    ];
    for name in cookies.keys() {
        if !order.contains(&name.as_str())
            && (name == "uaid" || name.starts_with("__Host-"))
        {
            order.push(name.as_str());
        }
    }
    order
        .iter()
        .filter_map(|n| cookies.get(*n).map(|v| format!("{n}={v}")))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Decodes a percent-encoded value (and `+` as space), like Java's `URLDecoder`.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 3 <= bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Exchanges Microsoft login cookies for an RPS access token via the implicit OAuth
/// grant: the token comes back in the `#access_token=` fragment of a 302 redirect that
/// we do not follow. `None` means these cookies didn't produce a token.
async fn rps_token_from_cookies(
    cookies: &std::collections::HashMap<String, String>,
    use_jsh: bool,
) -> AuthResult<Option<String>> {
    let url = "https://login.live.com/oauth20_authorize.srf?redirect_uri=https://sisu.xboxlive.com/connect/oauth/XboxLive&response_type=token&client_id=000000004420578E&scope=XboxLive.Signin%20XboxLive.offline_access";
    let response = crate::http::no_redirect_client()
        .get(url)
        .header("User-Agent", BROWSER_UA)
        .header("Cookie", cookie_header(cookies, use_jsh))
        .send()
        .await?;
    if response.status().as_u16() != 302 {
        return Ok(None);
    }
    let Some(location) = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
    else {
        return Ok(None);
    };
    let token = location
        .split_once('#')
        .and_then(|(_, fragment)| {
            fragment
                .split('&')
                .find_map(|p| p.strip_prefix("access_token="))
        })
        .map(percent_decode);
    Ok(token)
}

// ---------------------------------------------------------------------------
// Account store
// ---------------------------------------------------------------------------

/// Signed-in Microsoft accounts, saved in `accounts.json` (owner-only on Unix).
pub struct Accounts {
    path: PathBuf,
}

impl Accounts {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("accounts.json"),
        }
    }

    /// The saved accounts; none if the file doesn't exist yet. A file that can't be
    /// read or parsed is an error, never "no accounts": the next save would otherwise
    /// drop every other account's refresh token.
    fn load(&self) -> std::io::Result<Vec<Value>> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("accounts").and_then(Value::as_array).cloned())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} is not a valid account list", self.path.display()),
                )
            })
    }

    /// Read-modify-write of the account list. On a broken file nothing is written; it
    /// is backed up so the user can recover the tokens in it.
    // shortcut: no lock across processes, so a CLI `jagprox accounts add` racing the
    // launcher's proxy refreshing a token can drop one of the two writes; add a file
    // lock (fs4) around this if that ever shows up.
    fn modify<T>(&self, change: impl FnOnce(&mut Vec<Value>) -> T) -> std::io::Result<T> {
        let mut accounts = self.load().map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData
                && let Ok(backup) = crate::file_util::backup_file(&self.path)
            {
                return std::io::Error::new(
                    e.kind(),
                    format!("{e} (left untouched, copy saved to {})", backup.display()),
                );
            }
            e
        })?;
        let result = change(&mut accounts);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let content =
            serde_json::to_string_pretty(&json!({ "accounts": accounts })).unwrap_or_default();
        // Owner-only: write_file_atomic creates files 0600 on Unix.
        crate::file_util::write_file_atomic(&self.path, &content)?;
        Ok(result)
    }

    /// Minecraft names of the signed-in accounts (none if accounts.json is broken).
    pub fn names(&self) -> Vec<String> {
        self.load()
            .unwrap_or_default()
            .iter()
            .filter_map(|a| text(a, "name"))
            .collect()
    }

    fn store(&self, ms: &MsToken, session: &McSession) -> std::io::Result<()> {
        self.modify(|accounts| {
            accounts.retain(|a| text(a, "uuid").as_deref() != Some(&session.uuid));
            accounts.push(json!({
                "name": session.name,
                "uuid": session.uuid,
                "ms_refresh_token": ms.refresh_token,
                "mc_access_token": session.access_token,
                "mc_expires_at": session.expires_at,
            }));
        })
    }

    /// Removes an account by Minecraft name (case-insensitive). Returns whether it existed.
    pub fn remove(&self, name: &str) -> std::io::Result<bool> {
        if !self.load()?.iter().any(|a| has_name(a, name)) {
            return Ok(false);
        }
        self.modify(|accounts| {
            let before = accounts.len();
            accounts.retain(|a| !has_name(a, name));
            accounts.len() != before
        })
    }

    /// Signs in a new account with a device code. `show_code` is called once with
    /// what the user must do.
    pub async fn sign_in(&self, show_code: impl FnOnce(&DeviceCode)) -> AuthResult<McSession> {
        let code = start_device_code().await?;
        show_code(&code);
        let ms = poll_device_code(&code).await?;
        let session = minecraft_login(&ms.access_token).await?;
        self.store(&ms, &session)
            .map_err(|e| AuthError::Rejected(format!("could not save the account: {e}")))?;
        Ok(session)
    }

    /// Signs in a new account by importing Microsoft login cookies exported from a
    /// browser (Netscape `cookies.txt` format), the way Lumiere does. The implicit
    /// grant returns no refresh token, so the account lasts only until the Minecraft
    /// token expires (~24h) — then the cookies must be re-imported.
    pub async fn cookie_sign_in(&self, cookie_file: &str) -> AuthResult<McSession> {
        let cookies = parse_microsoft_cookies(cookie_file);
        if cookies.is_empty() {
            return Err(AuthError::Rejected(
                "no Microsoft login cookies (login.live.com) found in that file".into(),
            ));
        }
        // JSHP, then JSH — whichever yields a token.
        let mut token = None;
        for use_jsh in [false, true] {
            if token.is_none() {
                token = rps_token_from_cookies(&cookies, use_jsh).await?;
            }
        }
        let token = token.ok_or_else(|| {
            AuthError::Rejected(
                "those cookies didn't produce a Microsoft token — they're likely expired; \
                 sign in at login.live.com in your browser and export fresh cookies"
                    .into(),
            )
        })?;
        let session = minecraft_login(&format!("d={token}")).await?;
        self.store_cookie(&session)
            .map_err(|e| AuthError::Rejected(format!("could not save the account: {e}")))?;
        Ok(session)
    }

    /// Saves a cookie-imported account: like `store`, but with no refresh token.
    fn store_cookie(&self, session: &McSession) -> std::io::Result<()> {
        self.modify(|accounts| {
            accounts.retain(|a| text(a, "uuid").as_deref() != Some(&session.uuid));
            accounts.push(json!({
                "name": session.name,
                "uuid": session.uuid,
                "ms_refresh_token": "",
                "mc_access_token": session.access_token,
                "mc_expires_at": session.expires_at,
            }));
        })
    }

    /// A valid session for the account named `name`, refreshing tokens if needed.
    pub async fn session_for(&self, name: &str) -> AuthResult<McSession> {
        let account = self
            .load()
            .map_err(|e| AuthError::Rejected(format!("could not read the accounts: {e}")))?
            .into_iter()
            .find(|a| has_name(a, name))
            .ok_or_else(|| AuthError::NoAccount(name.to_string()))?;
        let cached = McSession {
            access_token: text(&account, "mc_access_token").unwrap_or_default(),
            expires_at: account
                .get("mc_expires_at")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            uuid: text(&account, "uuid").unwrap_or_default(),
            name: text(&account, "name").unwrap_or_default(),
        };
        if !cached.access_token.is_empty() && cached.expires_at > now() + EXPIRY_MARGIN {
            return Ok(cached);
        }
        let refresh = text(&account, "ms_refresh_token")
            .ok_or_else(|| AuthError::NoAccount(name.to_string()))?;
        if refresh.is_empty() {
            // A cookie-imported account has no refresh token; it can't be renewed.
            return Err(AuthError::Rejected(format!(
                "{name}'s session was imported from cookies and has expired — \
                 re-import the cookie file with `jagprox account cookie <file>`"
            )));
        }
        let ms = refresh_ms_token(&refresh).await?;
        let session = minecraft_login(&ms.access_token).await?;
        self.store(&ms, &session)
            .map_err(|e| AuthError::Rejected(format!("could not save the account: {e}")))?;
        Ok(session)
    }
}

fn has_name(account: &Value, name: &str) -> bool {
    text(account, "name").is_some_and(|n| n.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_parsing_and_header() {
        // Netscape cookies.txt: domain, flag, path, secure, expiry, name, value.
        let file = "\
# comment, skipped
.live.com\tTRUE\t/\tTRUE\t0\tJSHP\tabc123
login.live.com\tTRUE\t/\tTRUE\t0\tMSPAuth\ttok+val
example.com\tTRUE\t/\tTRUE\t0\tirrelevant\tnope
.live.com\tTRUE\t/\tTRUE\t0\tDead\tDisabled
";
        let cookies = parse_microsoft_cookies(file);
        assert_eq!(cookies.get("JSHP").map(String::as_str), Some("abc123"));
        assert_eq!(cookies.get("MSPAuth").map(String::as_str), Some("tok+val"));
        assert!(!cookies.contains_key("irrelevant")); // wrong domain dropped
        assert!(!cookies.contains_key("Dead")); // "Disabled" value dropped
        // JSHP must lead, in fixed order, with the "; " separator.
        assert_eq!(cookie_header(&cookies, false), "JSHP=abc123; MSPAuth=tok+val");
        assert_eq!(percent_decode("a%2Bb+c%2F"), "a+b c/");
    }

    #[test]
    fn device_code_link_prefills_the_code() {
        let code = DeviceCode {
            user_code: "ABCD1234".into(),
            verification_uri: "https://www.microsoft.com/link".into(),
            device_code: String::new(),
            interval: 5,
            expires_at: 0,
        };
        assert_eq!(code.link(), "https://www.microsoft.com/link?otc=ABCD1234");
    }

    /// Asks Microsoft for a real sign-in code (nothing is signed in). Run with
    /// `cargo test -p jagprox-core -- --ignored live_device_code`.
    #[tokio::test]
    #[ignore = "talks to login.live.com"]
    async fn live_device_code() {
        let code = start_device_code().await.unwrap();
        assert!(!code.user_code.is_empty());
        assert!(
            code.verification_uri.starts_with("https://"),
            "{}",
            code.verification_uri
        );
        println!(
            "got code {} for {}",
            code.user_code.len(),
            code.verification_uri
        );
    }

    #[test]
    fn account_store_round_trip() {
        let dir = std::env::temp_dir().join(format!("jagprox-accounts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let accounts = Accounts::new(&dir);
        let ms = MsToken {
            access_token: "a".into(),
            refresh_token: "r".into(),
        };
        let session = McSession {
            access_token: "mc".into(),
            expires_at: now() + 3600,
            uuid: "u1".into(),
            name: "Steve".into(),
        };
        accounts.store(&ms, &session).unwrap();
        assert_eq!(accounts.names(), vec!["Steve"]);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            rt.block_on(accounts.session_for("steve")).unwrap(),
            session,
            "valid cached token, case-insensitive"
        );
        assert!(matches!(
            rt.block_on(accounts.session_for("Alex")),
            Err(AuthError::NoAccount(_))
        ));
        assert!(accounts.remove("STEVE").unwrap());
        assert!(!accounts.remove("STEVE").unwrap());
        assert!(accounts.names().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_broken_account_file_is_never_overwritten() {
        let dir =
            std::env::temp_dir().join(format!("jagprox-accounts-broken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let accounts = Accounts::new(&dir);
        let broken = b"{\"accounts\": [{\"name\": \"Alex\", \"ms_refresh_token\": \"keep";
        std::fs::write(&accounts.path, broken).unwrap();
        let ms = MsToken {
            access_token: "a".into(),
            refresh_token: "r".into(),
        };
        let session = McSession {
            access_token: "mc".into(),
            expires_at: 0,
            uuid: "u1".into(),
            name: "Steve".into(),
        };
        assert!(accounts.store(&ms, &session).is_err());
        assert!(accounts.remove("Alex").is_err());
        assert_eq!(std::fs::read(&accounts.path).unwrap(), broken);
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.to_string_lossy().ends_with(".bak"))
            .collect();
        assert!(!backups.is_empty());
        assert_eq!(std::fs::read(&backups[0]).unwrap(), broken);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
