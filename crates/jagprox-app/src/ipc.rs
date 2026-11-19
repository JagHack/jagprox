//! The launcher pages' requests, one per channel of the old Electron IPC
//! (`launcher/launcher.js`). Replies go back as events with the old channel names.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use jagprox_core::account;
use jagprox_core::api::{Backend, Hypixel, OnlineStatus};
use jagprox_core::config::Config;
use jagprox_core::constants::{self, GAME_MODES};
use jagprox_core::formatter::{format_rank, player_name_color};
use jagprox_core::install::{self, Install};
use jagprox_core::log::log;
use jagprox_core::proxy::{Proxy, ProxyEvent, ProxyOptions};
use jagprox_core::stat_format;
use serde_json::{Map, Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::discord::Discord;
use crate::updater::{self, Check};

pub struct AppState {
    pub data_dir: PathBuf,
    pub config: Config,
    pub discord: Discord,
    /// JWT and display name of the logged-in JagProx account.
    session: Mutex<Option<(String, String)>>,
    login: Mutex<Option<tokio::task::AbortHandle>>,
    proxy: Mutex<Option<(Arc<Proxy>, tokio::task::AbortHandle)>>,
    hypixel: Mutex<Option<Hypixel>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, config: Config, discord: Discord) -> Self {
        Self {
            data_dir,
            config,
            discord,
            session: Mutex::new(None),
            login: Mutex::new(None),
            proxy: Mutex::new(None),
            hypixel: Mutex::new(None),
        }
    }

    fn jwt(&self) -> Option<String> {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(jwt, _)| jwt.clone())
    }

    /// Stats lookups for the Stat Search / Status pages, with the account's API key.
    fn hypixel(&self) -> Option<Hypixel> {
        let jwt = self.jwt()?;
        let mut h = self.hypixel.lock().unwrap_or_else(|e| e.into_inner());
        Some(
            h.get_or_insert_with(|| Hypixel::new(Backend::new(Some(jwt)), None, None))
                .clone(),
        )
    }

    pub fn stop_proxy(&self) {
        if let Some((proxy, task)) = self.proxy.lock().unwrap_or_else(|e| e.into_inner()).take() {
            task.abort();
            proxy.shutdown();
        }
    }
}

fn emit(app: &AppHandle, channel: &str, payload: impl serde::Serialize + Clone) {
    let _ = app.emit(channel, payload);
}

/// A log line in the launcher's Log Monitor.
pub fn log_line(app: &AppHandle, line: &str) {
    emit(app, "proxy-log", line);
}

#[tauri::command]
pub async fn ipc(app: AppHandle, channel: String, payload: Option<Value>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let payload = payload.unwrap_or(Value::Null);
    match channel.as_str() {
        "minimize-window" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.minimize();
            }
        }
        "maximize-window" => {
            if let Some(w) = app.get_webview_window("main") {
                if w.is_maximized().unwrap_or(false) {
                    let _ = w.unmaximize();
                } else {
                    let _ = w.maximize();
                }
            }
        }
        "close-window" => {
            state.stop_proxy();
            state.discord.set_enabled(false);
            app.exit(0);
        }
        "start-login" => start_login(&app),
        "restore-session" => restore_session(&app).await,
        "clear-jwt" => {
            account::clear();
            *state.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *state.hypixel.lock().unwrap_or_else(|e| e.into_inner()) = None;
            log("Logged out.");
        }
        "get-app-version" => emit(&app, "app-version", env!("CARGO_PKG_VERSION")),
        "check-for-updates" => {
            emit(&app, "update-status", "Checking for updates...");
            if let Check::Nothing(text) = updater::check(&app).await {
                emit(&app, "update-status", text);
            }
        }
        "update-response" => {
            if updater::respond(&app, payload.as_str() == Some("update")).await {
                // Installed: make way for the new version.
                state.stop_proxy();
                state.discord.set_enabled(false);
                app.exit(0);
            } else if app.get_webview_window("main").is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                crate::open_main(&app);
            }
        }
        "get-cli-status" => {
            let install = install::detect();
            let offer = matches!(install, Install::AppImage { .. } | Install::MacApp { .. });
            let installed = offer
                && install::cli_target(&install)
                    .ok()
                    .zip(install::link_path())
                    .is_some_and(|(target, link)| {
                        std::fs::canonicalize(link).ok() == std::fs::canonicalize(target).ok()
                    });
            emit(
                &app,
                "cli-status",
                json!({ "offer": offer, "installed": installed }),
            );
        }
        "install-cli" => {
            let reply = match install_cli() {
                Ok(message) => json!({ "ok": true, "message": message }),
                Err(message) => json!({ "ok": false, "message": message }),
            };
            emit(&app, "cli-install-reply", reply);
        }
        "get-config" => {
            let config = &state.config;
            emit(
                &app,
                "config-loaded",
                json!({
                    "auto_gg": config.get("auto_gg").unwrap_or_else(|| json!({})),
                    "discord_rpc": config.get("discord_rpc").unwrap_or_else(|| json!({})),
                }),
            );
        }
        "save-settings" => {
            let auto_gg = payload.get("auto_gg").cloned().unwrap_or_default();
            let delay = auto_gg
                .get("delay")
                .and_then(Value::as_i64)
                .map_or(1500, |d| d.clamp(0, 60000));
            let message: String = auto_gg
                .get("message")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty())
                .unwrap_or("gg")
                .chars()
                .take(100)
                .collect();
            let enabled = auto_gg.get("enabled").and_then(Value::as_bool) == Some(true);
            let ok = state
                .config
                .update(&["auto_gg"], |c| {
                    c.insert(
                        "auto_gg".into(),
                        json!({ "enabled": enabled, "message": message, "delay": delay }),
                    );
                })
                .inspect_err(|e| log(format!("Failed to save settings: {e}")))
                .is_ok();
            emit(&app, "settings-saved-reply", ok);
        }
        "toggle-discord-rpc" => {
            let enabled = payload.as_bool() == Some(true);
            state.discord.set_enabled(enabled);
            set_discord_setting(&state.config, enabled);
        }
        "save-api-key" => {
            let key = payload.as_str().unwrap_or("").trim().to_string();
            let ok = match state.jwt() {
                None => {
                    log("Cannot save API key without being logged in.");
                    false
                }
                Some(jwt) => match Backend::new(Some(jwt)).save_api_key(&key).await {
                    Ok(_) => {
                        log("Hypixel API key saved to backend.");
                        *state.hypixel.lock().unwrap_or_else(|e| e.into_inner()) = None;
                        true
                    }
                    Err(e) => {
                        log(format!("Failed to save API key to backend: {e}"));
                        false
                    }
                },
            };
            emit(&app, "api-key-saved-reply", ok);
        }
        "get-api-key-status" => {
            // Only whether a key is set; the key itself never goes to the page.
            let set = match state.jwt() {
                Some(jwt) => Backend::new(Some(jwt))
                    .get_api_key()
                    .await
                    .ok()
                    .flatten()
                    .is_some(),
                None => false,
            };
            emit(&app, "api-key-status", set);
        }
        "get-aliases" => {
            let aliases = std::fs::read_to_string(state.data_dir.join("aliases.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
                .unwrap_or_else(|| json!({}));
            emit(&app, "aliases-loaded", aliases);
        }
        "save-aliases" => {
            let mut clean = Map::new();
            for (key, value) in payload.as_object().cloned().unwrap_or_default() {
                let from = key.trim().to_lowercase();
                let to = value.as_str().unwrap_or("").trim().to_string();
                if from.starts_with('/') && to.starts_with('/') {
                    clean.insert(from, json!(to));
                }
            }
            let text = serde_json::to_string_pretty(&Value::Object(clean)).unwrap_or_default();
            let ok = jagprox_core::file_util::write_file_atomic(
                &state.data_dir.join("aliases.json"),
                &text,
            )
            .inspect_err(|e| log(format!("Failed to save aliases.json: {e}")))
            .is_ok();
            emit(&app, "aliases-saved-reply", ok);
        }
        "get-gamemode-list" => {
            let mut modes: Vec<(&str, &str)> = Vec::new();
            for (key, info) in GAME_MODES {
                if !modes.iter().any(|(name, _)| *name == info.display_name) {
                    modes.push((info.display_name, key));
                }
            }
            modes.sort_by_key(|(name, _)| name.to_lowercase());
            let list: Vec<Value> = modes
                .iter()
                .map(|(text, value)| json!({ "text": text, "value": value }))
                .collect();
            emit(&app, "gamemode-list-response", list);
        }
        "get-player-stats" => {
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mode = payload
                .get("gamemode")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            emit(
                &app,
                "player-stats-result",
                player_stats(&state, &name, &mode).await,
            );
        }
        "get-player-status" => {
            let name = payload.as_str().unwrap_or("").to_string();
            emit(
                &app,
                "player-status-result",
                player_status(&state, &name).await,
            );
        }
        "toggle-proxy" => {
            if payload.get("start").and_then(Value::as_bool) == Some(true) {
                start_proxy(&app);
            } else {
                state.stop_proxy();
                log_line(&app, "[SYSTEM] Proxy stopped.");
                emit(&app, "proxy-status", "stopped");
                state.discord.set_playing(false);
            }
        }
        other => return Err(format!("unknown channel {other}")),
    }
    Ok(())
}

fn set_discord_setting(config: &Config, enabled: bool) {
    let result = config.update(&["discord_rpc"], |c| {
        let entry = c.entry("discord_rpc").or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        entry["enabled"] = json!(enabled);
    });
    if let Err(e) = result {
        log(format!("Failed to save Discord RPC setting: {e}"));
    }
}

/// Checks a token with the backend and makes it the session.
async fn establish_session(app: &AppHandle, token: String) -> Value {
    let state = app.state::<AppState>();
    match Backend::new(Some(token.clone())).profile().await {
        Ok(profile) => {
            let name = profile
                .get("username")
                .or(profile.get("email"))
                .and_then(Value::as_str)
                .unwrap_or("User")
                .to_string();
            *state.session.lock().unwrap_or_else(|e| e.into_inner()) = Some((token, name.clone()));
            *state.hypixel.lock().unwrap_or_else(|e| e.into_inner()) = None;
            log("Session established.");
            json!({ "ok": true, "displayName": name })
        }
        Err(e) => {
            log(format!("Login failed: {e}"));
            *state.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
            json!({ "ok": false, "error": "Login failed. Please try again." })
        }
    }
}

async fn restore_session(app: &AppHandle) {
    let reply = match account::load() {
        Some((token, _)) => {
            let reply = establish_session(app, token).await;
            if reply["ok"] == json!(false) {
                // An expired login: forget it quietly.
                account::clear();
                json!({ "ok": false })
            } else {
                reply
            }
        }
        None => json!({ "ok": false }),
    };
    emit(app, "session-state", reply);
}

fn start_login(app: &AppHandle) {
    let state = app.state::<AppState>();
    let app = app.clone();
    let task = tokio::spawn(async move {
        let result = jagprox_core::web_login::browser_login(|url| {
            let _ = open::that_detached(url);
        })
        .await;
        let reply = match result {
            Ok(token) => {
                let reply = establish_session(&app, token.clone()).await;
                if reply["ok"] == json!(true)
                    && let Err(e) = account::save(&token)
                {
                    log(format!("Could not save the login: {e}"));
                }
                reply
            }
            Err(e) => {
                log(format!("Login failed: {e}"));
                json!({ "ok": false, "error": "Login failed. Please try again." })
            }
        };
        emit(&app, "session-state", reply);
    });
    // A new click replaces a login that's still waiting.
    if let Some(old) = state
        .login
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(task.abort_handle())
    {
        old.abort();
    }
}

fn start_proxy(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut slot = state.proxy.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return;
    }
    let Some(jwt) = state.jwt() else {
        log_line(app, "[SYSTEM] You must be logged in to launch the proxy.");
        return;
    };
    let (events, mut rx) = tokio::sync::broadcast::channel(256);
    let proxy = match Proxy::new(ProxyOptions {
        data_dir: state.data_dir.clone(),
        config: state.config.clone(),
        jwt: Some(jwt),
        events,
    }) {
        Ok(p) => p,
        Err(e) => {
            log_line(app, &format!("§c[ERROR] Could not start the proxy: {e}"));
            return;
        }
    };
    let handle = app.clone();
    let runner = proxy.clone();
    let task = tokio::spawn(async move {
        if let Err(e) = runner.run().await {
            log_line(&handle, &format!("§c❌ FATAL ERROR: {e}"));
        }
        let state = handle.state::<AppState>();
        state.proxy.lock().unwrap_or_else(|e| e.into_inner()).take();
        emit(&handle, "proxy-status", "stopped");
        state.discord.set_playing(false);
    });
    *slot = Some((proxy, task.abort_handle()));
    drop(slot);
    emit(app, "proxy-status", "running");

    let app = app.clone();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let state = app.state::<AppState>();
            match event {
                ProxyEvent::Chat(text) => emit(&app, "proxy-chat", text),
                ProxyEvent::Playing(playing) => state.discord.set_playing(playing),
                ProxyEvent::DiscordRpc(enabled) => state.discord.set_enabled(enabled),
                ProxyEvent::SignIn { name, code, link } => {
                    log_line(
                        &app,
                        &format!(
                            "§d§lSign in to Microsoft as §f§l{name}§d§l: open §b{link}§d§l and enter §f§l{code}"
                        ),
                    );
                    let _ = open::that_detached(&link);
                }
                ProxyEvent::SignedIn(name) => log_line(
                    &app,
                    &format!("§aSigned in to Microsoft as {name}. Reconnect in Minecraft."),
                ),
                ProxyEvent::Started { host, port } => log_line(
                    &app,
                    &format!(
                        "§dJagProx is running. In Minecraft 1.8.9, connect to §f{host}:{port}"
                    ),
                ),
            }
        }
    });
}

/// Stat Search page.
async fn player_stats(state: &AppState, name: &str, mode: &str) -> Value {
    let Some(h) = state.hypixel() else {
        return json!({ "error": "Hypixel handler not initialized. Is user logged in?" });
    };
    let Some(game) = constants::game_mode(&mode.to_lowercase()) else {
        return json!({ "error": format!("Unknown game mode: {mode}") });
    };
    let profile = match h.mojang_uuid(name).await {
        Ok(Some(p)) => p,
        _ => return json!({ "error": format!("Player '{name}' not found.") }),
    };
    let (data, guild) = tokio::join!(h.stats(&profile.uuid), h.guild(&profile.uuid));
    let Ok(Some(data)) = data else {
        return json!({ "error": format!("Could not retrieve player data for '{}'.", profile.username) });
    };
    let player = Some(data.player.as_ref());
    let guild = guild
        .ok()
        .filter(|g| !g.is_empty())
        .map(|g| format!(" §7{g}"))
        .unwrap_or_default();
    let s = stat_format::get_mode_stats(player, game);
    let mut lines = vec![
        format!("§d§lPlayer Stats for {}", game.display_name),
        format!(
            "{} {}{}{guild}",
            format_rank(player),
            player_name_color(player),
            profile.username
        ),
    ];
    if s.has_stats {
        lines.extend(stat_format::format_stat_lines(&s, game.api_name));
        if stat_format::has_winstreaks(game.api_name) {
            lines.push(stat_format::format_winstreak_line(&s));
        }
    } else {
        lines.push("§7No stats found for this game.".into());
    }
    json!({ "lines": lines })
}

/// Status Check page.
async fn player_status(state: &AppState, name: &str) -> Value {
    let Some(h) = state.hypixel() else {
        return json!({ "error": "Hypixel handler not initialized. Is user logged in?" });
    };
    let profile = match h.mojang_uuid(name).await {
        Ok(Some(p)) => p,
        _ => return json!({ "error": format!("Player '{name}' not found.") }),
    };
    let Ok(Some(status)) = h.player_status(&profile.uuid).await else {
        return json!({ "error": format!("Could not retrieve status for '{}'.", profile.username) });
    };
    let player = Some(status.data.player.as_ref());
    let mut lines = vec![
        format!("§d§lPlayer Status for {}", profile.username),
        format!(
            "{} {}{}",
            format_rank(player),
            player_name_color(player),
            profile.username
        ),
    ];
    match status.status {
        OnlineStatus::Offline => lines.push("§cOffline.".into()),
        OnlineStatus::Hidden => {
            lines.push("§aOnline.".into());
            lines.push("§7(Status is hidden, game info unavailable)".into());
        }
        OnlineStatus::Online {
            game_type,
            mode,
            map,
        } => {
            lines.push("§aOnline.".into());
            lines.push(format!("§fGame: §b{}", game_type.unwrap_or_default()));
            if let Some(mode) = mode.filter(|m| !m.is_empty()) {
                lines.push(format!("§fMode: §e{mode}"));
            }
            if let Some(map) = map.filter(|m| !m.is_empty()) {
                lines.push(format!("§fMap: §e{map}"));
            }
        }
    }
    json!({ "lines": lines })
}

/// The Settings "Install Command" button: links `jagprox` onto PATH and, on Linux,
/// adds ~/.local/bin to the shell's PATH if it isn't there.
#[cfg(unix)]
fn install_cli() -> Result<String, String> {
    let target = install::cli_target(&install::detect())?;
    let link = install::link_path().ok_or("could not find your home folder")?;
    match install::link_cli(&target, &link) {
        Ok(_) => {}
        #[cfg(target_os = "macos")]
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            install::link_cli_as_admin(&target, &link, true)
                .map_err(|e| format!("Not installed: {e}"))?;
        }
        Err(e) => return Err(format!("Could not create {}: {e}", link.display())),
    }
    log(format!(
        "Installed the jagprox command: {} -> {}",
        link.display(),
        target.display()
    ));
    if !cfg!(target_os = "macos")
        && let Some(dir) = link.parent()
        && !install::on_path(dir)
        && let Some(fix) = install::current_path_fix()
    {
        install::apply_path_fix(&fix)?;
    }
    Ok("Installed. Open a new terminal and run: jagprox --help".into())
}

#[cfg(not(unix))]
fn install_cli() -> Result<String, String> {
    Err("The installer already put jagprox on PATH.".into())
}
