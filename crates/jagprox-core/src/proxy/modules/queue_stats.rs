//! Port of `queueStatsHandler.js`: works out the current game from the sidebar title,
//! and at game start sends `/wtfmap` and (after the teleport) `/who`, then stat-checks
//! everyone in the game. Duels opponents are checked from the "Opponent:" line.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use super::{ServerPacket, State};
use crate::chat_parsers::{parse_online_line, parse_team_line};
use crate::constants::{GAME_MODES, GAME_START_MESSAGES, game_mode};
use crate::formatter::strip_colors;
use crate::log::{debug, log};
use crate::proxy::lookups;
use crate::proxy::session::Session;

const GAME_OVER_KEYWORDS: &[&str] = &[
    "VICTORY!",
    "GAME END",
    "You died!",
    "You have been eliminated!",
    "You won!",
    "Draw!",
];
const WHO_CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);

/// Sidebar title → game key. When several keys share a display name the last one
/// wins ("BED WARS" → "bw", "SKYWARS" → "sw"); gametrack modes depend on these keys.
fn title_to_key() -> &'static HashMap<String, &'static str> {
    static MAP: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        GAME_MODES
            .iter()
            .map(|(key, info)| (info.display_name.to_uppercase(), *key))
            .collect()
    })
}

#[derive(Default)]
pub struct QueueStats {
    pub current_game_key: Option<String>,
    pub has_triggered: bool,
    pub awaiting_teleport: bool,
    pub current_map: Option<String>,
    /// Players collected from `/who` while capturing, with a capture id.
    who: Option<(u64, Vec<String>)>,
    next_capture: u64,
}

impl QueueStats {
    /// Game over: allow the next game to trigger again.
    pub fn reset_trigger(&mut self) {
        if self.has_triggered {
            log("Game over detected. Re-arming queue stats trigger for the next game.");
            self.has_triggered = false;
            self.awaiting_teleport = false;
            self.current_map = None;
        }
    }

    fn start_who_capture(&mut self) -> u64 {
        self.next_capture += 1;
        self.who = Some((self.next_capture, Vec::new()));
        self.next_capture
    }

    fn capturing(&self) -> bool {
        self.who.is_some()
    }
}

/// queue_stats can be keyed by any alias of the game ("skywars" or "sw").
fn is_enabled_for(s: &Session, key: &str) -> bool {
    let Some(info) = game_mode(key) else {
        return false;
    };
    let settings = s.proxy().config().get("queue_stats").unwrap_or_default();
    GAME_MODES.iter().any(|(alias, other)| {
        other.display_name == info.display_name && crate::js::truthy(settings.get(*alias))
    })
}

fn is_duels(key: Option<&str>) -> bool {
    key.and_then(game_mode)
        .is_some_and(|g| g.api_name == "Duels")
}

/// The game changed: new gametrack session, and per-game state starts over.
fn reset_for_new_game(st: &mut State, s: &Session, key: &str) {
    log(format!("Game context set to: \"{key}\"."));
    st.queue.current_game_key = Some(key.to_string());
    st.queue.has_triggered = false;
    st.queue.awaiting_teleport = false;
    st.queue.current_map = None;
    // JS `proxy.onGameChanged`. Team/tab state is cleared on the server-switch respawn.
    super::gametrack::on_game_changed(st, s, key);
    st.queue.reset_trigger();
    st.alerter.reset();
    st.auto_gg.reset();
}

pub fn handle(st: &mut State, s: &Session, packet: &mut ServerPacket) -> bool {
    match packet {
        ServerPacket::Kick(_) => {
            st.queue.current_game_key = None;
            st.queue.has_triggered = false;
            st.queue.awaiting_teleport = false;
            st.queue.who = None;
        }
        // A server switch: the map belongs to the server just left.
        ServerPacket::JoinGame(_) | ServerPacket::Respawn(_) => st.queue.current_map = None,
        ServerPacket::Position(_) if st.queue.awaiting_teleport => {
            st.queue.awaiting_teleport = false;
            // /who is only useful for Bed Wars and SkyWars; Duels uses the "Opponent:" line.
            if is_duels(st.queue.current_game_key.as_deref()) {
                log("Skipping /who for Duels mode.");
                st.queue.has_triggered = true;
            } else {
                log("Teleport confirmed. Executing /who after a short safety delay.");
                let session = s.clone();
                s.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let id = session.state().queue.start_who_capture();
                    session.send_server_chat("/who");
                    tokio::time::sleep(WHO_CAPTURE_TIMEOUT).await;
                    // If Hypixel never answers, stop capturing instead of swallowing chat.
                    let players = {
                        let mut st = session.state();
                        match &st.queue.who {
                            Some((capture, _)) if *capture == id => {
                                st.queue.who.take().map(|(_, p)| p)
                            }
                            _ => None,
                        }
                    };
                    if let Some(players) = players {
                        let key = session.state().queue.current_game_key.clone();
                        finish_who(&session, players, key);
                    }
                });
            }
        }
        ServerPacket::Objective(objective) if objective.mode == 0 => {
            if let Some((display, _)) = &objective.display {
                let title = strip_colors(display).trim().to_uppercase();
                if let Some(key) = title_to_key().get(&title).copied() {
                    if st.queue.current_game_key.as_deref() != Some(key) {
                        reset_for_new_game(st, s, key);
                    }
                    st.queue.has_triggered = false;
                    // Lobby and game share a title ("BED WARS"), so the key alone can't re-arm.
                    st.auto_gg.reset();
                }
            }
        }
        ServerPacket::Chat(chat) => {
            if let Some(clean) = chat.clean.clone() {
                handle_chat(st, s, &clean);
            }
        }
        _ => {}
    }
    false
}

/// The end of the first `needle` that starts a line (only whitespace before it), so
/// player chat ("Bob: Opponent: X") can't pass for Hypixel's own lines.
fn after_line_start(clean: &str, needle: &str) -> Option<usize> {
    clean
        .match_indices(needle)
        .map(|(i, _)| i)
        .find(|&i| {
            clean[..i]
                .rsplit(['\n', '\r'])
                .next()
                .unwrap_or("")
                .trim()
                .is_empty()
        })
        .map(|i| i + needle.len())
}

/// JS `/You are currently playing on (?:map: )?(.+)/`.
fn map_name(clean: &str) -> Option<String> {
    let start = after_line_start(clean, "You are currently playing on ")?;
    let rest = &clean[start..];
    let rest = rest
        .strip_prefix("map: ")
        .filter(|r| !r.is_empty() && !r.starts_with('\n'))
        .unwrap_or(rest);
    let line = rest.split(['\n', '\r']).next().unwrap_or("");
    (!line.is_empty()).then(|| line.trim().to_string())
}

/// JS `/Opponent: (.+)/`.
fn opponent(clean: &str) -> Option<String> {
    let start = after_line_start(clean, "Opponent: ")?;
    let line = clean[start..].split(['\n', '\r']).next().unwrap_or("");
    (!line.is_empty()).then(|| line.trim().to_string())
}

fn handle_chat(st: &mut State, s: &Session, clean: &str) {
    if GAME_OVER_KEYWORDS.iter().any(|k| clean.contains(k)) {
        st.queue.reset_trigger();
    }

    if let Some(map) = map_name(clean) {
        log(format!("Captured map name: \"{map}\""));
        st.queue.current_map = Some(map);
        return;
    }

    if let Some(name) = opponent(clean) {
        let game = st.queue.current_game_key.clone();
        debug(format!(
            "Opponent line: \"{clean}\" -> \"{name}\" (game \"{game:?}\")"
        ));
        if !name.is_empty() && name != s.username() {
            log(format!("Auto-checking Duels stats for {name}..."));
            let session = s.clone();
            s.spawn(async move {
                lookups::statcheck(&session, game.as_deref().unwrap_or(""), &name).await;
            });
        }
    }

    if let Some(players) = parse_online_line(clean) {
        if let Some((_, captured)) = &mut st.queue.who {
            captured.extend(players);
            let players = st.queue.who.take().map(|(_, p)| p).unwrap_or_default();
            finish_who(s, players, st.queue.current_game_key.clone());
        } else if st.queue.current_game_key.as_deref() == Some("bw") && !players.is_empty() {
            log(format!(
                "ONLINE: update received, refreshing tab display for {} players.",
                players.len()
            ));
            let session = s.clone();
            s.spawn(async move {
                super::tab_manager::update_player_tags(&session, players, "bw").await
            });
        }
        return;
    }

    if st.queue.capturing() {
        if let Some(players) = parse_team_line(clean) {
            if let Some((_, captured)) = &mut st.queue.who {
                captured.extend(players);
            }
            return;
        }
        if clean.is_empty() && st.queue.who.as_ref().is_some_and(|(_, p)| !p.is_empty()) {
            let players = st.queue.who.take().map(|(_, p)| p).unwrap_or_default();
            finish_who(s, players, st.queue.current_game_key.clone());
            return;
        }
    }

    if !st.queue.has_triggered
        && let Some(key) = st.queue.current_game_key.clone()
        && GAME_START_MESSAGES.iter().any(|m| clean.contains(m))
        && is_enabled_for(s, &key)
    {
        st.queue.has_triggered = true;
        st.queue.awaiting_teleport = true;
        log(format!(
            "Game start message detected for '{key}'. Awaiting teleport to execute /who."
        ));
        let session = s.clone();
        s.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            session.send_server_chat("/wtfmap");
            log("Game start detected. Sending /wtfmap to capture map name.");
        });
    }
}

/// Called with the state locked, so it must not lock it again.
fn finish_who(s: &Session, players: Vec<String>, key: Option<String>) {
    log(format!("Captured {} players from /who.", players.len()));
    if players.is_empty() {
        return;
    }
    let session = s.clone();
    s.spawn(async move { lookups::queue_bulk(&session, players, key).await });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_map_to_the_last_alias() {
        assert_eq!(title_to_key().get("BED WARS"), Some(&"bw"));
        assert_eq!(title_to_key().get("SKYWARS"), Some(&"sw"));
        assert_eq!(title_to_key().get("DUELS"), Some(&"duels"));
    }

    #[test]
    fn map_and_opponent_lines() {
        assert_eq!(
            map_name("You are currently playing on Lighthouse").as_deref(),
            Some("Lighthouse")
        );
        assert_eq!(
            map_name("You are currently playing on map: Aquarium ").as_deref(),
            Some("Aquarium")
        );
        assert_eq!(map_name("nothing here"), None);
        assert_eq!(
            opponent("Opponent: [MVP+] Notch").as_deref(),
            Some("[MVP+] Notch")
        );
        assert_eq!(opponent("Opponent: "), None);
    }

    #[test]
    fn only_hypixels_own_lines_match() {
        let banner = "▬▬▬▬▬▬▬▬▬▬▬▬\n                 Bed Wars Duel\n\n         Opponent: [MVP+] Notch\n▬▬▬▬▬▬▬▬▬▬▬▬";
        assert_eq!(opponent(banner).as_deref(), Some("[MVP+] Notch"));
        assert_eq!(opponent("[VIP] Bob: Opponent: Technoblade"), None);
        assert_eq!(
            map_name("Bob: You are currently playing on §kLOL"),
            None,
            "player chat must not set the map"
        );
    }
}
