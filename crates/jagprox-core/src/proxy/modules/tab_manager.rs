//! Port of `tabManager.js`: stat tags (level prefix, FKDR/KDR/WLR suffix) in the tab
//! list, written into Hypixel's own teams so they survive Hypixel's team updates.

use std::collections::HashMap;
use std::time::Duration;

use mcproto::packets::Packet;
use mcproto::packets::clientbound::{TeamInfo, Teams};

use super::{ServerPacket, State};
use crate::js::take_chars;
use crate::log::{debug, log};
use crate::proxy::lookups;
use crate::proxy::session::Session;

/// Bed Wars team letter (after any § codes in the team prefix) → color code and id.
fn bed_color(hypixel_prefix: &str) -> (&'static str, i8) {
    let mut rest = hypixel_prefix;
    while let Some(after) = rest.strip_prefix('§') {
        let mut chars = after.chars();
        if chars.next().is_none() {
            break;
        }
        rest = chars.as_str();
    }
    match rest.chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('R') => ("§c", 12),
        Some('B') => ("§9", 9),
        Some('G') => ("§a", 10),
        Some('Y') => ("§e", 14),
        Some('A') => ("§b", 11),
        Some('W') => ("§f", 15),
        Some('P') => ("§d", 13),
        Some('S') => ("§8", 8),
        _ => DEFAULT_COLOR,
    }
}

const DEFAULT_COLOR: (&str, i8) = ("§f", 15);

#[derive(Default)]
pub struct TabManager {
    /// lowercase player name → team
    player_team: HashMap<String, String>,
    team_color: HashMap<String, i8>,
    team_prefix: HashMap<String, String>,
    team_suffix: HashMap<String, String>,
    our_prefix: HashMap<String, String>,
    our_suffix: HashMap<String, String>,
    /// Never reset: the client may still hold our old jpN teams, and creating a
    /// team that already exists kicks a 1.8 client.
    team_counter: u64,
}

impl TabManager {
    pub fn reset(&mut self) {
        debug("Player Tag Manager reset.");
        self.player_team.clear();
        self.team_color.clear();
        self.team_prefix.clear();
        self.team_suffix.clear();
        self.our_prefix.clear();
        self.our_suffix.clear();
    }

    fn remove_team(&mut self, team: &str) {
        self.team_color.remove(team);
        self.team_prefix.remove(team);
        self.team_suffix.remove(team);
        self.our_prefix.remove(team);
        self.our_suffix.remove(team);
        self.player_team.retain(|_, t| t != team);
    }
}

pub fn handle(st: &mut State, packet: &mut ServerPacket) -> bool {
    let tab = &mut st.tab;
    let teams = match packet {
        // Hypixel sends a respawn on every server switch; the new server has its own teams.
        ServerPacket::JoinGame(_) | ServerPacket::Respawn(_) => {
            tab.reset();
            return false;
        }
        ServerPacket::Teams(t) => t,
        _ => return false,
    };

    if teams.mode == 1 {
        tab.remove_team(&teams.team);
        return false;
    }
    if let Some(players) = &teams.players {
        match teams.mode {
            0 | 3 => {
                for p in players {
                    tab.player_team.insert(p.to_lowercase(), teams.team.clone());
                }
            }
            4 => {
                for p in players {
                    tab.player_team.remove(&p.to_lowercase());
                }
            }
            _ => {}
        }
    }

    let mut changed = false;
    if let Some(info) = &mut teams.info {
        tab.team_prefix
            .insert(teams.team.clone(), info.prefix.clone());
        tab.team_suffix
            .insert(teams.team.clone(), info.suffix.clone());
        tab.team_color.insert(teams.team.clone(), info.color);

        // Keep our stat tags when Hypixel updates a team we've already tagged.
        if let Some(ours) = tab.our_suffix.get(&teams.team).filter(|s| !s.is_empty()) {
            let base = tab
                .team_suffix
                .get(&teams.team)
                .cloned()
                .unwrap_or_default();
            info.suffix = take_chars(&(base + ours), 16).to_string();
            changed = true;
        }
        if let Some(ours) = tab.our_prefix.get(&teams.team).filter(|p| !p.is_empty()) {
            info.prefix = take_chars(ours, 16).to_string();
            info.color = bed_color(tab.team_prefix.get(&teams.team).map_or("", String::as_str)).1;
            changed = true;
        }
    }
    changed
}

/// Tags each player, 150 ms apart (JS `updatePlayerTags`).
pub async fn update_player_tags(s: &Session, names: Vec<String>, game_key: &str) {
    log(format!(
        "Updating player tab display for {} players...",
        names.len()
    ));
    for name in names {
        tag_player(s, &name, game_key).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    log("Finished updating player tags.");
}

async fn tag_player(s: &Session, name: &str, game_key: &str) {
    if name.is_empty() {
        return;
    }
    let Some((prefix, suffix)) = lookups::tab_data(s, name, game_key).await else {
        return;
    };
    let team = s.state().tab.player_team.get(&name.to_lowercase()).cloned();

    match team {
        Some(team) => {
            let mut hypixel_prefix = s
                .state()
                .tab
                .team_prefix
                .get(&team)
                .cloned()
                .unwrap_or_default();
            if hypixel_prefix.trim().is_empty() {
                tokio::time::sleep(Duration::from_millis(500)).await;
                hypixel_prefix = s
                    .state()
                    .tab
                    .team_prefix
                    .get(&team)
                    .cloned()
                    .unwrap_or_default();
            }
            let (code, color) = bed_color(&hypixel_prefix);
            let final_prefix = take_chars(&format!("{prefix}{code}"), 16).to_string();
            let packet = {
                let mut st = s.state();
                let base = st.tab.team_suffix.get(&team).cloned().unwrap_or_default();
                st.tab.our_prefix.insert(team.clone(), final_prefix.clone());
                st.tab.our_suffix.insert(team.clone(), suffix.clone());
                Teams {
                    team: team.clone(),
                    mode: 2,
                    info: Some(TeamInfo {
                        display_name: team.clone(),
                        prefix: final_prefix,
                        suffix: take_chars(&(base + &suffix), 16).to_string(),
                        friendly_fire: 0,
                        name_tag_visibility: "always".into(),
                        color,
                    }),
                    players: None,
                }
            };
            debug(format!("Tagged {name} in team {team}"));
            s.send_to_client(packet.to_payload());
        }
        None => {
            let team = {
                let mut st = s.state();
                let team = format!("jp{}", st.tab.team_counter);
                st.tab.team_counter += 1;
                team
            };
            let final_prefix = take_chars(&format!("{prefix}{}", DEFAULT_COLOR.0), 16).to_string();
            {
                let mut st = s.state();
                st.tab.our_prefix.insert(team.clone(), final_prefix.clone());
                st.tab.our_suffix.insert(team.clone(), suffix.clone());
            }
            let create = Teams {
                team: team.clone(),
                mode: 0,
                info: Some(TeamInfo {
                    display_name: team.clone(),
                    prefix: final_prefix,
                    suffix: take_chars(&suffix, 16).to_string(),
                    friendly_fire: 0,
                    name_tag_visibility: "always".into(),
                    color: DEFAULT_COLOR.1,
                }),
                players: Some(Vec::new()),
            };
            s.send_to_client(create.to_payload());
            s.send_to_client(
                Teams {
                    team,
                    mode: 3,
                    info: None,
                    players: Some(vec![name.to_string()]),
                }
                .to_payload(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bed_colors_from_team_prefixes() {
        assert_eq!(bed_color("§cR §c"), ("§c", 12));
        assert_eq!(bed_color("§9§lB "), ("§9", 9));
        assert_eq!(bed_color(""), DEFAULT_COLOR);
        assert_eq!(bed_color("§7"), DEFAULT_COLOR);
    }

    #[test]
    fn hypixel_updates_keep_our_tags() {
        let mut st = State::default();
        st.tab.our_prefix.insert("Red".into(), "§e[123✫] §c".into());
        st.tab.our_suffix.insert("Red".into(), " §8|§f1.50".into());
        let mut packet = ServerPacket::Teams(Teams {
            team: "Red".into(),
            mode: 2,
            info: Some(TeamInfo {
                display_name: "Red".into(),
                prefix: "§cR ".into(),
                suffix: "".into(),
                friendly_fire: 0,
                name_tag_visibility: "always".into(),
                color: 12,
            }),
            players: None,
        });
        assert!(handle(&mut st, &mut packet));
        let ServerPacket::Teams(t) = packet else {
            unreachable!()
        };
        let info = t.info.unwrap();
        assert_eq!(
            (info.prefix.as_str(), info.suffix.as_str(), info.color),
            ("§e[123✫] §c", " §8|§f1.50", 12)
        );
    }
}
