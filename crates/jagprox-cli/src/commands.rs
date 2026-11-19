//! The Hypixel and account commands (`jagprox stats`, `status`, `login`, ...).
//! Lookups produce the same `§` lines as the in-game commands and are rendered here.

use std::path::Path;

use jagprox_core::api::{ApiError, Backend, Hypixel};
use jagprox_core::constants::{self, GameInfo};
use jagprox_core::nicknames::Nicknames;
use jagprox_core::reports::{self, ReportError};
use serde_json::{Value, json};

use crate::session;
use crate::term::{self, ColorMode, Spinner};

pub type CmdResult = Result<(), String>;

pub struct Ctx {
    pub color: ColorMode,
    pub json: bool,
}

fn backend() -> Backend {
    Backend::new(session::load().map(|(token, _)| token))
}

/// Hypixel access via the JagProx account's stored key, or `HYPIXEL_API_KEY`.
fn hypixel() -> Result<Hypixel, String> {
    let fixed_key = std::env::var("HYPIXEL_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());
    let backend = backend();
    if backend.jwt().is_none() && fixed_key.is_none() {
        return Err(not_logged_in());
    }
    Ok(Hypixel::new(backend, fixed_key, None))
}

fn not_logged_in() -> String {
    "you are not logged in. Run `jagprox login` (or set HYPIXEL_API_KEY).".into()
}

fn api_error(e: &ApiError) -> String {
    match e {
        ApiError::NotLoggedIn => not_logged_in(),
        ApiError::NoApiKey => {
            "no Hypixel API key is set for your account. Add one with `jagprox key <your-key>`."
                .into()
        }
        ApiError::InvalidApiKey => "Hypixel rejected your API key (invalid or expired). \
                                    Set a new one with `jagprox key <your-key>`."
            .into(),
        ApiError::Status(401 | 403, _) => {
            "your login has expired. Run `jagprox login` again.".into()
        }
        other => other.to_string(),
    }
}

fn report_error(e: ReportError) -> String {
    match &e.cause {
        Some(cause) => api_error(cause),
        None => jagprox_core::formatter::strip_colors(e.line.trim_start_matches("§c")),
    }
}

/// Nicknames from config.yml, so `jagprox stats <nickname>` works like in game.
fn nicknames() -> Nicknames {
    let Ok(dir) = session::data_dir() else {
        return Nicknames::new();
    };
    let Ok(text) = std::fs::read_to_string(dir.join("config.yml")) else {
        return Nicknames::new();
    };
    let config = jagprox_core::file_util::parse_yaml(&text).unwrap_or(Value::Null);
    config
        .get("nicknames")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// `bw`, `bedwars`, `"Bed Wars"`, `bed wars` all find Bed Wars.
fn resolve_mode(words: &[String]) -> Option<(&'static str, &'static GameInfo)> {
    let joined = words.join(" ").to_lowercase();
    let compact: String = joined.split_whitespace().collect();
    constants::GAME_MODES
        .iter()
        .find(|(alias, _)| *alias == joined || *alias == compact)
        .or_else(|| {
            constants::GAME_MODES
                .iter()
                .find(|(_, g)| g.display_name.to_lowercase() == joined)
        })
        .map(|(alias, game)| (*alias, game))
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

// --- account ---------------------------------------------------------------

pub async fn login(email: Option<Option<String>>) -> CmdResult {
    let token = match email {
        Some(email) => crate::login::email_login(email).await?,
        None => crate::login::browser_login().await?,
    };
    let profile = Backend::new(Some(token.clone()))
        .profile()
        .await
        .map_err(|e| format!("login failed: {e}"))?;
    let store = session::save(&token)?;
    let name = profile
        .get("username")
        .or(profile.get("email"))
        .and_then(Value::as_str)
        .unwrap_or("User");
    println!(
        "Logged in as {name}. Your login is saved in {}.",
        store.describe()
    );
    Ok(())
}

pub fn logout() -> CmdResult {
    if session::clear() {
        println!("Logged out.");
    } else {
        println!("You were not logged in.");
    }
    if std::env::var_os("JAGPROX_TOKEN").is_some() {
        println!("Note: JAGPROX_TOKEN is still set in your environment.");
    }
    Ok(())
}

pub async fn profile(ctx: &Ctx) -> CmdResult {
    let backend = backend();
    if backend.jwt().is_none() {
        return Err(not_logged_in());
    }
    let spinner = Spinner::start("Fetching profile...");
    let profile = backend.profile().await.map_err(|e| api_error(&e));
    spinner.stop();
    let profile = profile?;
    if ctx.json {
        print_json(&profile);
        return Ok(());
    }
    let text = |k: &str| {
        profile
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string()
    };
    let rank = profile
        .get("globalRank")
        .and_then(Value::as_str)
        .unwrap_or("User");
    term::print_lines(
        &[
            "§5§lJagProx Profile".into(),
            format!("§dUsername §8» §f{}", text("username")),
            format!("§dEmail §8» §f{}", text("email")),
            format!("§dRank §8» §e{rank}"),
        ],
        ctx.color,
    );
    Ok(())
}

fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len());
    }
    let start: String = chars[..4].iter().collect();
    let end: String = chars[chars.len() - 4..].iter().collect();
    format!("{start}…{end}")
}

pub async fn key(new_key: Option<String>, show: bool) -> CmdResult {
    let backend = backend();
    if backend.jwt().is_none() {
        return Err(not_logged_in());
    }
    match new_key {
        Some(key) => {
            let key = key.trim();
            let looks_like_uuid = key.len() == 36 && key.chars().filter(|c| *c == '-').count() == 4;
            if !looks_like_uuid {
                eprintln!(
                    "Warning: that doesn't look like a Hypixel API key (they look like a UUID). Saving anyway."
                );
            }
            backend.save_api_key(key).await.map_err(|e| api_error(&e))?;
            println!("Hypixel API key saved to your JagProx account.");
        }
        None => match backend.get_api_key().await.map_err(|e| api_error(&e))? {
            Some(key) if show => println!("{key}"),
            Some(key) => println!(
                "Hypixel API key: {} (set). Use --show to reveal it.",
                mask(&key)
            ),
            None => println!("No Hypixel API key is set. Add one with `jagprox key <your-key>`."),
        },
    }
    Ok(())
}

// --- lookups ---------------------------------------------------------------

pub async fn stats(ctx: &Ctx, player: &str, mode_words: &[String]) -> CmdResult {
    let (alias, game) = if mode_words.is_empty() {
        resolve_mode(&["bedwars".into()]).expect("bedwars exists")
    } else {
        resolve_mode(mode_words).ok_or_else(|| {
            format!(
                "unknown game mode '{}'. Run `jagprox modes` for the list.",
                mode_words.join(" ")
            )
        })?
    };
    let h = hypixel()?;
    let spinner = Spinner::start(format!(
        "Fetching {} stats for {player}...",
        game.display_name
    ));
    let result = reports::statcheck(&h, alias, player, &nicknames()).await;
    spinner.stop();
    let sc = result.map_err(report_error)?;

    if ctx.json {
        let s = jagprox_core::stat_format::get_mode_stats(Some(&sc.data.player), sc.game);
        print_json(&json!({
            "username": sc.profile.username,
            "uuid": sc.profile.uuid,
            "rank": sc.data.rank,
            "game": sc.game.display_name,
            "stats": {
                "wins": s.wins, "losses": s.losses, "kills": s.kills, "deaths": s.deaths,
                "final_kills": s.final_kills, "final_deaths": s.final_deaths,
                "beds_broken": s.beds_broken, "beds_lost": s.beds_lost,
                "winstreak": s.winstreak, "best_winstreak": s.best_winstreak, "level": s.level,
            },
            "player": sc.data.player.as_ref(),
        }));
        return Ok(());
    }

    let head = sc.head.as_ref().map(|h| h.composite(10));
    for row in stats_layout(&sc.lines, head.as_ref(), ctx.color) {
        println!("{row}");
    }
    Ok(())
}

/// The old `hypixel stats` layout: the head on the left, stat lines on the right,
/// between two separators. Without colors the head is left out.
fn stats_layout(
    stat_lines: &[String],
    head: Option<&[[[u8; 4]; 8]; 8]>,
    color: ColorMode,
) -> Vec<String> {
    let mut lines = stat_lines.to_vec();
    lines.insert(2.min(lines.len()), "§8─────────────────────────".into());
    let head = head.filter(|_| color != ColorMode::None);
    let separator = term::render(&format!("  §8{}", "━".repeat(60)), color);

    let mut out = vec![String::new(), separator.clone()];
    for i in 0..lines.len().max(if head.is_some() { 8 } else { 0 }) {
        let mut row = String::from("  ");
        match head {
            Some(pixels) if i < 8 => {
                for [r, g, b, a] in pixels[i] {
                    if a > 128 {
                        row.push_str(&term::fg(color, [r, g, b]));
                        row.push_str("██");
                        row.push_str(term::reset(color));
                    } else {
                        row.push_str("  ");
                    }
                }
                row.push_str("   ");
            }
            Some(_) => row.push_str(&" ".repeat(19)),
            None => {}
        }
        if let Some(line) = lines.get(i) {
            row.push_str(&term::render(line, color));
        }
        out.push(row.trim_end().to_string());
    }
    out.push(separator);
    out.push(String::new());
    out
}

pub fn modes(ctx: &Ctx) -> CmdResult {
    term::print_lines(&reports::statcheck_modes(), ctx.color);
    Ok(())
}

pub async fn status(ctx: &Ctx, player: &str) -> CmdResult {
    let h = hypixel()?;
    let name = reports::resolve_nickname(player, &nicknames());
    let spinner = Spinner::start(format!("Checking status for {name}..."));
    if ctx.json {
        let profile = h.mojang_uuid(&name).await.map_err(|e| api_error(&e))?;
        let profile = profile.ok_or_else(|| format!("player '{name}' not found."))?;
        let status = h
            .player_status(&profile.uuid)
            .await
            .map_err(|e| api_error(&e))?;
        spinner.stop();
        let status = status
            .ok_or_else(|| format!("could not retrieve status for '{}'.", profile.username))?;
        let (online, hidden, game, mode, map) = match status.status {
            jagprox_core::api::OnlineStatus::Online {
                game_type,
                mode,
                map,
            } => (true, false, game_type, mode, map),
            jagprox_core::api::OnlineStatus::Hidden => (true, true, None, None, None),
            jagprox_core::api::OnlineStatus::Offline => (false, false, None, None, None),
        };
        print_json(&json!({
            "username": profile.username, "uuid": profile.uuid, "rank": status.data.rank,
            "online": online, "hidden": hidden, "game": game, "mode": mode, "map": map,
        }));
        return Ok(());
    }
    let result = reports::status(&h, &name).await;
    spinner.stop();
    term::print_lines(&result.map_err(report_error)?, ctx.color);
    Ok(())
}

async fn lookup_uuid(h: &Hypixel, player: &str) -> Result<(String, String), String> {
    let name = reports::resolve_nickname(player, &nicknames());
    match h.mojang_uuid(&name).await {
        Ok(Some(p)) => Ok((p.uuid, p.username)),
        Ok(None) => Err(format!("player '{name}' not found.")),
        Err(e) => Err(api_error(&e)),
    }
}

pub async fn spread(ctx: &Ctx, player: &str) -> CmdResult {
    let h = hypixel()?;
    let spinner = Spinner::start(format!("Fetching Duels stats for {player}..."));
    let result = async {
        let (uuid, name) = lookup_uuid(&h, player).await?;
        reports::spread(&h, &uuid, Some(&name))
            .await
            .map_err(report_error)
    }
    .await;
    spinner.stop();
    term::print_lines(&result?, ctx.color);
    Ok(())
}

pub async fn jtitles(ctx: &Ctx, player: &str) -> CmdResult {
    let h = hypixel()?;
    let spinner = Spinner::start(format!("Fetching Duels titles for {player}..."));
    let result = async {
        let (uuid, name) = lookup_uuid(&h, player).await?;
        reports::jtitles(&h, &uuid, &name)
            .await
            .map_err(report_error)
    }
    .await;
    spinner.stop();
    term::print_lines(&result?, ctx.color);
    Ok(())
}

pub async fn playercount(ctx: &Ctx, mode_words: &[String]) -> CmdResult {
    let mode = mode_words.join(" ");
    if mode.is_empty() || mode == "?" {
        term::print_lines(&reports::playercount_modes(), ctx.color);
        return Ok(());
    }
    let h = hypixel()?;
    let spinner = Spinner::start("Fetching player counts...");
    let result = reports::playercount(&h, &mode).await;
    spinner.stop();
    match result {
        Ok(lines) => term::print_lines(&lines, ctx.color),
        Err(e) if e.cause.is_none() => {
            return Err(format!(
                "unknown Duels mode '{mode}'. Run `jagprox playercount` for the list."
            ));
        }
        Err(e) => return Err(report_error(e)),
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub enum GametrackPeriod {
    Hours(u32),
    Day,
    Log,
}

pub async fn gametrack(ctx: &Ctx, period: GametrackPeriod) -> CmdResult {
    let backend = backend();
    if backend.jwt().is_none() {
        return Err(not_logged_in());
    }
    let spinner = Spinner::start("Fetching tracked games...");
    let data = match &period {
        GametrackPeriod::Hours(n) => backend.gametrack_hours(*n).await,
        GametrackPeriod::Day => backend.gametrack_day().await,
        GametrackPeriod::Log => backend.gametrack_log().await,
    };
    let data = data.map_err(|e| api_error(&e));
    if ctx.json || data.is_err() {
        spinner.stop();
        print_json(&data?);
        return Ok(());
    }
    let data = data?;

    let (title, empty) = match &period {
        GametrackPeriod::Hours(n) => (
            format!("Game Stats for the Last {n} Hour(s)"),
            "No game data found for your account in this period.",
        ),
        GametrackPeriod::Day => (
            "Game Stats For Today".to_string(),
            "No game data found for your account today.",
        ),
        GametrackPeriod::Log => {
            spinner.stop();
            term::print_lines(&reports::gametrack_log(&data), ctx.color);
            return Ok(());
        }
    };

    // The backend groups results per linked Minecraft account.
    let accounts: Vec<(&String, &Value)> = data
        .as_object()
        .map(|m| m.iter().collect())
        .unwrap_or_default();
    let h = Hypixel::new(Backend::new(None), None, None);
    let mut blocks = Vec::new();
    for (uuid, modes) in &accounts {
        let name = h
            .username_from_uuid(uuid)
            .await
            .unwrap_or_else(|| uuid.to_string());
        let title = if accounts.len() > 1 {
            format!("{title} §8({name})")
        } else {
            format!("{title} §8- §d{name}")
        };
        blocks.push(reports::gametrack_modes(&title, empty, Some(modes)));
    }
    spinner.stop();
    if blocks.is_empty() {
        blocks.push(reports::gametrack_modes(&title, empty, None));
    }
    for block in blocks {
        term::print_lines(&block, ctx.color);
    }
    Ok(())
}

pub fn link() -> CmdResult {
    println!("Linking proves you own the Minecraft account, which needs your Minecraft login.");
    println!("Join Hypixel through JagProx and run /link in chat.");
    Ok(())
}

// --- config ----------------------------------------------------------------

pub fn open_config(default_config: &str) -> CmdResult {
    let dir = session::data_dir()?;
    let path = dir.join("config.yml");
    if !path.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, default_config).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("Created default config at {}", path.display());
    }
    open_in_editor(&path)
}

fn open_in_editor(path: &Path) -> CmdResult {
    // Terminal users with $EDITOR set get it in place; everyone else gets the OS default app.
    if let Some(editor) = std::env::var_os("EDITOR").filter(|e| !e.is_empty()) {
        let status = std::process::Command::new(&editor)
            .arg(path)
            .status()
            .map_err(|e| format!("could not start {}: {e}", editor.to_string_lossy()))?;
        if !status.success() {
            return Err(format!("{} exited with {status}", editor.to_string_lossy()));
        }
    } else {
        open::that_detached(path).map_err(|e| format!("could not open {}: {e}", path.display()))?;
        println!("Opened {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn resolve_mode_accepts_aliases_and_display_names() {
        assert_eq!(
            resolve_mode(&words("bw")).unwrap().1.display_name,
            "Bed Wars"
        );
        assert_eq!(
            resolve_mode(&words("Bed Wars")).unwrap().1.display_name,
            "Bed Wars"
        );
        assert_eq!(
            resolve_mode(&words("mega walls")).unwrap().1.display_name,
            "Mega Walls"
        );
        assert_eq!(
            resolve_mode(&words("Classic Duels")).unwrap().1.prefix,
            "classic_duel"
        );
        assert!(resolve_mode(&words("nope")).is_none());
    }

    fn sample_lines() -> Vec<String> {
        vec![
            "§8[§5Bed Wars§8]".into(),
            "§b[MVP§c+§b] §bNotch §5[TAG]".into(),
            "§dWins §5» §f1,234 §8| §dLosses §5» §f56".into(),
            "§dWinstreak §5» §f3 §8| §dBest §5» §f12".into(),
        ]
    }

    #[test]
    fn stats_layout_puts_the_head_next_to_the_lines() {
        let mut head = [[[0u8; 4]; 8]; 8];
        head[0][0] = [255, 0, 0, 255];
        let rows = stats_layout(&sample_lines(), Some(&head), ColorMode::TrueColor);
        // blank, separator, 8 head rows (5 stat lines incl. divider), separator, blank
        assert_eq!(rows.len(), 12);
        assert!(
            rows[2].starts_with("  \x1b[38;2;255;0;0m██\x1b[0m"),
            "{:?}",
            rows[2]
        );
        assert!(rows[2].contains("Bed Wars"));
        for row in &rows {
            println!("{row}");
        }
    }

    #[test]
    fn stats_layout_without_colors_is_plain_text() {
        let rows = stats_layout(&sample_lines(), None, ColorMode::None);
        assert_eq!(
            rows,
            vec![
                "".to_string(),
                format!("  {}", "━".repeat(60)),
                "  [Bed Wars]".into(),
                "  [MVP+] Notch [TAG]".into(),
                "  ─────────────────────────".into(),
                "  Wins » 1,234 | Losses » 56".into(),
                "  Winstreak » 3 | Best » 12".into(),
                format!("  {}", "━".repeat(60)),
                "".into(),
            ]
        );
    }

    #[test]
    fn mask_hides_the_middle() {
        assert_eq!(mask("12345678-aaaa-bbbb-cccc-1234567890ab"), "1234…90ab");
        assert_eq!(mask("short"), "*****");
    }
}
