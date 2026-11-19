//! Prequeue ranks: Hypixel hides the players you're queued against behind an
//! obfuscated (`§k`) name, but sorts them into a scoreboard team named by their rank
//! colour. This announces each one's rank once ("One of your opponents is a
//! §6MVP§c++").
//!
//! Follows Jaglient's player directory (agent/core/src/player_directory.cpp), which
//! was checked live. Unlike the old JS rank tracker:
//! - players are not forgotten when Hypixel removes them from the tab list (it does
//!   that almost immediately: add → team → remove);
//! - there is no wait for a "has joined (x/y)!" line before announcing;
//! - the rank comes from the team name's colour, mapped to a tier label.

use mcproto::packets::clientbound::PlayerListAction;

use super::{ServerPacket, State};
use crate::log::log;
use crate::proxy::session::Session;

#[derive(Debug, Clone)]
struct Hidden {
    uuid: Option<u128>,
    /// "§r" + the obfuscated profile name (how teams list the player).
    ign: String,
    /// The scoreboard team name; its colour is the rank tier.
    rank: String,
    announced: bool,
}

#[derive(Default)]
pub struct RankTracker {
    players: Vec<Hidden>,
}

impl RankTracker {
    pub fn reset(&mut self) {
        self.players.clear();
    }

    fn by_ign(&mut self, ign: &str) -> Option<&mut Hidden> {
        self.players.iter_mut().find(|p| p.ign == ign)
    }

    fn by_uuid(&mut self, uuid: u128) -> Option<&mut Hidden> {
        self.players.iter_mut().find(|p| p.uuid == Some(uuid))
    }
}

/// The first colour code (0-9, a-f) in `text`, skipping format codes like `§k`.
fn first_colour(text: &str) -> Option<char> {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            let code = chars.next()?.to_ascii_lowercase();
            if code.is_ascii_hexdigit() {
                return Some(code);
            }
        }
    }
    None
}

/// The rank label for a team name's colour, or `None` if it isn't a rank colour.
/// The colour can't tell VIP from VIP+ or MVP from MVP+, so those say both.
pub fn rank_label(team: &str) -> Option<&'static str> {
    Some(match first_colour(team)? {
        '7' | '8' => "§7Non",
        'a' => "§aVIP/VIP§6+",
        'b' => "§bMVP/MVP§c+",
        '6' => "§6MVP§c++",
        'c' => "§cYT",
        _ => return None,
    })
}

fn announce_pending(ranks: &mut RankTracker, s: &Session) {
    for p in &mut ranks.players {
        if p.announced || p.uuid.is_none() || p.rank.is_empty() {
            continue;
        }
        let Some(label) = rank_label(&p.rank) else {
            continue;
        };
        p.announced = true;
        log(format!(
            "Prequeue rank: {} is {}",
            p.ign,
            crate::formatter::strip_colors(label)
        ));
        s.chat(&format!("§eOne of your opponents is a {label}"));
    }
}

pub fn handle(st: &mut State, s: &Session, packet: &ServerPacket) {
    let ranks = &mut st.ranks;
    match packet {
        ServerPacket::PlayerList(list) => {
            // Removals are ignored on purpose: Hypixel drops hidden opponents from the
            // tab list right away, but they're still in the game.
            let PlayerListAction::Add(players) = &list.action else {
                return;
            };
            for p in players.iter().filter(|p| p.name.starts_with("§k")) {
                let ign = format!("§r{}", p.name);
                if let Some(existing) = ranks.by_ign(&ign) {
                    existing.uuid = Some(p.uuid);
                } else if let Some(existing) = ranks.by_uuid(p.uuid) {
                    existing.ign = ign;
                } else {
                    ranks.players.push(Hidden {
                        uuid: Some(p.uuid),
                        ign,
                        rank: String::new(),
                        announced: false,
                    });
                }
            }
            announce_pending(ranks, s);
        }
        ServerPacket::Teams(teams) => {
            let Some(members) = teams.players.as_ref().filter(|p| !p.is_empty()) else {
                return;
            };
            if !teams.team.starts_with('§') {
                return;
            }
            for ign in members.iter().filter(|i| i.starts_with("§r§k")) {
                match ranks.by_ign(ign) {
                    Some(p) => p.rank = teams.team.clone(),
                    None => ranks.players.push(Hidden {
                        uuid: None,
                        ign: ign.clone(),
                        rank: teams.team.clone(),
                        announced: false,
                    }),
                }
            }
            announce_pending(ranks, s);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_labels_from_team_colours() {
        assert_eq!(rank_label("§6[MVP§c++§6]"), Some("§6MVP§c++"));
        assert_eq!(rank_label("§b"), Some("§bMVP/MVP§c+"));
        assert_eq!(
            rank_label("§l§aVIP"),
            Some("§aVIP/VIP§6+"),
            "format codes are skipped"
        );
        assert_eq!(rank_label("§7"), Some("§7Non"));
        assert_eq!(rank_label("§8zzz"), Some("§7Non"));
        assert_eq!(rank_label("§9blue"), None);
        assert_eq!(rank_label("plain"), None);
    }
}
