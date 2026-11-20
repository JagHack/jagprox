//! The JagProx launcher window, native (egui/eframe). Replaces the Tauri/WebView build:
//! same features, driven straight from `jagprox-core` instead of through an IPC bridge,
//! and no bundled browser engine.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod discord;
mod theme;
mod updater;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use jagprox_core::account;
use jagprox_core::api::{Backend, Hypixel, OnlineStatus};
use jagprox_core::config::Config;
use jagprox_core::constants::{self, GAME_MODES};
use jagprox_core::formatter::{format_rank, player_name_color};
use jagprox_core::log::{self, log};
use jagprox_core::proxy::{Proxy, ProxyEvent, ProxyOptions};
use jagprox_core::{install, stat_format};
use serde_json::json;
use tokio::task::AbortHandle;

use crate::discord::Discord;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Page {
    Home,
    Logs,
    Chat,
    Aliases,
    Stats,
    Status,
    Settings,
}

/// The result of a stat/status lookup for a results pane.
enum Lookup {
    Idle,
    Loading,
    Lines(Vec<String>),
    Error(String),
}

/// Everything the UI thread and the background tasks share.
struct Shared {
    ctx: egui::Context,
    rt: tokio::runtime::Handle,
    data_dir: PathBuf,
    config: Config,
    discord: Discord,
    session: Mutex<Option<(String, String)>>,
    hypixel: Mutex<Option<Hypixel>>,
    proxy: Mutex<Option<(Arc<Proxy>, AbortHandle)>>,
    proxy_running: AtomicBool,
    login_task: Mutex<Option<AbortHandle>>,
    logging_in: AtomicBool,
    logs: Mutex<Vec<String>>,
    chat: Mutex<Vec<String>>,
    stats: Mutex<Lookup>,
    status: Mutex<Lookup>,
}

impl Shared {
    fn push_log(&self, line: impl Into<String>) {
        let mut logs = self.logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.push(line.into());
        if logs.len() > 2000 {
            let drop = logs.len() - 2000;
            logs.drain(0..drop);
        }
        self.ctx.request_repaint();
    }

    fn jwt(&self) -> Option<String> {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(jwt, _)| jwt.clone())
    }

    fn display_name(&self) -> Option<String> {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(_, name)| name.clone())
    }

    /// A Hypixel client keyed to the logged-in account, built on demand.
    fn hypixel(&self) -> Option<Hypixel> {
        let jwt = self.jwt()?;
        let mut h = self.hypixel.lock().unwrap_or_else(|e| e.into_inner());
        Some(
            h.get_or_insert_with(|| Hypixel::new(Backend::new(Some(jwt)), None, None))
                .clone(),
        )
    }

    fn stop_proxy(&self) {
        if let Some((proxy, task)) = self.proxy.lock().unwrap_or_else(|e| e.into_inner()).take() {
            task.abort();
            proxy.shutdown();
        }
        self.proxy_running.store(false, Ordering::SeqCst);
        self.discord.set_playing(false);
    }
}

/// Checks a token with the backend and makes it the session.
async fn establish_session(shared: &Arc<Shared>, token: String) -> bool {
    match Backend::new(Some(token.clone())).profile().await {
        Ok(profile) => {
            let name = profile
                .get("username")
                .or(profile.get("email"))
                .and_then(|v| v.as_str())
                .unwrap_or("User")
                .to_string();
            *shared.session.lock().unwrap_or_else(|e| e.into_inner()) = Some((token, name));
            *shared.hypixel.lock().unwrap_or_else(|e| e.into_inner()) = None;
            log("Session established.");
            shared.ctx.request_repaint();
            true
        }
        Err(e) => {
            log(format!("Login failed: {e}"));
            *shared.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
            shared.ctx.request_repaint();
            false
        }
    }
}

fn restore_session(shared: &Arc<Shared>) {
    if let Some((token, _)) = account::load() {
        let shared = shared.clone();
        let rt = shared.rt.clone();
        rt.spawn(async move {
            if !establish_session(&shared, token).await {
                account::clear();
            }
        });
    }
}

fn start_login(shared: &Arc<Shared>) {
    if shared.logging_in.swap(true, Ordering::SeqCst) {
        return;
    }
    let reg = shared.clone();
    let shared = shared.clone();
    let rt = shared.rt.clone();
    let task = rt.spawn(async move {
        let result = jagprox_core::web_login::browser_login(|url| {
            let _ = open::that_detached(url);
        })
        .await;
        match result {
            Ok(token) => {
                if establish_session(&shared, token.clone()).await
                    && let Err(e) = account::save(&token)
                {
                    log(format!("Could not save the login: {e}"));
                }
            }
            Err(e) => log(format!("Login failed: {e}")),
        }
        shared.logging_in.store(false, Ordering::SeqCst);
        shared.ctx.request_repaint();
    });
    if let Some(old) = reg
        .login_task
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(task.abort_handle())
    {
        old.abort();
    }
}

fn start_proxy(shared: &Arc<Shared>) {
    {
        let slot = shared.proxy.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return;
        }
    }
    let Some(jwt) = shared.jwt() else {
        shared.push_log("§c[SYSTEM] You must be logged in to launch the proxy.");
        return;
    };
    let (events, mut rx) = tokio::sync::broadcast::channel(256);
    let proxy = match Proxy::new(ProxyOptions {
        data_dir: shared.data_dir.clone(),
        config: shared.config.clone(),
        jwt: Some(jwt),
        events,
    }) {
        Ok(p) => p,
        Err(e) => {
            shared.push_log(format!("§c[ERROR] Could not start the proxy: {e}"));
            return;
        }
    };
    let runner = proxy.clone();
    let s = shared.clone();
    let task = shared.rt.spawn(async move {
        if let Err(e) = runner.run().await {
            s.push_log(format!("§c❌ FATAL ERROR: {e}"));
        }
        s.proxy.lock().unwrap_or_else(|e| e.into_inner()).take();
        s.proxy_running.store(false, Ordering::SeqCst);
        s.discord.set_playing(false);
        s.ctx.request_repaint();
    });
    *shared.proxy.lock().unwrap_or_else(|e| e.into_inner()) = Some((proxy, task.abort_handle()));
    shared.proxy_running.store(true, Ordering::SeqCst);

    let s = shared.clone();
    shared.rt.spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                ProxyEvent::Chat(text) => {
                    let mut chat = s.chat.lock().unwrap_or_else(|e| e.into_inner());
                    chat.push(text);
                    if chat.len() > 2000 {
                        let drop = chat.len() - 2000;
                        chat.drain(0..drop);
                    }
                    s.ctx.request_repaint();
                }
                ProxyEvent::Playing(p) => s.discord.set_playing(p),
                ProxyEvent::DiscordRpc(e) => s.discord.set_enabled(e),
                ProxyEvent::SignIn { name, code, link } => {
                    s.push_log(format!(
                        "§d§lSign in to Microsoft as §f§l{name}§d§l: open §b{link}§d§l and enter §f§l{code}"
                    ));
                    let _ = open::that_detached(&link);
                }
                ProxyEvent::SignedIn(name) => {
                    s.push_log(format!("§aSigned in to Microsoft as {name}. Reconnect in Minecraft."));
                }
                ProxyEvent::Started { host, port } => {
                    s.push_log(format!("§dJagProx is running. In Minecraft 1.8.9, connect to §f{host}:{port}"));
                }
            }
        }
    });
}

async fn lookup_stats(shared: &Arc<Shared>, name: String, mode: String) -> Lookup {
    let Some(h) = shared.hypixel() else {
        return Lookup::Error("Log in first.".into());
    };
    let Some(game) = constants::game_mode(&mode.to_lowercase()) else {
        return Lookup::Error(format!("Unknown game mode: {mode}"));
    };
    let profile = match h.mojang_uuid(&name).await {
        Ok(Some(p)) => p,
        _ => return Lookup::Error(format!("Player '{name}' not found.")),
    };
    let (data, guild) = tokio::join!(h.stats(&profile.uuid), h.guild(&profile.uuid));
    let Ok(Some(data)) = data else {
        return Lookup::Error(format!(
            "Could not retrieve data for '{}'.",
            profile.username
        ));
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
    Lookup::Lines(lines)
}

async fn lookup_status(shared: &Arc<Shared>, name: String) -> Lookup {
    let Some(h) = shared.hypixel() else {
        return Lookup::Error("Log in first.".into());
    };
    let profile = match h.mojang_uuid(&name).await {
        Ok(Some(p)) => p,
        _ => return Lookup::Error(format!("Player '{name}' not found.")),
    };
    let Ok(Some(status)) = h.player_status(&profile.uuid).await else {
        return Lookup::Error(format!(
            "Could not retrieve status for '{}'.",
            profile.username
        ));
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
    Lookup::Lines(lines)
}

struct App {
    #[allow(dead_code)]
    rt: tokio::runtime::Runtime,
    shared: Arc<Shared>,
    page: Page,
    // Settings form
    autogg_enabled: bool,
    autogg_msg: String,
    autogg_delay: String,
    api_key: String,
    api_key_set: bool,
    discord_rpc: bool,
    cli_status: String,
    // Lookups
    stat_name: String,
    stat_mode: String,
    status_name: String,
    gamemodes: Vec<(String, String)>,
    // Aliases editor
    aliases: Vec<(String, String)>,
    aliases_loaded: bool,
    aliases_status: String,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        rt: tokio::runtime::Runtime,
        shared: Arc<Shared>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let autogg = shared.config.get("auto_gg").unwrap_or_else(|| json!({}));
        let discord = shared
            .config
            .get("discord_rpc")
            .unwrap_or_else(|| json!({}));
        let mut gamemodes: Vec<(String, String)> = Vec::new();
        for (key, info) in GAME_MODES {
            if !gamemodes.iter().any(|(name, _)| name == info.display_name) {
                gamemodes.push((info.display_name.to_string(), key.to_string()));
            }
        }
        gamemodes.sort_by_key(|(name, _)| name.to_lowercase());

        restore_session(&shared);
        let app = App {
            rt,
            autogg_enabled: autogg
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            autogg_msg: autogg
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("gg")
                .to_string(),
            autogg_delay: autogg
                .get("delay")
                .and_then(|v| v.as_i64())
                .unwrap_or(1500)
                .to_string(),
            discord_rpc: discord
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            api_key: String::new(),
            api_key_set: false,
            cli_status: String::new(),
            stat_name: String::new(),
            stat_mode: gamemodes
                .iter()
                .find(|(n, _)| n == "Bed Wars")
                .or_else(|| gamemodes.first())
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
            status_name: String::new(),
            gamemodes,
            aliases: Vec::new(),
            aliases_loaded: false,
            aliases_status: String::new(),
            page: Page::Home,
            shared,
        };
        if app.discord_rpc {
            app.shared.discord.set_enabled(true);
        }
        app
    }

    fn save_settings(&self) {
        let enabled = self.autogg_enabled;
        let message = if self.autogg_msg.trim().is_empty() {
            "gg".to_string()
        } else {
            self.autogg_msg.trim().to_string()
        };
        let delay: i64 = self.autogg_delay.trim().parse().unwrap_or(1500);
        let _ = self.shared.config.update(&["auto_gg"], move |c| {
            let entry = c.entry("auto_gg").or_insert_with(|| json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            entry["enabled"] = json!(enabled);
            entry["message"] = json!(message);
            entry["delay"] = json!(delay);
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sidebar(ctx, self);
        egui::CentralPanel::default().show(ctx, |ui| match self.page {
            Page::Home => theme::home(ui, self),
            Page::Logs => theme::log_view(ui, &self.shared.logs, "Live Logs"),
            Page::Chat => theme::log_view(ui, &self.shared.chat, "In-Game Chat"),
            Page::Aliases => theme::aliases_page(ui, self),
            Page::Stats => theme::stats_page(ui, self),
            Page::Status => theme::status_page(ui, self),
            Page::Settings => theme::settings(ui, self),
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shared.stop_proxy();
        self.shared.discord.set_enabled(false);
    }
}

/// `jagprox install-cli` links `jagprox` to the AppImage itself, so the AppImage
/// starts this binary for CLI commands too; those runs go to the bundled CLI.
#[cfg(target_os = "linux")]
fn run_cli_if_called_as_jagprox() {
    use std::os::unix::process::CommandExt;
    if std::env::var_os("APPIMAGE").is_none() {
        return;
    }
    let Some(argv0) = std::env::var_os("ARGV0") else {
        return;
    };
    if std::path::Path::new(&argv0).file_name() != Some("jagprox".as_ref()) {
        return;
    }
    let Some(cli) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("jagprox")))
    else {
        return;
    };
    let error = std::process::Command::new(&cli)
        .args(std::env::args_os().skip(1))
        .exec();
    eprintln!("jagprox: could not start {}: {error}", cli.display());
    std::process::exit(1);
}

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "linux")]
    run_cli_if_called_as_jagprox();

    let data_dir = match account::data_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("JagProx: {e}");
            std::process::exit(1);
        }
    };
    let _ = std::fs::create_dir_all(data_dir.join("logs"));
    let config = match Config::load(&data_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("JagProx: could not load config.yml: {e}");
            std::process::exit(1);
        }
    };
    let _ = jagprox_core::aliases::Aliases::load(&data_dir);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let discord = Discord::start();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 650.0])
            .with_min_inner_size([760.0, 480.0])
            .with_title("JagProx"),
        ..Default::default()
    };

    let rt_handle = rt.handle().clone();
    let config_watch = config.clone();
    {
        // watch() spawns its own task and returns a handle; run it in the runtime.
        let _guard = rt.enter();
        config_watch.watch();
    }

    eframe::run_native(
        "JagProx",
        options,
        Box::new(move |cc| {
            let shared = Arc::new(Shared {
                ctx: cc.egui_ctx.clone(),
                rt: rt_handle,
                data_dir: data_dir.clone(),
                config: config.clone(),
                discord,
                session: Mutex::new(None),
                hypixel: Mutex::new(None),
                proxy: Mutex::new(None),
                proxy_running: AtomicBool::new(false),
                login_task: Mutex::new(None),
                logging_in: AtomicBool::new(false),
                logs: Mutex::new(Vec::new()),
                chat: Mutex::new(Vec::new()),
                stats: Mutex::new(Lookup::Idle),
                status: Mutex::new(Lookup::Idle),
            });
            // Log lines flow to the Log Monitor and logs/main.log.
            let sink = shared.clone();
            let log_file = std::sync::Mutex::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(data_dir.join("logs/main.log"))
                    .ok(),
            );
            log::set_sink(move |line| {
                use std::io::Write;
                if let Some(f) = log_file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                sink.push_log(line.to_string());
            });
            // First start after JagProx 2: keep the login, drop the Chromium leftovers.
            let moved = jagprox_core::electron::migrate(&data_dir);
            if moved.login_imported {
                log("Kept your JagProx 2 login.");
            }
            // Quietly check for a newer release on startup.
            updater::check(&shared, true);
            Ok(Box::new(App::new(cc, rt, shared)))
        }),
    )
}

/// Actions the panes call back into.
impl App {
    fn toggle_proxy(&self) {
        if self.shared.proxy_running.load(Ordering::SeqCst) {
            self.shared.stop_proxy();
            self.shared.push_log("§7[SYSTEM] Proxy stopped.");
        } else {
            start_proxy(&self.shared);
        }
    }

    fn login(&self) {
        start_login(&self.shared);
    }

    fn logout(&self) {
        account::clear();
        *self
            .shared
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self
            .shared
            .hypixel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        self.shared.stop_proxy();
        log("Logged out.");
    }

    fn toggle_discord(&self, enabled: bool) {
        self.shared.discord.set_enabled(enabled);
        let _ = self.shared.config.update(&["discord_rpc"], move |c| {
            let entry = c.entry("discord_rpc").or_insert_with(|| json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            entry["enabled"] = json!(enabled);
        });
    }

    fn save_api_key(&mut self) {
        let key = self.api_key.trim().to_string();
        if key.is_empty() {
            return;
        }
        let Some(jwt) = self.shared.jwt() else {
            return;
        };
        let shared = self.shared.clone();
        self.api_key.clear();
        self.shared.rt.spawn(async move {
            match Backend::new(Some(jwt)).save_api_key(&key).await {
                Ok(_) => log("API Key saved."),
                Err(e) => log(format!("Could not save API key: {e}")),
            }
            *shared.hypixel.lock().unwrap_or_else(|e| e.into_inner()) = None;
        });
    }

    fn do_stats(&self) {
        *self.shared.stats.lock().unwrap_or_else(|e| e.into_inner()) = Lookup::Loading;
        let (shared, name, mode) = (
            self.shared.clone(),
            self.stat_name.trim().to_string(),
            self.stat_mode.clone(),
        );
        self.shared.rt.spawn(async move {
            let result = lookup_stats(&shared, name, mode).await;
            *shared.stats.lock().unwrap_or_else(|e| e.into_inner()) = result;
            shared.ctx.request_repaint();
        });
    }

    fn do_status(&self) {
        *self.shared.status.lock().unwrap_or_else(|e| e.into_inner()) = Lookup::Loading;
        let (shared, name) = (self.shared.clone(), self.status_name.trim().to_string());
        self.shared.rt.spawn(async move {
            let result = lookup_status(&shared, name).await;
            *shared.status.lock().unwrap_or_else(|e| e.into_inner()) = result;
            shared.ctx.request_repaint();
        });
    }

    fn check_updates(&self) {
        updater::check(&self.shared, false);
    }

    /// Reads aliases.json into the editor the first time the page is shown.
    fn ensure_aliases_loaded(&mut self) {
        if self.aliases_loaded {
            return;
        }
        self.aliases_loaded = true;
        let text =
            std::fs::read_to_string(self.shared.data_dir.join("aliases.json")).unwrap_or_default();
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}'))
        {
            let mut pairs: Vec<(String, String)> = map
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
            pairs.sort();
            self.aliases = pairs;
        }
    }

    fn save_aliases(&mut self) {
        let mut clean = serde_json::Map::new();
        for (from, to) in &self.aliases {
            let from = from.trim().to_lowercase();
            let to = to.trim().to_string();
            if from.starts_with('/') && to.starts_with('/') {
                clean.insert(from, json!(to));
            }
        }
        let count = clean.len();
        let text =
            serde_json::to_string_pretty(&serde_json::Value::Object(clean)).unwrap_or_default();
        match jagprox_core::file_util::write_file_atomic(
            &self.shared.data_dir.join("aliases.json"),
            &text,
        ) {
            Ok(()) => self.aliases_status = format!("Saved {count} aliases."),
            Err(e) => self.aliases_status = format!("Could not save: {e}"),
        }
    }

    fn install_cli_clicked(&mut self) {
        match install_cli() {
            Ok(msg) => self.cli_status = msg,
            Err(e) => self.cli_status = e,
        }
    }
}

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
