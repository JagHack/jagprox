//! The text of JagProx's lookup commands, as `§`-colored lines. In game they are sent
//! as chat (exactly as the JS commands did); the `jagprox` CLI renders the same lines
//! with terminal colors, so both always match.

use std::sync::Arc;

use serde_json::Value;

use crate::api::{ApiError, Head, Hypixel, MojangProfile, OnlineStatus, PlayerData};
use crate::constants::{self, GameInfo};
use crate::formatter::{format_rank, player_name_color};
use crate::js::{field, pad_end, pad_start, to_fixed, to_locale};
use crate::nicknames::Nicknames;
use crate::stat_format;

pub const DIVIDER: &str = "§5§m----------------------------------------------------";
pub const SHORT_DIVIDER: &str = "§5§m----------------------------------------";

/// A failed lookup: the chat line the JS showed, plus the underlying API error when
/// there was one (the CLI shows that instead, it says more).
#[derive(Debug, Clone)]
pub struct ReportError {
    pub line: String,
    pub cause: Option<ApiError>,
}

impl ReportError {
    fn line(line: impl Into<String>) -> Self {
        Self {
            line: line.into(),
            cause: None,
        }
    }

    fn api(line: impl Into<String>, cause: ApiError) -> Self {
        Self {
            line: line.into(),
            cause: Some(cause),
        }
    }
}

pub type ReportResult<T> = Result<T, ReportError>;

/// `"[MVP+] Name"` / `"§aName"` → `"Name"` (JS `cleanRankPrefix`).
pub fn clean_rank_prefix(username: &str) -> String {
    let mut out = String::with_capacity(username.len());
    let mut rest = username;
    loop {
        if let Some(stripped) = rest.strip_prefix('§') {
            let mut chars = stripped.chars();
            if chars
                .next()
                .is_some_and(|c| !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
            {
                rest = chars.as_str();
                continue;
            }
        }
        if let Some(stripped) = rest.strip_prefix('[') {
            let len = stripped
                .find(|c: char| !(c.is_ascii_uppercase() || c == '+'))
                .unwrap_or(stripped.len());
            if len > 0 && stripped[len..].starts_with(']') {
                rest = &stripped[len + 1..];
                if let Some(c) = rest.chars().next().filter(|c| c.is_whitespace()) {
                    rest = &rest[c.len_utf8()..];
                }
                continue;
            }
        }
        let Some(c) = rest.chars().next() else { break };
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out.trim().to_string()
}

/// A nickname (or a differently-cased real name) → the real name; otherwise unchanged.
pub fn resolve_nickname(name: &str, nicknames: &Nicknames) -> String {
    let lower = name.to_lowercase();
    if let Some((real, _)) = nicknames
        .iter()
        .find(|(_, nick)| nick.to_lowercase() == lower)
    {
        return real.clone();
    }
    nicknames
        .keys()
        .find(|real| real.to_lowercase() == lower)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

/// Everything `/sc` fetched, for display and reuse (telemetry).
pub struct Statcheck {
    pub profile: MojangProfile,
    pub data: PlayerData,
    pub game: &'static GameInfo,
    pub head: Option<Arc<Head>>,
    /// Mode header, name line, stat lines and (where the game has them) the winstreak.
    pub lines: Vec<String>,
}

/// `/sc <mode> <player>`.
pub async fn statcheck(
    h: &Hypixel,
    mode: &str,
    username: &str,
    nicknames: &Nicknames,
) -> ReportResult<Statcheck> {
    let game = constants::game_mode(&mode.to_lowercase())
        .ok_or_else(|| ReportError::line(format!("§cUnknown game mode: {mode}")))?;

    let resolved = resolve_nickname(&clean_rank_prefix(username), nicknames);
    let profile = match h.mojang_uuid(&resolved).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Err(ReportError::line(format!(
                "§cPlayer '{resolved}' not found."
            )));
        }
        Err(e) => {
            return Err(ReportError::api(
                format!("§cPlayer '{resolved}' not found."),
                e,
            ));
        }
    };
    let not_found = || format!("§cCould not retrieve data for '{}'.", profile.username);
    let data = match h.stats(&profile.uuid).await {
        Ok(Some(d)) => d,
        Ok(None) => return Err(ReportError::line(not_found())),
        Err(e) => return Err(ReportError::api(not_found(), e)),
    };
    let has_game = data
        .player
        .get("stats")
        .and_then(|s| s.get(game.api_name))
        .is_some_and(|g| crate::js::truthy(Some(g)));
    if !has_game {
        return Err(ReportError::line(format!(
            "§cNo {} stats found for '{}'.",
            game.display_name, profile.username
        )));
    }

    let (head, guild) = tokio::join!(h.head(&profile.uuid), h.guild(&profile.uuid));
    let head = head.ok().flatten();
    let guild = guild
        .ok()
        .filter(|g| !g.is_empty())
        .map(|g| format!(" §5{g}"))
        .unwrap_or_default();

    let player = Some(data.player.as_ref());
    let s = stat_format::get_mode_stats(player, game);
    let mut lines = vec![
        format!("§8[§5{}§8]", game.display_name),
        format!(
            "{} {}{}{guild}",
            format_rank(player),
            player_name_color(player),
            profile.username
        ),
    ];
    lines.extend(stat_format::format_stat_lines(&s, game.api_name));
    if stat_format::has_winstreaks(game.api_name) {
        lines.push(stat_format::format_winstreak_line(&s));
    }
    Ok(Statcheck {
        profile,
        data,
        game,
        head,
        lines,
    })
}

/// `/sc ?`: every game and its aliases, grouped and sorted by display name.
pub fn statcheck_modes() -> Vec<String> {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for (alias, game) in constants::GAME_MODES {
        match groups
            .iter_mut()
            .find(|(name, _)| *name == game.display_name)
        {
            Some((_, aliases)) => aliases.push(alias),
            None => groups.push((game.display_name, vec![alias])),
        }
    }
    groups.sort_by(|a, b| a.0.cmp(b.0));
    let mut lines = vec![
        DIVIDER.to_string(),
        "§r  §5§lAvailable Statcheck Gamemodes".into(),
        " ".into(),
    ];
    lines.extend(
        groups
            .iter()
            .map(|(name, aliases)| format!("§r  §d{name} §8- §f{}", aliases.join(", "))),
    );
    lines.push(String::new());
    lines.push(DIVIDER.into());
    lines
}

/// `/status <player>` (after the "Checking status…" line).
pub async fn status(h: &Hypixel, username: &str) -> ReportResult<Vec<String>> {
    let profile = match h.mojang_uuid(username).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Err(ReportError::line(format!(
                "§cPlayer '{username}' not found."
            )));
        }
        Err(e) => {
            return Err(ReportError::api(
                format!("§cPlayer '{username}' not found."),
                e,
            ));
        }
    };
    let unavailable = || format!("§cCould not retrieve status for '{}'.", profile.username);
    let status = match h.player_status(&profile.uuid).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err(ReportError::line(unavailable())),
        Err(e) => return Err(ReportError::api(unavailable(), e)),
    };

    let player = Some(status.data.player.as_ref());
    let name_line = format!(
        "{} {}{}",
        format_rank(player),
        player_name_color(player),
        profile.username
    );
    let mut lines = vec![SHORT_DIVIDER.to_string()];
    match status.status {
        OnlineStatus::Online {
            game_type,
            mode,
            map,
        } => {
            lines.push(format!("{name_line} §dOnline"));
            lines.push(format!(
                "§dGame §8» §f{}",
                game_type.as_deref().unwrap_or("undefined")
            ));
            if let Some(mode) = mode.filter(|m| !m.is_empty()) {
                lines.push(format!("§dMode §8» §f{mode}"));
            }
            if let Some(map) = map.filter(|m| !m.is_empty()) {
                lines.push(format!("§dMap §8» §f{map}"));
            }
        }
        OnlineStatus::Hidden => {
            lines.push(format!("{name_line} §dOnline"));
            lines.push("§8(Status is hidden, game info unavailable)".into());
        }
        OnlineStatus::Offline => lines.push(format!("{name_line} §8Offline")),
    }
    lines.push(SHORT_DIVIDER.into());
    Ok(lines)
}

/// Looks up `username` (or returns `None` for "yourself" callers that pass a UUID).
async fn duels_stats(
    h: &Hypixel,
    uuid: &str,
    missing: impl Fn() -> String,
) -> ReportResult<Arc<Value>> {
    let data = h
        .stats(uuid)
        .await
        .map_err(|e| ReportError::api(missing(), e))?;
    let Some(data) = data else {
        return Err(ReportError::line(missing()));
    };
    let has_duels = data
        .player
        .get("stats")
        .and_then(|s| s.get("Duels"))
        .is_some_and(|d| crate::js::truthy(Some(d)));
    if !has_duels {
        return Err(ReportError::line(missing()));
    }
    Ok(data.player)
}

/// `/spread`: wins and losses per Duels mode. `name` adds "for <name>" to the title
/// (the in-game command only ever shows your own spread and leaves it out).
pub async fn spread(h: &Hypixel, uuid: &str, name: Option<&str>) -> ReportResult<Vec<String>> {
    let player = duels_stats(h, uuid, || match name {
        Some(n) => format!("§cCould not fetch Duels stats for {n}."),
        None => "§cCould not fetch your Duels stats.".into(),
    })
    .await?;
    let d = player.pointer("/stats/Duels");

    let rows: Vec<(&str, String, String)> = constants::DUELS_STATS
        .iter()
        .filter_map(|(mode, prefix)| {
            let wins = field(d, &format!("{prefix}_wins"));
            let losses = field(d, &format!("{prefix}_losses"));
            (wins > 0.0 || losses > 0.0).then(|| (*mode, to_locale(wins), to_locale(losses)))
        })
        .collect();
    if rows.is_empty() {
        return Err(ReportError::line("§cNo Duels stats found for any mode."));
    }

    let name_width = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
    let wins_width = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
    let mut lines = vec![DIVIDER.to_string()];
    lines.push(match name {
        Some(n) => format!("  §5§lDuels Mode Spread for {n}"),
        None => "  §5§lDuels Mode Spread".into(),
    });
    for (mode, wins, losses) in rows {
        lines.push(format!(
            "  §5{} §8: §a{} §8| §4{losses}",
            pad_end(mode, name_width),
            pad_start(&wins, wins_width)
        ));
    }
    lines.push(DIVIDER.into());
    Ok(lines)
}

/// `/jtitles`: the Duels title per mode (50+ wins).
pub async fn jtitles(h: &Hypixel, uuid: &str, display_name: &str) -> ReportResult<Vec<String>> {
    let player = duels_stats(h, uuid, || {
        format!("§cCould not fetch Duels stats for {display_name}.")
    })
    .await?;
    let d = player.pointer("/stats/Duels");

    let titles: Vec<(&str, String)> = constants::DUELS_STATS
        .iter()
        .filter_map(|(mode, prefix)| {
            let (division, roman) = constants::duels_title(field(d, &format!("{prefix}_wins")))?;
            let mut name = division.name.to_string();
            name[..1].make_ascii_uppercase();
            Some((*mode, format!("{}{name} {roman}", division.color)))
        })
        .collect();
    if titles.is_empty() {
        return Err(ReportError::line(format!(
            "§c{display_name} doesn't have any Duels titles yet (minimum 50 wins in a mode)."
        )));
    }

    let width = titles
        .iter()
        .map(|t| t.0.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines = vec![
        DIVIDER.to_string(),
        format!("  §5§lDuels Mode Titles for {display_name}"),
    ];
    lines.extend(
        titles
            .iter()
            .map(|(mode, title)| format!("  §5{} §8: {title}", pad_end(mode, width))),
    );
    lines.push(DIVIDER.into());
    Ok(lines)
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// `/playercount ?`: one (longest) name per Duels mode, sorted.
pub fn playercount_modes() -> Vec<String> {
    let mut names: Vec<(&str, &str)> = Vec::new(); // (api key, longest alias)
    for (alias, key) in constants::DUELS_PLAYER_COUNTS {
        match names.iter_mut().find(|(k, _)| k == key) {
            Some(entry) if alias.len() > entry.1.len() => entry.1 = alias,
            Some(_) => {}
            None => names.push((key, alias)),
        }
    }
    names.sort_by(|a, b| a.1.cmp(b.1));
    let mut lines = vec![
        DIVIDER.to_string(),
        "§r  §5§lAvailable Duels Modes for /playercount".into(),
        " ".into(),
    ];
    lines.extend(
        names
            .iter()
            .map(|(_, alias)| format!("§r  §d{}", title_case(alias))),
    );
    lines.push(String::new());
    lines.push(DIVIDER.into());
    lines
}

/// `/playercount <mode>`.
pub async fn playercount(h: &Hypixel, mode: &str) -> ReportResult<Vec<String>> {
    let query = mode.to_lowercase();
    let Some((_, key)) = constants::DUELS_PLAYER_COUNTS
        .iter()
        .find(|(alias, _)| *alias == query)
    else {
        return Err(ReportError::line(format!("§cUnknown duels mode: {query}")));
    };
    let unavailable = "§cCould not retrieve player counts from Hypixel.";
    let games = h
        .player_counts()
        .await
        .map_err(|e| ReportError::api(unavailable, e))?;
    let Some(modes) = games.pointer("/DUELS/modes").filter(|m| m.is_object()) else {
        return Err(ReportError::line(unavailable));
    };
    let count = field(Some(modes), key);
    Ok(vec![format!(
        "§5Duels §8» §f{} Players §8(§d{query}§8)",
        to_locale(count)
    )])
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `/gametrack hour|day` for one Minecraft account: `{ mode: { wins, losses } }`.
pub fn gametrack_modes(title: &str, empty: &str, modes: Option<&Value>) -> Vec<String> {
    let mut lines = vec![DIVIDER.to_string(), format!("  §5§l{title}")];
    match modes.and_then(Value::as_object).filter(|m| !m.is_empty()) {
        None => lines.push(format!("    §8{empty}")),
        Some(modes) => {
            for (mode, stats) in modes {
                let wins = field(Some(stats), "wins");
                let losses = field(Some(stats), "losses");
                let wlr = if losses == 0.0 {
                    if wins > 0.0 {
                        "Infinite".to_string()
                    } else {
                        "N/A".to_string()
                    }
                } else {
                    to_fixed(wins / losses, 2)
                };
                lines.push(format!(
                    "  §d{} §8- §f{wins} Wins §8| §f{losses} Losses §8| §d{wlr} WLR",
                    capitalize(mode)
                ));
            }
        }
    }
    lines.push(DIVIDER.into());
    lines
}

/// `/gametrack log`: the 10 most recent games.
pub fn gametrack_log(log: &Value) -> Vec<String> {
    let mut lines = vec![DIVIDER.to_string(), "  §5§lRecent Game Log".into()];
    match log.as_array().filter(|l| !l.is_empty()) {
        None => lines.push("    §8No recent games found.".into()),
        Some(entries) => {
            for entry in entries.iter().take(10) {
                let result = entry
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("undefined");
                let color = if result == "win" { "§d" } else { "§8" };
                let time = entry
                    .get("timestamp")
                    .and_then(Value::as_i64)
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                    .unwrap_or_else(|| "Invalid Date".into());
                let mode = capitalize(entry.get("mode").and_then(Value::as_str).unwrap_or(""));
                lines.push(format!(
                    "  §8[{time}] §d{mode} §8» {color}{}",
                    result.to_uppercase()
                ));
            }
        }
    }
    lines.push(DIVIDER.into());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_rank_prefix_matches_node() {
        // Expected values produced by the JS regex in Node.
        assert_eq!(clean_rank_prefix("[MVP+] Bob"), "Bob");
        assert_eq!(clean_rank_prefix("§b[MVP§c++§b] Bob"), "[MVP++] Bob");
        assert_eq!(clean_rank_prefix("[mvp] Bob"), "[mvp] Bob");
        assert_eq!(clean_rank_prefix("§a[VIP] §7Bob"), "Bob");
    }

    #[test]
    fn resolve_nickname_prefers_nickname_then_case_insensitive_name() {
        let nicks: Nicknames = [("RealName".to_string(), "Nick".to_string())]
            .into_iter()
            .collect();
        assert_eq!(resolve_nickname("nick", &nicks), "RealName");
        assert_eq!(resolve_nickname("realname", &nicks), "RealName");
        assert_eq!(resolve_nickname("Other", &nicks), "Other");
    }

    #[test]
    fn statcheck_modes_groups_aliases() {
        let lines = statcheck_modes();
        assert!(lines.contains(&"§r  §dBed Wars §8- §fbedwars, bw".to_string()));
        assert!(
            lines.iter().position(|l| l.contains("Arcade"))
                < lines.iter().position(|l| l.contains("Wool Wars"))
        );
    }

    #[test]
    fn playercount_modes_uses_the_longest_alias() {
        let lines = playercount_modes();
        assert!(lines.contains(&"§r  §dBed Wars Duel".to_string()));
        assert!(!lines.contains(&"§r  §dBw".to_string()));
    }

    #[test]
    fn gametrack_modes_formats_wlr() {
        let data = serde_json::json!({ "bw": { "wins": 3, "losses": 0 }, "sumo": { "wins": 1, "losses": 4 } });
        let lines = gametrack_modes("T", "none", Some(&data));
        assert_eq!(
            lines[2],
            "  §dBw §8- §f3 Wins §8| §f0 Losses §8| §dInfinite WLR"
        );
        assert_eq!(
            lines[3],
            "  §dSumo §8- §f1 Wins §8| §f4 Losses §8| §d0.25 WLR"
        );
        assert_eq!(gametrack_modes("T", "none", None)[2], "    §8none");
    }
}
