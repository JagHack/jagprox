//! Turning the broadcaster's live packet stream into what a spectator should see.
//!
//! A spectator is a normal Minecraft client in adventure mode on a world the host
//! rebuilds for them. Most of Hypixel's packets are forwarded untouched (chunks,
//! blocks, other entities, chat, scoreboard, titles, sounds …). Packets that are
//! about the broadcaster *as a player* — their health, inventory, held item,
//! position corrections, abilities — would corrupt the spectator's own client, so
//! they are dropped. The broadcaster is instead shown as a separate player entity
//! that the host animates from the broadcaster's own outgoing movement.

use mcproto::buf::Write as _;
use mcproto::packets::{Packet, clientbound as cb};
use mcproto::world;

/// Clientbound ids that must NOT reach a spectator (they describe the broadcaster
/// as a player, or open UI the spectator can't have).
fn is_dropped(id: i32) -> bool {
    matches!(
        id,
        cb::JoinGame::ID            // 0x01: spectator has its own
        | 0x06                      // Update Health
        | 0x08                      // Player Position And Look (would yank the spectator)
        | 0x09                      // Held Item Change
        | 0x1F                      // Set Experience
        | 0x2D                      // Open Window
        | 0x2E                      // Close Window
        | 0x2F                      // Set Slot
        | 0x30                      // Window Items
        | 0x31                      // Window Property
        | 0x32                      // Confirm Transaction
        | 0x37                      // Statistics
        | 0x39                      // Player Abilities (host controls the spectator's)
        | 0x3A                      // Tab-Complete
        | 0x48 // Resource Pack Send
    )
}

/// What a spectator should receive for one clientbound payload from Hypixel: the
/// (possibly unchanged) payload, or `None` to drop it.
pub fn for_spectator(payload: &[u8]) -> Option<Vec<u8>> {
    let mut r: &[u8] = payload;
    let id = mcproto::buf::Read::varint(&mut r).ok()?;
    if is_dropped(id) {
        return None;
    }
    match id {
        // Keep the spectator in adventure mode: the broadcaster's own gamemode would
        // let them break blocks client-side (flight is re-sent after the teleport).
        cb::Respawn::ID => {
            if let Ok(mut rp) = cb::Respawn::from_payload(payload) {
                rp.game_mode = 2;
                return Some(rp.to_payload());
            }
        }
        cb::Chat::ID if is_private_chat(payload) => return None,
        _ => {}
    }
    Some(payload.to_vec())
}

/// Whispers and party/guild chat are the broadcaster's private conversations.
fn is_private_chat(payload: &[u8]) -> bool {
    let Ok(chat) = cb::Chat::from_payload(payload) else {
        return false;
    };
    let text = match serde_json::from_str::<serde_json::Value>(&chat.message) {
        Ok(v) => crate::formatter::extract_text(&v),
        Err(_) => chat.message,
    };
    let text = crate::formatter::strip_colors(&text);
    let text = text.trim_start();
    ["From ", "To ", "Party >", "Guild >", "Officer >"]
        .iter()
        .any(|p| text.starts_with(p))
}

/// A whole-degree angle as the protocol's single-byte 0–256 unit.
fn angle_byte(degrees: f32) -> i8 {
    (degrees / 360.0 * 256.0).round() as i32 as u8 as i8
}

fn fixed(coord: f64) -> i32 {
    (coord * 32.0).round() as i32
}

/// Spawns the broadcaster as a player entity (`eid`), using their current position.
/// The skin comes from the broadcaster's tab-list entry, which the spectator already
/// has from the snapshot, so only a Spawn Player packet is needed.
pub fn spawn_broadcaster(
    eid: i32,
    uuid: u128,
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.varint(world::id::SPAWN_PLAYER);
    out.varint(eid);
    out.uuid(uuid);
    out.i32(fixed(x));
    out.i32(fixed(y));
    out.i32(fixed(z));
    out.i8(angle_byte(yaw));
    out.i8(angle_byte(pitch));
    out.i16(0); // current item
    out.u8(0x7f); // empty metadata
    out
}

/// Moves the broadcaster's model to an absolute position (robust: sent every update
/// instead of relative deltas, so it can't drift).
pub fn move_broadcaster(eid: i32, x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> Vec<Vec<u8>> {
    let tp = world::EntityTeleport {
        eid,
        x: fixed(x),
        y: fixed(y),
        z: fixed(z),
        yaw: angle_byte(yaw),
        pitch: angle_byte(pitch),
        on_ground: true,
    }
    .to_payload();
    let head = world::EntityHeadLook {
        eid,
        head_yaw: angle_byte(yaw),
    }
    .to_payload();
    vec![tp, head]
}

/// The broadcaster swung their arm.
pub fn swing_broadcaster(eid: i32) -> Vec<u8> {
    world::Animation { eid, animation: 0 }.to_payload()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_self_and_inventory_packets_but_keeps_chunks() {
        // Update Health (0x06) is dropped.
        assert!(for_spectator(&[0x06, 0, 0, 0]).is_none());
        // Set Slot (0x2F) dropped.
        assert!(for_spectator(&[0x2F, 0, 0]).is_none());
        // A chat packet (0x02) is forwarded unchanged.
        let chat = cb::Chat {
            message: "{}".into(),
            position: 0,
        }
        .to_payload();
        assert_eq!(for_spectator(&chat).as_deref(), Some(chat.as_slice()));
    }

    #[test]
    fn private_chat_is_dropped() {
        let whisper = cb::Chat {
            message:
                r#"{"text":"","extra":[{"text":"§dFrom §b[MVP§c+§b] Bob§7: "},{"text":"hi"}]}"#
                    .into(),
            position: 0,
        }
        .to_payload();
        assert!(for_spectator(&whisper).is_none());
        let party = cb::Chat {
            message: r#"{"text":"§9Party §8> §bBob§f: gg"}"#.into(),
            position: 0,
        }
        .to_payload();
        assert!(for_spectator(&party).is_none());
    }

    #[test]
    fn respawn_keeps_spectator_in_adventure() {
        let rp = cb::Respawn {
            dimension: -1,
            difficulty: 2,
            game_mode: 0,
            level_type: "default".into(),
        }
        .to_payload();
        let out = cb::Respawn::from_payload(&for_spectator(&rp).unwrap()).unwrap();
        assert_eq!((out.dimension, out.game_mode), (-1, 2));
    }

    #[test]
    fn angles_wrap_into_one_byte() {
        assert_eq!(angle_byte(0.0), 0);
        assert_eq!(angle_byte(90.0), 64);
        assert_eq!(angle_byte(180.0), -128);
        assert_eq!(angle_byte(360.0), 0);
    }

    #[test]
    fn broadcaster_spawn_is_a_valid_spawn_player() {
        let p = spawn_broadcaster(55, 0x1234, 10.0, 64.0, -20.0, 90.0, 0.0);
        let (eid, uuid, pos) = world::spawn_player_pos(&p).unwrap();
        assert_eq!(eid, 55);
        assert_eq!(uuid, 0x1234);
        assert_eq!(pos.x, 32 * 10);
        assert_eq!(pos.yaw, 64);
    }
}
