//! The packet modules, run on every packet Hypixel sends that JagProx inspects, in the
//! same order as the JS proxy: nicknames → queue stats → tab tags → tab alerts →
//! Auto GG → rank tracker → map footer → brand.

pub mod auto_gg;
pub mod gametrack;
pub mod party;
pub mod queue_stats;
pub mod rank_tracker;
pub mod super_friends;
pub mod tab_alerter;
pub mod tab_manager;

use mcproto::packets::clientbound as cb;
use mcproto::packets::{Packet, vanilla_brand};
use serde_json::{Value, json};

use super::ProxyEvent;
use super::session::Session;
use crate::formatter;
use crate::nicknames::{replace_names_in_component, replace_names_in_text};

/// Everything the modules remember during one session.
#[derive(Default)]
pub struct State {
    pub queue: queue_stats::QueueStats,
    pub tab: tab_manager::TabManager,
    pub alerter: tab_alerter::TabAlerter,
    pub auto_gg: auto_gg::AutoGg,
    pub ranks: rank_tracker::RankTracker,
    pub gametrack: gametrack::Gametrack,
    pub party: Option<party::PartyCapture>,
    /// The last `/play` command, for `/rq`.
    pub last_play_command: Option<String>,
}

/// A server chat message, with its JSON parsed once for every module.
pub struct ChatMessage {
    pub packet: cb::Chat,
    pub json: Option<Value>,
    /// Plain text (colors stripped, trimmed), `None` if the message isn't JSON.
    pub clean: Option<String>,
}

/// The packets from Hypixel that JagProx reads.
pub enum ServerPacket {
    JoinGame(cb::JoinGame),
    Chat(ChatMessage),
    Respawn(cb::Respawn),
    Position(cb::PlayerPositionAndLook),
    SpawnPlayer(cb::SpawnPlayer),
    DestroyEntities(cb::DestroyEntities),
    PlayerList(cb::PlayerListItem),
    Objective(cb::ScoreboardObjective),
    Teams(cb::Teams),
    Plugin(cb::PluginMessage),
    Kick(cb::Disconnect),
    Title(cb::Title),
    HeaderFooter(cb::PlayerListHeaderFooter),
}

impl ServerPacket {
    /// Decodes a packet JagProx inspects; `None` for every other id.
    pub fn decode(id: i32, payload: &[u8]) -> Option<mcproto::Result<Self>> {
        fn wrap<P: Packet>(
            payload: &[u8],
            f: fn(P) -> ServerPacket,
        ) -> mcproto::Result<ServerPacket> {
            P::from_payload(payload).map(f)
        }
        Some(match id {
            cb::JoinGame::ID => wrap(payload, ServerPacket::JoinGame),
            cb::Chat::ID => cb::Chat::from_payload(payload).map(|packet| {
                let json = serde_json::from_str::<Value>(&packet.message).ok();
                let clean = json.as_ref().map(|j| {
                    formatter::strip_colors(&formatter::extract_text(j))
                        .trim()
                        .to_string()
                });
                ServerPacket::Chat(ChatMessage {
                    packet,
                    json,
                    clean,
                })
            }),
            cb::Respawn::ID => wrap(payload, ServerPacket::Respawn),
            cb::PlayerPositionAndLook::ID => wrap(payload, ServerPacket::Position),
            cb::SpawnPlayer::ID => {
                cb::SpawnPlayer::decode_payload(payload).map(ServerPacket::SpawnPlayer)
            }
            cb::DestroyEntities::ID => wrap(payload, ServerPacket::DestroyEntities),
            cb::PlayerListItem::ID => wrap(payload, ServerPacket::PlayerList),
            cb::ScoreboardObjective::ID => wrap(payload, ServerPacket::Objective),
            cb::Teams::ID => wrap(payload, ServerPacket::Teams),
            cb::PluginMessage::ID => wrap(payload, ServerPacket::Plugin),
            cb::Disconnect::ID => wrap(payload, ServerPacket::Kick),
            cb::Title::ID => wrap(payload, ServerPacket::Title),
            cb::PlayerListHeaderFooter::ID => wrap(payload, ServerPacket::HeaderFooter),
            _ => return None,
        })
    }

    /// Re-encodes a packet a module changed. Spawn-player packets are never changed.
    pub fn encode(&self) -> Option<Vec<u8>> {
        Some(match self {
            ServerPacket::JoinGame(p) => p.to_payload(),
            ServerPacket::Chat(c) => c.packet.to_payload(),
            ServerPacket::Respawn(p) => p.to_payload(),
            ServerPacket::Position(p) => p.to_payload(),
            ServerPacket::SpawnPlayer(_) => return None,
            ServerPacket::DestroyEntities(p) => p.to_payload(),
            ServerPacket::PlayerList(p) => p.to_payload(),
            ServerPacket::Objective(p) => p.to_payload(),
            ServerPacket::Teams(p) => p.to_payload(),
            ServerPacket::Plugin(p) => p.to_payload(),
            ServerPacket::Kick(p) => p.to_payload(),
            ServerPacket::Title(p) => p.to_payload(),
            ServerPacket::HeaderFooter(p) => p.to_payload(),
        })
    }
}

/// Runs every module on a packet from Hypixel. Returns whether it was changed.
pub fn on_server_packet(s: &Session, packet: &mut ServerPacket) -> bool {
    let mut changed = false;
    let nicknames = s.proxy().config().nicknames();

    match packet {
        ServerPacket::Chat(chat) => changed |= on_chat(s, chat, &nicknames),
        ServerPacket::PlayerList(list) if !nicknames.is_empty() => {
            changed |= nickname_player_list(list, &nicknames)
        }
        _ => {}
    }

    {
        let mut st = s.state();
        changed |= queue_stats::handle(&mut st, s, packet);
        changed |= tab_manager::handle(&mut st, packet);
        tab_alerter::handle(&mut st, s, packet);
        auto_gg::handle(&mut st, s, packet);
        rank_tracker::handle(&mut st, s, packet);
        party::handle(&mut st, s, packet);
    }

    match packet {
        ServerPacket::HeaderFooter(hf) => {
            changed |= brand_tab_header(hf);
            changed |= inject_map_into_footer(s, hf);
        }
        ServerPacket::Plugin(p) if p.channel == "MC|Brand" => {
            p.data = vanilla_brand();
            changed = true;
        }
        _ => {}
    }
    changed
}

/// Nicknames in chat, the launcher's chat mirror and game results.
fn on_chat(s: &Session, chat: &mut ChatMessage, nicknames: &crate::nicknames::Nicknames) -> bool {
    let mut changed = false;
    match &mut chat.json {
        None => {
            crate::log::debug("Non-JSON chat message, passing through.");
            gametrack::on_chat(s, &chat.packet.message);
        }
        Some(json) => {
            if !nicknames.is_empty() {
                replace_names_in_component(json, nicknames);
                chat.packet.message = json.to_string();
                chat.clean = Some(
                    formatter::strip_colors(&formatter::extract_text(json))
                        .trim()
                        .to_string(),
                );
                changed = true;
            }
            if chat.packet.position == 0 || chat.packet.position == 1 {
                let _ = s
                    .proxy()
                    .events()
                    .send(ProxyEvent::Chat(formatter::reconstruct_legacy_text(json)));
            }
            gametrack::on_chat(s, &formatter::extract_text(json));
        }
    }
    changed
}

/// Nicknames in the tab list.
fn nickname_player_list(
    list: &mut cb::PlayerListItem,
    nicknames: &crate::nicknames::Nicknames,
) -> bool {
    let mut changed = false;
    let mut apply = |name: &str, display: &mut Option<String>| {
        let Some(nickname) = nicknames.get(name) else {
            return;
        };
        match display {
            None => *display = Some(json!({ "text": nickname }).to_string()),
            Some(current) if current.contains(name) => {
                let mapping: crate::nicknames::Nicknames =
                    [(name.to_string(), nickname.clone())].into_iter().collect();
                *current = match serde_json::from_str::<Value>(current) {
                    Ok(mut component) => {
                        replace_names_in_component(&mut component, &mapping);
                        component.to_string()
                    }
                    Err(_) => replace_names_in_text(current, &mapping),
                };
            }
            Some(_) => return,
        }
        changed = true;
    };
    match &mut list.action {
        cb::PlayerListAction::Add(players) => {
            for p in players {
                apply(&p.name.clone(), &mut p.display_name);
            }
        }
        // The JS looked up `player.name`, which update packets don't carry: a no-op.
        cb::PlayerListAction::UpdateDisplayName(_) => {}
        _ => {}
    }
    changed
}

/// Hypixel's tab header starts "You are playing on §eMC.HYPIXEL.NET"; add
/// "§e through §5JagProx!" to that first line so it's clear the proxy is in the way.
fn brand_tab_header(hf: &mut cb::PlayerListHeaderFooter) -> bool {
    let header =
        serde_json::from_str::<Value>(&hf.header).unwrap_or_else(|_| json!({ "text": hf.header }));
    let text = formatter::reconstruct_legacy_text(&header);
    // Only touch Hypixel's "playing on" header, and never add our line twice.
    if !text.contains("HYPIXEL.NET") || text.contains("JagProx") {
        return false;
    }
    let (first, rest) = text
        .split_once('\n')
        .map_or((text.as_str(), None), |(a, b)| (a, Some(b)));
    let mut branded = format!("{first}§e through §5JagProx!");
    if let Some(rest) = rest {
        branded.push('\n');
        branded.push_str(rest);
    }
    hf.header = json!({ "text": branded }).to_string();
    true
}

/// "You are currently playing on <map>" under the tab list.
fn inject_map_into_footer(s: &Session, hf: &mut cb::PlayerListHeaderFooter) -> bool {
    let Some(map) = s.state().queue.current_map.clone() else {
        return false;
    };
    let mut footer =
        serde_json::from_str::<Value>(&hf.footer).unwrap_or_else(|_| json!({ "text": hf.footer }));
    let line = json!({ "text": format!("\n§7You are currently playing on §5{map}") });
    match footer.get_mut("extra").and_then(Value::as_array_mut) {
        Some(extra) => extra.push(line),
        None => footer = json!({ "text": "", "extra": [footer, line] }),
    }
    hf.footer = footer.to_string();
    true
}

/// Resets on disconnect (JS `resetModules`).
pub fn reset(st: &mut State) {
    st.queue.reset();
    st.tab.reset();
    st.alerter.reset();
    st.auto_gg.reset();
    st.ranks.reset();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(text: &str) -> cb::PlayerListHeaderFooter {
        cb::PlayerListHeaderFooter {
            header: json!({ "text": text }).to_string(),
            footer: json!({ "text": "" }).to_string(),
        }
    }

    #[test]
    fn brands_the_hypixel_header() {
        let mut hf = header("§eYou are playing on §6MC.HYPIXEL.NET");
        assert!(brand_tab_header(&mut hf));
        let shown = formatter::reconstruct_legacy_text(&serde_json::from_str(&hf.header).unwrap());
        assert_eq!(
            shown,
            "§eYou are playing on §6MC.HYPIXEL.NET§e through §5JagProx!"
        );
    }

    #[test]
    fn works_on_hypixels_component_format() {
        // Hypixel sends colour fields, not § codes; branding must keep its colours.
        let mut hf = cb::PlayerListHeaderFooter {
            header: json!({
                "text": "",
                "extra": [
                    { "text": "You are playing on ", "color": "yellow" },
                    { "text": "MC.HYPIXEL.NET", "color": "gold" }
                ]
            })
            .to_string(),
            footer: json!({ "text": "" }).to_string(),
        };
        assert!(brand_tab_header(&mut hf));
        let shown = formatter::reconstruct_legacy_text(&serde_json::from_str(&hf.header).unwrap());
        assert_eq!(
            shown,
            "§eYou are playing on §6MC.HYPIXEL.NET§e through §5JagProx!"
        );
    }

    #[test]
    fn keeps_lines_after_the_first() {
        // In-game, line 2 is the player's name; it must survive untouched.
        let mut hf = header("§eYou are playing on §6MC.HYPIXEL.NET\n§7Steve");
        assert!(brand_tab_header(&mut hf));
        let shown = formatter::reconstruct_legacy_text(&serde_json::from_str(&hf.header).unwrap());
        assert_eq!(
            shown,
            "§eYou are playing on §6MC.HYPIXEL.NET§e through §5JagProx!\n§7Steve"
        );
    }

    #[test]
    fn is_idempotent_and_leaves_other_headers_alone() {
        let mut once = header("§eYou are playing on §6MC.HYPIXEL.NET");
        brand_tab_header(&mut once);
        assert!(!brand_tab_header(&mut once), "must not brand twice");

        let mut other = header("§aSome other server");
        assert!(!brand_tab_header(&mut other));
    }
}
