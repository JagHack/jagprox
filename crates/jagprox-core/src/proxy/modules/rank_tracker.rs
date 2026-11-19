//! Port of `rankTracker.js`: in a pre-game queue lobby, announces the rank of players
//! whose names are hidden (`§k`), from the team Hypixel puts them in.

use mcproto::packets::clientbound::PlayerListAction;

use super::{ServerPacket, State};
use crate::constants::GAME_START_MESSAGES;
use crate::log::log;
use crate::proxy::session::Session;

#[derive(Debug, Clone)]
struct Hidden {
    uuid: Option<u128>,
    ign: String,
    rank: String,
    entity_id: i32,
    announced: bool,
}

#[derive(Default)]
pub struct RankTracker {
    players: Vec<Hidden>,
    in_queue: bool,
}

impl RankTracker {
    pub fn reset(&mut self) {
        self.players.clear();
        self.in_queue = false;
    }

    fn by_ign(&mut self, ign: &str) -> Option<&mut Hidden> {
        self.players.iter_mut().find(|p| p.ign == ign)
    }

    fn by_uuid(&mut self, uuid: u128) -> Option<&mut Hidden> {
        self.players.iter_mut().find(|p| p.uuid == Some(uuid))
    }
}

fn announce(player: &mut Hidden, s: &Session) {
    if player.announced || player.rank.is_empty() {
        return;
    }
    player.announced = true;
    s.chat(&format!("§7Hidden player in queue: {}", player.rank));
}

/// "Name has joined (3/8)!", printed in every Hypixel pre-game lobby.
fn is_queue_join(clean: &str) -> bool {
    let Some(body) = clean.strip_suffix(")!") else {
        return false;
    };
    let Some(open) = body.rfind("has joined (") else {
        return false;
    };
    let Some((a, b)) = body[open + "has joined (".len()..].split_once('/') else {
        return false;
    };
    !a.is_empty()
        && !b.is_empty()
        && a.bytes().all(|c| c.is_ascii_digit())
        && b.bytes().all(|c| c.is_ascii_digit())
}

pub fn handle(st: &mut State, s: &Session, packet: &ServerPacket) {
    let ranks = &mut st.ranks;
    match packet {
        // Hypixel sends a respawn on every server switch: new lobby, new player list.
        ServerPacket::JoinGame(_) | ServerPacket::Respawn(_) => ranks.reset(),
        ServerPacket::Chat(chat) => {
            let Some(clean) = &chat.clean else { return };
            if is_queue_join(clean) {
                if !ranks.in_queue {
                    ranks.in_queue = true;
                    log("Rank tracker: queue lobby detected, tracking joining players.");
                    // Players already in the lobby were sent before our own join message.
                    for p in &mut ranks.players {
                        announce(p, s);
                    }
                }
            } else if ranks.in_queue
                && (clean.contains("The game starts in 1 second!")
                    || GAME_START_MESSAGES.iter().any(|m| clean.contains(m)))
            {
                ranks.in_queue = false;
                log("Rank tracker: game started, no longer tracking joining players.");
            }
        }
        ServerPacket::PlayerList(list) => match &list.action {
            PlayerListAction::Add(players) => {
                for p in players.iter().filter(|p| p.name.starts_with("§k")) {
                    let ign = format!("§r{}", p.name);
                    if let Some(existing) = ranks.by_ign(&ign) {
                        existing.uuid = Some(p.uuid);
                    } else if let Some(existing) = ranks.by_uuid(p.uuid) {
                        existing.ign = ign;
                        existing.rank.clear();
                        existing.entity_id = -1;
                    } else {
                        ranks.players.push(Hidden {
                            uuid: Some(p.uuid),
                            ign,
                            rank: String::new(),
                            entity_id: -1,
                            announced: false,
                        });
                    }
                }
            }
            PlayerListAction::Remove(uuids) => {
                for uuid in uuids {
                    if let Some(index) = ranks.players.iter().position(|p| p.uuid == Some(*uuid)) {
                        ranks.players.remove(index);
                    }
                }
            }
            _ => {}
        },
        ServerPacket::SpawnPlayer(spawn) => {
            if let Some(p) = ranks.by_uuid(spawn.uuid) {
                p.entity_id = spawn.entity_id;
            }
        }
        ServerPacket::Teams(teams) => {
            let Some(players) = teams.players.as_ref().filter(|p| !p.is_empty()) else {
                return;
            };
            if !teams.team.starts_with('§') {
                return;
            }
            for ign in players.iter().filter(|i| i.starts_with("§r§k")) {
                let in_queue = ranks.in_queue;
                let player = match ranks.by_ign(ign) {
                    Some(p) => {
                        p.rank = teams.team.clone();
                        p
                    }
                    None => {
                        ranks.players.push(Hidden {
                            uuid: None,
                            ign: ign.clone(),
                            rank: teams.team.clone(),
                            entity_id: -1,
                            announced: false,
                        });
                        ranks.players.last_mut().expect("just pushed")
                    }
                };
                if in_queue {
                    announce(player, s);
                }
            }
        }
        ServerPacket::DestroyEntities(destroy) => {
            ranks
                .players
                .retain(|p| !destroy.entity_ids.contains(&p.entity_id));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_join_lines() {
        assert!(is_queue_join("Notch has joined (3/8)!"));
        assert!(is_queue_join("[MVP+] Notch has joined (12/16)!"));
        assert!(!is_queue_join("Notch has joined the lobby!"));
        assert!(!is_queue_join("Notch has joined (/8)!"));
        assert!(!is_queue_join("Notch has joined (3/8)! extra"));
    }
}
