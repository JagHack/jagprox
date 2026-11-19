//! Hypixel, Mojang and JagProx backend clients. Port of `modules/hypixelHandler.js`
//! (data layer), `utils/apiHandler.js` and `modules/gametrackApiHandler.js`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use serde_json::{Value, json};

use crate::cache::TtlCache;
use crate::formatter;
use crate::http::client;
use crate::log::log;

pub const API_BASE_URL: &str = "https://jagprox.jaghack.com/api/v1";
pub const WEB_BASE_URL: &str = "https://jagprox.jaghack.com";
const HYPIXEL_API: &str = "https://api.hypixel.net/v2";

const API_KEY_TTL: Duration = Duration::from_secs(60);
const NOTICE_COOLDOWN: Duration = Duration::from_secs(30);
/// Bulk lookups (party, queue, leaderboard) run at most this many requests at once.
pub const LOOKUP_CONCURRENCY: usize = 4;

/// Why a request failed. Cloneable so cached in-flight loads can share it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// No JagProx login (JWT).
    NotLoggedIn,
    /// The JagProx account has no Hypixel API key.
    NoApiKey,
    /// Hypixel rejected the API key (expired or wrong).
    InvalidApiKey,
    RateLimited(&'static str),
    /// Non-success HTTP status (with the server's message, if it sent one).
    Status(u16, Option<String>),
    /// Hypixel answered `success: false`.
    Hypixel(String),
    Network(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::NotLoggedIn => write!(f, "You are not logged in to JagProx."),
            ApiError::NoApiKey => write!(f, "No Hypixel API key is set for your account."),
            ApiError::InvalidApiKey => {
                write!(f, "Hypixel rejected the API key (invalid or expired).")
            }
            ApiError::RateLimited(service) => write!(
                f,
                "{service} API rate limit reached. Try again in a minute."
            ),
            ApiError::Status(_, Some(message)) => write!(f, "{message}"),
            ApiError::Status(code, None) => write!(f, "API returned status {code}"),
            ApiError::Hypixel(cause) => write!(f, "Hypixel API error: {cause}"),
            ApiError::Network(e) => write!(f, "Network error: {e}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Network(e.without_url().to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Reads a JSON body, tolerating non-JSON error pages.
async fn json_body(response: reqwest::Response) -> (u16, Option<Value>) {
    let status = response.status().as_u16();
    let body = response.json::<Value>().await.ok();
    (status, body)
}

/// JS `parseResponse`: the body on success, or an error carrying the server's message.
async fn parse_response(response: reqwest::Response) -> ApiResult<Value> {
    let ok = response.status().is_success();
    let (status, body) = json_body(response).await;
    if !ok {
        let message = body
            .as_ref()
            .and_then(|b| b.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string);
        return Err(ApiError::Status(status, message));
    }
    Ok(body.unwrap_or(Value::Null))
}

// ---------------------------------------------------------------------------
// JagProx backend
// ---------------------------------------------------------------------------

/// The JagProx backend, authenticated with the user's JWT.
#[derive(Clone)]
pub struct Backend {
    base_url: String,
    jwt: Option<String>,
}

impl Backend {
    pub fn new(jwt: Option<String>) -> Self {
        Self {
            base_url: API_BASE_URL.to_string(),
            jwt: jwt.filter(|j| !j.trim().is_empty()),
        }
    }

    pub fn jwt(&self) -> Option<&str> {
        self.jwt.as_deref()
    }

    fn auth(&self) -> ApiResult<&str> {
        self.jwt.as_deref().ok_or(ApiError::NotLoggedIn)
    }

    async fn get(&self, path: &str) -> ApiResult<Value> {
        let response = client()
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(self.auth()?)
            .send()
            .await?;
        parse_response(response).await
    }

    async fn post(&self, path: &str, body: &Value) -> ApiResult<Value> {
        let response = client()
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(self.auth()?)
            .json(body)
            .send()
            .await?;
        parse_response(response).await
    }

    /// The Hypixel API key stored for this account, `None` if none is set (404).
    pub async fn get_api_key(&self) -> ApiResult<Option<String>> {
        let response = client()
            .get(format!("{}/user/api-key", self.base_url))
            .bearer_auth(self.auth()?)
            .send()
            .await?;
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        let data = parse_response(response).await?;
        Ok(data
            .get("apiKey")
            .and_then(Value::as_str)
            .filter(|k| !k.is_empty())
            .map(str::to_string))
    }

    pub async fn save_api_key(&self, api_key: &str) -> ApiResult<Value> {
        self.post("/user/api-key", &json!({ "apiKey": api_key }))
            .await
    }

    /// `{ username, email, globalRank }`.
    pub async fn profile(&self) -> ApiResult<Value> {
        self.get("/user/profile").await
    }

    /// Email/password login for terminals; the token is valid for 90 days.
    pub async fn cli_login(email: &str, password: &str) -> ApiResult<String> {
        let response = client()
            .post(format!("{API_BASE_URL}/cli-login"))
            .json(&json!({ "email": email, "password": password }))
            .send()
            .await?;
        let data = parse_response(response).await?;
        data.get("token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError::Status(200, Some("The server did not return a token.".into())))
    }

    pub async fn gametrack_start(&self, mc_uuid: &str, mode: &str) -> ApiResult<Value> {
        self.post(
            "/gametrack/start",
            &json!({ "mc_uuid": mc_uuid, "mode": mode }),
        )
        .await
    }

    pub async fn gametrack_event(
        &self,
        mc_uuid: &str,
        mode: &str,
        result: &str,
    ) -> ApiResult<Value> {
        self.post(
            "/gametrack/event",
            &json!({ "mc_uuid": mc_uuid, "mode": mode, "result": result }),
        )
        .await
    }

    /// Wins/losses per Minecraft account per mode: `{ mc_uuid: { mode: { wins, losses } } }`.
    pub async fn gametrack_hours(&self, hours: u32) -> ApiResult<Value> {
        self.get(&format!("/gametrack/hour?hours={hours}")).await
    }

    /// Same shape as [`Self::gametrack_hours`], since local midnight on the server.
    pub async fn gametrack_day(&self) -> ApiResult<Value> {
        self.get("/gametrack/day").await
    }

    /// Every tracked game, newest first: `[{ mc_uuid, mode, result, timestamp }]`.
    pub async fn gametrack_log(&self) -> ApiResult<Value> {
        self.get("/gametrack/log").await
    }

    pub async fn link_challenge(&self) -> ApiResult<Value> {
        self.post("/link/challenge", &json!({})).await
    }

    pub async fn link_verify(&self, mc_username: &str, server_id: &str) -> ApiResult<Value> {
        self.post(
            "/link/verify",
            &json!({ "mc_username": mc_username, "server_id": server_id }),
        )
        .await
    }

    /// Opt-in stats telemetry (`stats_telemetry: true`).
    pub async fn send_stat_telemetry(
        &self,
        username: &str,
        mode: &str,
        win_count: f64,
    ) -> ApiResult<()> {
        let response = client()
            .post(format!("{WEB_BASE_URL}/api/ingest"))
            .bearer_auth(self.auth()?)
            .json(&json!({ "username": username, "mode": mode, "win_count": win_count }))
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(ApiError::Status(status.as_u16(), None))
        }
    }
}

// ---------------------------------------------------------------------------
// Hypixel + Mojang
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MojangProfile {
    /// Undashed UUID as Mojang returns it.
    pub uuid: String,
    /// Correctly capitalised current name.
    pub username: String,
}

/// A Hypixel player object plus its resolved rank.
#[derive(Debug, Clone)]
pub struct PlayerData {
    pub player: Arc<Value>,
    pub rank: String,
}

#[derive(Debug, Clone)]
pub enum OnlineStatus {
    Online {
        game_type: Option<String>,
        mode: Option<String>,
        map: Option<String>,
    },
    /// Logged in more recently than out, but the status API hides the session.
    Hidden,
    Offline,
}

#[derive(Debug, Clone)]
pub struct PlayerStatus {
    pub status: OnlineStatus,
    pub data: PlayerData,
}

/// Face (and hat) layer of a skin; each pixel is RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub face: [[[u8; 4]; 8]; 8],
    pub hat: [[[u8; 4]; 8]; 8],
}

impl Head {
    /// Hat over face where the hat pixel's alpha is above `hat_alpha`.
    pub fn composite(&self, hat_alpha: u8) -> [[[u8; 4]; 8]; 8] {
        let mut out = self.face;
        for (y, row) in out.iter_mut().enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                if self.hat[y][x][3] > hat_alpha {
                    *pixel = self.hat[y][x];
                }
            }
        }
        out
    }

    /// The in-game `/sc` avatar: 8 lines of `§x█`, transparent pixels as spaces.
    pub fn chat_lines(&self) -> Vec<String> {
        self.composite(128)
            .iter()
            .map(|row| {
                row.iter()
                    .map(|[r, g, b, a]| {
                        if *a > 128 {
                            format!(
                                "{}█",
                                crate::constants::find_closest_minecraft_color(*r, *g, *b)
                            )
                        } else {
                            " ".into()
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

/// Something that can show a notice to the user: chat in game, stderr in the terminal.
pub type Notifier = Arc<dyn Fn(&str) + Send + Sync>;

type KeyLoad = Shared<BoxFuture<'static, ApiResult<String>>>;

#[derive(Default)]
struct KeyState {
    cached: Option<(String, Instant)>,
    in_flight: Option<KeyLoad>,
}

/// Hypixel/Mojang data with the same caching as the JS handler. Cheap to clone.
#[derive(Clone)]
pub struct Hypixel {
    inner: Arc<Inner>,
}

struct Inner {
    backend: Backend,
    /// Fixed key (e.g. `HYPIXEL_API_KEY`); bypasses the backend.
    fixed_api_key: Option<String>,
    notifier: Option<Notifier>,
    key: Mutex<KeyState>,
    last_notice: Mutex<HashMap<&'static str, Instant>>,
    uuids: TtlCache<String, MojangProfile, ApiError>,
    profiles: TtlCache<String, Arc<Value>, ApiError>,
    stats: TtlCache<String, PlayerData, ApiError>,
    guilds: TtlCache<String, String, ApiError>,
    heads: TtlCache<String, Arc<Head>, ApiError>,
}

impl Hypixel {
    pub fn new(
        backend: Backend,
        fixed_api_key: Option<String>,
        notifier: Option<Notifier>,
    ) -> Self {
        let hour = Duration::from_secs(60 * 60);
        Self {
            inner: Arc::new(Inner {
                backend,
                fixed_api_key: fixed_api_key.filter(|k| !k.trim().is_empty()),
                notifier,
                key: Mutex::new(KeyState::default()),
                last_notice: Mutex::new(HashMap::new()),
                uuids: TtlCache::new(hour, 2000),
                profiles: TtlCache::new(hour, 2000),
                stats: TtlCache::new(Duration::from_secs(5 * 60), 500),
                guilds: TtlCache::new(Duration::from_secs(10 * 60), 500),
                heads: TtlCache::new(hour, 200),
            }),
        }
    }

    pub fn backend(&self) -> &Backend {
        &self.inner.backend
    }

    /// Shows `message` at most once per cooldown window per `key`.
    fn notify_once(&self, key: &'static str, message: &str) {
        let Some(notifier) = &self.inner.notifier else {
            return;
        };
        {
            let mut last = self
                .inner
                .last_notice
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            if last
                .get(key)
                .is_some_and(|t| now.duration_since(*t) < NOTICE_COOLDOWN)
            {
                return;
            }
            last.insert(key, now);
        }
        notifier(message);
    }

    /// The Hypixel API key: fixed, cached for 60 s, or fetched from the backend with
    /// one shared request for all concurrent callers.
    pub async fn api_key(&self) -> ApiResult<String> {
        if let Some(key) = &self.inner.fixed_api_key {
            return Ok(key.clone());
        }
        let load = {
            let mut state = self.inner.key.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((key, at)) = &state.cached
                && at.elapsed() < API_KEY_TTL
            {
                return Ok(key.clone());
            }
            match &state.in_flight {
                Some(load) => load.clone(),
                None => {
                    let backend = self.inner.backend.clone();
                    let load = async move {
                        match backend.get_api_key().await {
                            Ok(Some(key)) => Ok(key),
                            Ok(None) => Err(ApiError::NoApiKey),
                            Err(e) => Err(e),
                        }
                    }
                    .boxed()
                    .shared();
                    state.in_flight = Some(load.clone());
                    load
                }
            }
        };

        let result = load.await;
        {
            let mut state = self.inner.key.lock().unwrap_or_else(|e| e.into_inner());
            state.in_flight = None;
            if let Ok(key) = &result {
                state.cached = Some((key.clone(), Instant::now()));
            }
        }
        match &result {
            Err(ApiError::NoApiKey) => self.notify_once(
                "api-key",
                "§cNo Hypixel API key is set for your account. Add one in the launcher settings.",
            ),
            Err(ApiError::NotLoggedIn) => {}
            Err(e) => self.notify_once("api-key", &format!("§cError fetching API Key: {e}")),
            Ok(_) => {}
        }
        result
    }

    /// GET a Hypixel v2 endpoint; the parsed body when `success` is true.
    pub async fn hypixel_get(&self, endpoint: &str, params: &[(&str, &str)]) -> ApiResult<Value> {
        let key = self.api_key().await?;
        let response = client()
            .get(format!("{HYPIXEL_API}/{endpoint}"))
            .query(params)
            .header("API-Key", key)
            .send()
            .await
            .map_err(ApiError::from)
            .inspect_err(|e| log(format!("Hypixel /{endpoint} error: {e}")))?;
        let status = response.status().as_u16();
        if status == 429 {
            self.notify_once(
                "hypixel-429",
                "§cHypixel API rate limit reached. Try again in a minute.",
            );
            return Err(ApiError::RateLimited("Hypixel"));
        }
        let (_, body) = json_body(response).await;
        if !(200..300).contains(&status) {
            log(format!("Hypixel /{endpoint} returned {status}"));
            if status == 403 {
                return Err(ApiError::InvalidApiKey);
            }
            let cause = body
                .as_ref()
                .and_then(|b| b.get("cause"))
                .and_then(Value::as_str);
            return Err(ApiError::Hypixel(
                cause.map_or_else(|| format!("HTTP {status}"), str::to_string),
            ));
        }
        let body = body.unwrap_or(Value::Null);
        if body.get("success").and_then(Value::as_bool) == Some(true) {
            Ok(body)
        } else {
            let cause = body
                .get("cause")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            Err(ApiError::Hypixel(cause.to_string()))
        }
    }

    /// Name → UUID via Mojang, cached for an hour by lowercase name. `None` if the
    /// name doesn't exist.
    pub async fn mojang_uuid(&self, username: &str) -> ApiResult<Option<MojangProfile>> {
        if username.is_empty() {
            return Ok(None);
        }
        let this = self.clone();
        let name = username.to_string();
        self.inner
            .uuids
            .get_or_load(username.to_lowercase(), async move {
                let mut url =
                    reqwest::Url::parse("https://api.mojang.com/users/profiles/minecraft/")
                        .expect("valid");
                url.path_segments_mut()
                    .expect("base")
                    .pop_if_empty()
                    .push(&name);
                let response = client().get(url).send().await?;
                match response.status().as_u16() {
                    200 => {
                        let data: Value = response.json().await?;
                        let field =
                            |k: &str| data.get(k).and_then(Value::as_str).map(str::to_string);
                        Ok(field("id")
                            .zip(field("name"))
                            .map(|(uuid, username)| MojangProfile { uuid, username }))
                    }
                    204 | 404 => Ok(None),
                    429 => {
                        this.notify_once("mojang-429", "§cMojang API rate limit reached.");
                        Err(ApiError::RateLimited("Mojang"))
                    }
                    code => Err(ApiError::Status(code, None)),
                }
            })
            .await
    }

    /// Mojang session profile (current name + skin textures), cached for an hour.
    pub async fn session_profile(&self, uuid: &str) -> ApiResult<Option<Arc<Value>>> {
        let id = uuid.to_string();
        self.inner
            .profiles
            .get_or_load(uuid.to_string(), async move {
                let mut url = reqwest::Url::parse(
                    "https://sessionserver.mojang.com/session/minecraft/profile/",
                )
                .expect("valid");
                url.path_segments_mut()
                    .expect("base")
                    .pop_if_empty()
                    .push(&id);
                let response = client().get(url).send().await?;
                if !response.status().is_success() {
                    return Ok(None);
                }
                Ok(Some(Arc::new(response.json::<Value>().await?)))
            })
            .await
    }

    pub async fn username_from_uuid(&self, uuid: &str) -> Option<String> {
        let profile = self.session_profile(uuid).await.ok()??;
        profile
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    async fn skin_url(&self, uuid: &str) -> Option<String> {
        let profile = self.session_profile(uuid).await.ok()??;
        let textures = profile
            .get("properties")?
            .as_array()?
            .iter()
            .find(|p| p.get("name").and_then(Value::as_str) == Some("textures"))?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(textures.get("value")?.as_str()?)
            .ok()?;
        let data: Value = serde_json::from_slice(&decoded).ok()?;
        data.pointer("/textures/SKIN/url")?
            .as_str()
            .map(str::to_string)
    }

    /// Player data, cached for 5 minutes. `None` if Hypixel has never seen the player.
    pub async fn stats(&self, uuid: &str) -> ApiResult<Option<PlayerData>> {
        if uuid.is_empty() {
            return Ok(None);
        }
        let this = self.clone();
        let id = uuid.to_string();
        self.inner
            .stats
            .get_or_load(uuid.to_string(), async move {
                let mut data = this.hypixel_get("player", &[("uuid", &id)]).await?;
                let player = data
                    .get_mut("player")
                    .map(Value::take)
                    .filter(|p| !p.is_null());
                Ok(player.map(|player| {
                    let rank = formatter::get_rank(Some(&player));
                    PlayerData {
                        player: Arc::new(player),
                        rank,
                    }
                }))
            })
            .await
    }

    /// Guild tag like `[TAG]` (or `[Name]` without a tag); empty string for no guild.
    pub async fn guild(&self, uuid: &str) -> ApiResult<String> {
        let this = self.clone();
        let id = uuid.to_string();
        let tag = self
            .inner
            .guilds
            .get_or_load(uuid.to_string(), async move {
                let data = this.hypixel_get("guild", &[("player", &id)]).await?;
                let Some(guild) = data.get("guild").filter(|g| !g.is_null()) else {
                    return Ok(Some(String::new()));
                };
                let text = |k: &str| {
                    guild
                        .get(k)
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                };
                Ok(Some(match text("tag") {
                    Some(tag) => format!("[{tag}]"),
                    None => format!("[{}]", text("name").unwrap_or("")),
                }))
            })
            .await?;
        Ok(tag.unwrap_or_default())
    }

    /// `/status` session object (uncached; used for polling).
    pub async fn session_status(&self, uuid: &str) -> ApiResult<Value> {
        let mut data = self.hypixel_get("status", &[("uuid", uuid)]).await?;
        Ok(data
            .get_mut("session")
            .map(Value::take)
            .unwrap_or(Value::Null))
    }

    /// Online status plus player data. `None` if Hypixel has no data for the player.
    pub async fn player_status(&self, uuid: &str) -> ApiResult<Option<PlayerStatus>> {
        let (session, data) = tokio::join!(self.session_status(uuid), self.stats(uuid));
        let Some(data) = data? else { return Ok(None) };
        let session = session.ok();
        let text = |k: &str| {
            session
                .as_ref()
                .and_then(|s| s.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let status = if session
            .as_ref()
            .and_then(|s| s.get("online"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            OnlineStatus::Online {
                game_type: text("gameType"),
                mode: text("mode"),
                map: text("map"),
            }
        } else if crate::js::field(Some(&data.player), "lastLogin")
            > crate::js::field(Some(&data.player), "lastLogout")
        {
            OnlineStatus::Hidden
        } else {
            OnlineStatus::Offline
        };
        Ok(Some(PlayerStatus { status, data }))
    }

    /// `/counts` → `games`.
    pub async fn player_counts(&self) -> ApiResult<Value> {
        let mut data = self.hypixel_get("counts", &[]).await?;
        Ok(data
            .get_mut("games")
            .map(Value::take)
            .unwrap_or(Value::Null))
    }

    /// `/leaderboards` (uncached, as in the original).
    pub async fn leaderboards(&self) -> ApiResult<Value> {
        let mut data = self.hypixel_get("leaderboards", &[]).await?;
        Ok(data
            .get_mut("leaderboards")
            .map(Value::take)
            .unwrap_or(Value::Null))
    }

    /// The player's 8×8 head from their skin, falling back to Minotar. Cached an hour.
    pub async fn head(&self, uuid: &str) -> ApiResult<Option<Arc<Head>>> {
        let this = self.clone();
        let id = uuid.to_string();
        self.inner
            .heads
            .get_or_load(uuid.to_string(), async move {
                let from_skin = async {
                    let url = this.skin_url(&id).await.ok_or("no skin URL found")?;
                    let bytes = fetch_bytes(&url).await?;
                    let skin = decode_rgba(&bytes)?;
                    if skin.width < 48 || skin.height < 16 {
                        return Err("skin image is too small".to_string());
                    }
                    Ok::<_, String>(Head {
                        face: skin.block(8, 8),
                        hat: skin.block(40, 8),
                    })
                };
                match from_skin.await {
                    Ok(head) => Ok(Some(Arc::new(head))),
                    Err(e) => {
                        log(format!(
                            "Skin processing failed: {e}. Falling back to Minotar."
                        ));
                        let bytes = fetch_bytes(&format!("https://minotar.net/helm/{id}/8.png"))
                            .await
                            .map_err(ApiError::Network)?;
                        let image = decode_rgba(&bytes).map_err(ApiError::Network)?;
                        if image.width < 8 || image.height < 8 {
                            return Ok(None);
                        }
                        Ok(Some(Arc::new(Head {
                            face: image.block(0, 0),
                            hat: [[[0; 4]; 8]; 8],
                        })))
                    }
                }
            })
            .await
    }
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = client().get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {} for {url}", response.status().as_u16()));
    }
    Ok(response.bytes().await.map_err(|e| e.to_string())?.to_vec())
}

struct Rgba {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Rgba {
    fn block(&self, x0: usize, y0: usize) -> [[[u8; 4]; 8]; 8] {
        let mut out = [[[0u8; 4]; 8]; 8];
        for (y, row) in out.iter_mut().enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                let i = ((y0 + y) * self.width + x0 + x) * 4;
                pixel.copy_from_slice(&self.data[i..i + 4]);
            }
        }
        out
    }
}

/// Decodes any PNG to 8-bit RGBA, like pngjs does.
fn decode_rgba(bytes: &[u8]) -> Result<Rgba, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (width, height) = (info.width as usize, info.height as usize);
    let data = match info.color_type {
        png::ColorType::Rgba => buf[..width * height * 4].to_vec(),
        png::ColorType::GrayscaleAlpha => buf[..width * height * 2]
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        other => return Err(format!("unexpected PNG color type {other:?}")),
    };
    Ok(Rgba {
        width,
        height,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                data.extend(pixel(x, y));
            }
        }
        writer.write_image_data(&data).unwrap();
        drop(writer);
        out
    }

    #[test]
    fn head_composites_hat_over_face() {
        // Face red everywhere, hat opaque blue only at (0,0).
        let bytes = png_bytes(64, 64, |x, y| {
            if (8..16).contains(&x) && (8..16).contains(&y) {
                [255, 0, 0, 255]
            } else if x == 40 && y == 8 {
                [0, 0, 255, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let skin = decode_rgba(&bytes).unwrap();
        let head = Head {
            face: skin.block(8, 8),
            hat: skin.block(40, 8),
        };
        let lines = head.chat_lines();
        assert_eq!(lines.len(), 8);
        assert!(lines[0].starts_with("§1█§4█"), "{}", lines[0]);
        assert_eq!(lines[1], "§4█".repeat(8));
    }
}
