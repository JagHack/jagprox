//! Port of `tabAlerter.js`: a chat notice and a sound when someone on the alert list
//! shows up in the tab list.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use mcproto::packets::Packet;
use mcproto::packets::clientbound::{PlayerListAction, SoundEffect};
use serde_json::Value;

use super::{ServerPacket, State};
use crate::formatter::{extract_text, strip_colors};
use crate::log::{debug, log};
use crate::proxy::session::Session;

#[derive(Default)]
pub struct TabAlerter {
    /// uuid → shown name (display name text if there is one)
    lobby: HashMap<u128, String>,
    alerted: HashSet<(String, u128)>,
    pub position: (f64, f64, f64),
}

impl TabAlerter {
    pub fn reset(&mut self) {
        debug("Tab Alerter reset.");
        self.lobby.clear();
        self.alerted.clear();
    }
}

/// Display names arrive as JSON chat components; fall back to the raw string.
fn display_text(display: &str) -> String {
    match serde_json::from_str::<Value>(display) {
        Ok(v) => extract_text(&v),
        Err(_) => display.to_string(),
    }
}

pub fn handle(st: &mut State, s: &Session, packet: &ServerPacket) {
    let alerter = &mut st.alerter;
    match packet {
        ServerPacket::Kick(_) => alerter.reset(),
        ServerPacket::Position(p) => alerter.position = (p.x, p.y, p.z),
        ServerPacket::PlayerList(list) => match &list.action {
            PlayerListAction::Add(players) => {
                for p in players {
                    let shown = p
                        .display_name
                        .as_deref()
                        .map_or_else(|| p.name.clone(), display_text);
                    alerter.lobby.insert(p.uuid, shown);
                    check(alerter, s, p.uuid);
                }
            }
            PlayerListAction::UpdateDisplayName(players) => {
                for (uuid, display) in players {
                    if let Some(display) = display
                        && alerter.lobby.contains_key(uuid)
                    {
                        alerter.lobby.insert(*uuid, display_text(display));
                        check(alerter, s, *uuid);
                    }
                }
            }
            PlayerListAction::Remove(uuids) => {
                for uuid in uuids {
                    alerter.lobby.remove(uuid);
                    alerter.alerted.retain(|(_, u)| u != uuid);
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn check(alerter: &mut TabAlerter, s: &Session, uuid: u128) {
    let Some(shown) = alerter.lobby.get(&uuid).filter(|n| !n.is_empty()) else {
        return;
    };
    let targets: Vec<String> = s
        .proxy()
        .config()
        .get("tab_alerts")
        .and_then(|v| v.as_array().cloned())
        .map(|a| {
            a.iter()
                .map(|t| t.as_str().map_or_else(|| t.to_string(), str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if targets.is_empty() {
        return;
    }
    let clean = strip_colors(shown);
    let key = (clean.clone(), uuid);
    if alerter.alerted.contains(&key) {
        return;
    }
    let lower = clean.to_lowercase();
    if targets.iter().any(|t| lower.contains(&t.to_lowercase())) {
        log(format!("Alerting for player: {clean}"));
        s.chat(&format!("§dFound player §5{clean}§d!"));
        alerter.alerted.insert(key);
        let (x, y, z) = alerter.position;
        // 1.8 sound names; coordinates are fixed-point (×8), pitch 63 = normal.
        let sound = SoundEffect {
            sound_name: "random.orb".into(),
            x: (x * 8.0).round() as i32,
            y: (y * 8.0).round() as i32,
            z: (z * 8.0).round() as i32,
            volume: 1.0,
            pitch: 63,
        }
        .to_payload();
        let session = s.clone();
        s.spawn(async move {
            for i in 0..4 {
                if i > 0 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                session.send_to_client(sound.clone());
            }
        });
    }
}
