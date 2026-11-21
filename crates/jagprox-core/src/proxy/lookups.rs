//! The in-game side of `modules/hypixelHandler.js`: stat checks shown in chat, party
//! and queue stat blocks, tab tag data and leaderboards.

use serde_json::{Value, json};

use super::CHAT_PREFIX;
use super::session::Session;
use crate::api::{ApiError, LOOKUP_CONCURRENCY};
use crate::cache::map_limit;
use crate::constants::{self, GameInfo};
use crate::formatter::{format_rank, player_name_color, strip_colors};
use crate::js::to_fixed;
use crate::log::log;
use crate::reports::{self, DIVIDER, Statcheck};
use crate::stat_format;

/// `/sc`: avatar lines (action-bar-free chat position 1, as in the JS), then the stats.
/// Returns what was fetched, for telemetry.
pub async fn statcheck(s: &Session, mode: &str, username: &str) -> Option<Statcheck> {
    if mode.is_empty() || username.is_empty() {
        s.chat("§cUsage: /sc <gamemode> <username>");
        return None;
    }
    let nicknames = s.proxy().config().nicknames();
    match reports::statcheck(s.hypixel(), mode, username, &nicknames).await {
        Ok(sc) => {
            if let Some(head) = &sc.head {
                for line in head.chat_lines() {
                    s.chat_at(
                        &json!({ "text": format!("{CHAT_PREFIX}{line}") }).to_string(),
                        1,
                    );
                }
            }
            for line in &sc.lines {
                s.chat(line);
            }
            Some(sc)
        }
        Err(e) => {
            if let Some(cause) = &e.cause {
                log(format!("Statcheck error: {cause}"));
            }
            s.chat(&e.line);
            None
        }
    }
}

/// `/status <player>`.
pub async fn status(s: &Session, username: &str) {
    s.chat(&format!("§eChecking status for {username}..."));
    match reports::status(s.hypixel(), username).await {
        Ok(lines) => lines.iter().for_each(|l| s.chat(l)),
        Err(e) => s.chat(&e.line),
    }
}

/// One player's block in party/queue stats (JS `getAndFormatPartyPlayerStats`).
async fn player_block(s: &Session, username: &str, game: &GameInfo) -> Option<String> {
    let clean = remove_rank_brackets(&strip_colors(username), false)
        .trim()
        .to_string();
    if clean.is_empty() {
        return None;
    }
    let real = reports::resolve_nickname(&clean, &s.proxy().config().nicknames());
    let profile = match s.hypixel().mojang_uuid(&real).await {
        Ok(Some(p)) => p,
        Ok(None) | Err(_) => return Some(format!("  §c§o'{clean}' not found.")),
    };
    let data = match s.hypixel().stats(&profile.uuid).await {
        Ok(Some(d)) => d,
        Ok(None) | Err(_) => return Some(format!("  §7{} §7- No stats found.", profile.username)),
    };
    let player = Some(data.player.as_ref());
    let stats = stat_format::get_mode_stats(player, game);
    let mut header = format!(
        "  {} {}{}",
        format_rank(player),
        player_name_color(player),
        profile.username
    );
    if !stats.has_stats {
        return Some(format!("{header} §7- No stats found."));
    }
    if let Some(level) = &stats.level_text {
        header.push_str(&format!(" §8[§d{level}§8]"));
    }
    Some(format!(
        "{header}\n    {}",
        stat_format::format_compact_line(&stats, game.api_name)
    ))
}

/// JS `/\[.*?\]\s/g` (`optional_space = false`) or `/\[.*?\]\s?/g` (`true`).
fn remove_rank_brackets(text: &str, optional_space: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let mut search_from = 0;
        // Lazy match: the first ']' (without a line break before it) that is followed
        // by whitespace (or anything, when the space is optional).
        while let Some(rel) = after[search_from..].find(']') {
            let close = search_from + rel;
            if after[..close].contains(['\n', '\r']) {
                break;
            }
            let tail = &after[close + 1..];
            let space = tail.chars().next().filter(|c| c.is_whitespace());
            if space.is_some() || optional_space {
                out.push_str(&rest[..open]);
                rest = &tail[space.map_or(0, char::len_utf8)..];
                continue 'scan;
            }
            search_from = close + 1;
        }
        out.push_str(&rest[..=open]);
        rest = after;
    }
    out.push_str(rest);
    out
}

async fn stat_blocks(s: &Session, names: Vec<String>, game: &'static GameInfo) -> Vec<String> {
    map_limit(names, LOOKUP_CONCURRENCY, |name| {
        let s = s.clone();
        async move { player_block(&s, &name, game).await }
    })
    .await
    .into_iter()
    .flatten()
    .collect()
}

/// `/psc`: the stats of everyone `/party list` named.
pub async fn party_stats(s: &Session, members: Vec<String>, mode: Option<String>) {
    if members.is_empty() {
        s.chat("§cCould not find any party members.");
        return;
    }
    let current = s.state().queue.current_game_key.clone();
    let game = mode
        .as_deref()
        .and_then(constants::game_mode)
        .or_else(|| current.as_deref().and_then(constants::game_mode))
        .unwrap_or_else(|| constants::game_mode("bedwars").expect("bedwars"));
    s.chat(&format!(
        "§dFound §f{} §dmembers. Fetching stats for §5{}§d...",
        members.len(),
        game.display_name
    ));
    let blocks = stat_blocks(s, members, game).await;
    s.chat(&format!(
        "{DIVIDER}\n  §5§lParty Stats for §d{}\n \n{}\n{DIVIDER}",
        game.display_name,
        blocks.join("\n \n")
    ));
}

/// After `/who`: tab tags plus one chat summary of everyone in the game.
pub async fn queue_bulk(s: &Session, names: Vec<String>, game_key: Option<String>) {
    let key = game_key.unwrap_or_else(|| "bedwars".into());
    let game = constants::game_mode(&key)
        .unwrap_or_else(|| constants::game_mode("bedwars").expect("bedwars"));
    let own = s.username().to_string();
    let nicknames = s.proxy().config().nicknames();
    // Resolved before the self-filter: chat shows your own nickname, not `own`.
    let names: Vec<String> = names
        .iter()
        .map(|n| {
            reports::resolve_nickname(
                remove_rank_brackets(&strip_colors(n), true).trim(),
                &nicknames,
            )
        })
        .filter(|n| !n.is_empty() && *n != own)
        .collect();
    if names.is_empty() {
        return;
    }
    s.chat(&format!(
        "§dAuto-checking §f{} §dplayers for §5{}§d...",
        names.len(),
        game.display_name
    ));
    // Tags and the summary run side by side; the stats cache makes them share requests.
    let tags = s.clone();
    let tag_names = names.clone();
    let tag_key = key.clone();
    s.spawn(async move {
        super::modules::tab_manager::update_player_tags(&tags, tag_names, &tag_key).await
    });

    let blocks = stat_blocks(s, names, game).await;
    if blocks.is_empty() {
        return;
    }
    s.chat(&format!(
        "{DIVIDER}\n  §5§lGame Stats §8({})\n \n{}\n{DIVIDER}",
        game.display_name,
        blocks.join("\n \n")
    ));
}

fn level_color(level: f64) -> &'static str {
    match level {
        l if l >= 500.0 => "§5",
        l if l >= 350.0 => "§4",
        l if l >= 200.0 => "§c",
        l if l >= 100.0 => "§e",
        l if l >= 50.0 => "§f",
        _ => "§7",
    }
}

fn ratio_color(ratio: f64) -> &'static str {
    match ratio {
        r if r >= 20.0 => "§5",
        r if r >= 10.0 => "§4",
        r if r >= 5.0 => "§c",
        r if r >= 3.0 => "§e",
        r if r >= 1.0 => "§f",
        _ => "§7",
    }
}

/// Tab prefix/suffix for a player (each at most 16 characters), or `None`.
pub async fn tab_data(s: &Session, name: &str, game_key: &str) -> Option<(String, String)> {
    let game = constants::game_mode(game_key)?;
    let profile = s.hypixel().mojang_uuid(name).await.ok()??;
    let data = s.hypixel().stats(&profile.uuid).await.ok()??;
    let stats = stat_format::get_mode_stats(Some(&data.player), game);
    if !stats.has_stats {
        return None;
    }
    let cut = |t: String| crate::js::take_chars(&t, 16).to_string();
    let suffix_for = |ratio: f64| {
        let text = to_fixed(ratio, 2);
        let parsed: f64 = text.parse().unwrap_or(0.0);
        format!(" §8|{}{text}", ratio_color(parsed))
    };
    let level = stats.level.unwrap_or(0.0);
    let level_text = stats.level_text.clone().unwrap_or_default();
    match game.api_name {
        "Bedwars" => Some((
            cut(format!("{}[{level_text}] ", level_color(level))),
            cut(suffix_for(stat_format::ratio(
                stats.final_kills,
                stats.final_deaths,
            ))),
        )),
        "SkyWars" => Some((
            cut(format!("{}[{level_text}] ", level_color(level))),
            cut(suffix_for(stat_format::ratio(stats.kills, stats.deaths))),
        )),
        "Duels" => Some((
            String::new(),
            cut(suffix_for(stat_format::ratio(stats.wins, stats.losses))),
        )),
        _ => None,
    }
}

/// `/leaderboard duels <monthly wins|weekly wins>`.
pub async fn leaderboard(
    s: &Session,
    game: &str,
    kind: &str,
) -> Result<(String, Vec<String>), String> {
    let (prefix, title) = kind
        .split_once(' ')
        .ok_or_else(|| format!("Invalid leaderboard type '{kind}'."))?;
    let boards = s
        .hypixel()
        .leaderboards()
        .await
        .map_err(|_: ApiError| "Failed to fetch leaderboards from Hypixel.".to_string())?;
    let list = boards
        .get(game.to_uppercase())
        .and_then(Value::as_array)
        .ok_or_else(|| format!("No leaderboards found for game: {game}"))?;
    let board = list
        .iter()
        .find(|lb| {
            let field = |k: &str| lb.get(k).and_then(Value::as_str).map(str::to_lowercase);
            field("prefix").as_deref() == Some(&prefix.to_lowercase())
                && field("title").as_deref() == Some(&title.to_lowercase())
        })
        .ok_or_else(|| format!("No '{kind}' leaderboard found for {game}."))?;
    let uuids: Vec<String> = board
        .get("leaders")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .take(10)
                .filter_map(|u| u.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let names = map_limit(uuids, LOOKUP_CONCURRENCY, |uuid| {
        let h = s.hypixel().clone();
        async move { h.username_from_uuid(&uuid).await.unwrap_or(uuid) }
    })
    .await;
    let text = |k: &str| {
        board
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    Ok((
        format!("{} {} for {game}", text("prefix"), text("title")),
        names,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_brackets_match_the_js_regexes() {
        assert_eq!(remove_rank_brackets("[MVP+] Bob", false), "Bob");
        assert_eq!(remove_rank_brackets("[MVP+]Bob", false), "[MVP+]Bob");
        assert_eq!(remove_rank_brackets("[MVP+]Bob", true), "Bob");
        assert_eq!(remove_rank_brackets("[a] [b] Bob", false), "Bob");
        assert_eq!(remove_rank_brackets("Bob [x", true), "Bob [x");
    }

    #[test]
    fn tab_colors() {
        assert_eq!(level_color(523.0), "§5");
        assert_eq!(level_color(12.0), "§7");
        assert_eq!(ratio_color(3.0), "§e");
        assert_eq!(ratio_color(0.99), "§7");
    }
}
