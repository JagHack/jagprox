//! Every packet type is checked against bytes produced by node-minecraft-protocol
//! (`node scripts/gen-packet-fixtures.js`): decoding must succeed, read the right
//! values, and re-encoding must give back the exact same bytes.

use mcproto::packets::clientbound::{self as cb, PlayerListAction, Title};
use mcproto::packets::{Packet, handshake, login, parse_uuid, serverbound as sb, status};
use serde_json::Value;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// Decodes, re-encodes and compares; returns the decoded packet for field checks.
fn round_trip<P: Packet + std::fmt::Debug>(name: &str, bytes: &[u8]) -> P {
    let packet = P::from_payload(bytes).unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
    assert_eq!(
        packet.to_payload(),
        bytes,
        "{name}: re-encoded bytes differ ({packet:?})"
    );
    packet
}

#[test]
fn packets_match_node_minecraft_protocol() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!("fixtures/packets.json")).unwrap();
    let uuid_a = parse_uuid("069a79f4-44e9-4726-a5be-fca90e38aaf5").unwrap();
    let uuid_b = parse_uuid("853c80ef-3c37-49fd-aa49-938b674adae6").unwrap();
    let mut checked = 0;

    for fixture in &fixtures {
        let state = fixture["state"].as_str().unwrap();
        let clientbound = fixture["clientbound"].as_bool().unwrap();
        let name = fixture["name"].as_str().unwrap();
        let bytes = hex(fixture["hex"].as_str().unwrap());
        let label = format!(
            "{state}/{}/{name}",
            if clientbound {
                "clientbound"
            } else {
                "serverbound"
            }
        );

        match (state, clientbound, name) {
            ("handshaking", false, "set_protocol") => {
                let p: handshake::Handshake = round_trip(&label, &bytes);
                assert_eq!(
                    (
                        p.protocol_version,
                        p.server_address.as_str(),
                        p.server_port,
                        p.next_state
                    ),
                    (47, "mc.hypixel.net", 25565, 2)
                );
            }
            ("status", false, "ping_start") => {
                round_trip::<status::Request>(&label, &bytes);
            }
            ("status", false, "ping") => {
                let p: status::Ping = round_trip(&label, &bytes);
                assert_eq!(p.payload, 0x0102030405060708);
            }
            ("status", true, "server_info") => {
                let p: status::Response = round_trip(&label, &bytes);
                assert!(p.json.contains("\"protocol\":47"));
            }
            ("status", true, "ping") => {
                let p: status::Pong = round_trip(&label, &bytes);
                assert_eq!(p.payload, 42);
            }
            ("login", false, "login_start") => {
                let p: login::LoginStart = round_trip(&label, &bytes);
                assert_eq!(p.username, "JagHack");
            }
            ("login", false, "encryption_begin") => {
                let p: login::EncryptionResponse = round_trip(&label, &bytes);
                assert_eq!((p.shared_secret.len(), p.verify_token[0]), (128, 9));
            }
            ("login", true, "disconnect") => {
                round_trip::<login::Disconnect>(&label, &bytes);
            }
            ("login", true, "encryption_begin") => {
                let p: login::EncryptionRequest = round_trip(&label, &bytes);
                assert_eq!(
                    (
                        p.server_id.as_str(),
                        p.public_key.len(),
                        p.verify_token.as_slice()
                    ),
                    ("", 162, &[1, 2, 3, 4][..])
                );
            }
            ("login", true, "success") => {
                let p: login::LoginSuccess = round_trip(&label, &bytes);
                assert_eq!(
                    (p.uuid.as_str(), p.username.as_str()),
                    ("069a79f4-44e9-4726-a5be-fca90e38aaf5", "Notch")
                );
            }
            ("login", true, "compress") => {
                let p: login::SetCompression = round_trip(&label, &bytes);
                assert_eq!(p.threshold, 256);
            }
            ("play", true, "login") => {
                let p: cb::JoinGame = round_trip(&label, &bytes);
                assert_eq!(
                    (
                        p.entity_id,
                        p.dimension,
                        p.max_players,
                        p.level_type.as_str()
                    ),
                    (1234, -1, 100, "default")
                );
            }
            ("play", true, "chat") => {
                let p: cb::Chat = round_trip(&label, &bytes);
                assert_eq!(p.position, 1);
                assert!(p.message.contains("world ✫"));
            }
            ("play", true, "respawn") => {
                let p: cb::Respawn = round_trip(&label, &bytes);
                assert_eq!((p.dimension, p.level_type.as_str()), (1, "flat"));
            }
            ("play", true, "position") => {
                let p: cb::PlayerPositionAndLook = round_trip(&label, &bytes);
                assert_eq!(
                    (p.x, p.y, p.z, p.yaw, p.pitch),
                    (1.5, 64.0, -1000.25, 90.0, -12.5)
                );
            }
            ("play", true, "named_entity_spawn") => {
                let p = cb::SpawnPlayer::decode_payload(&bytes).unwrap();
                assert_eq!((p.entity_id, p.uuid), (4242, uuid_b));
            }
            ("play", true, "entity_destroy") => {
                let p: cb::DestroyEntities = round_trip(&label, &bytes);
                assert_eq!(p.entity_ids, vec![1, 300, 70000]);
            }
            ("play", true, "named_sound_effect") => {
                let p: cb::SoundEffect = round_trip(&label, &bytes);
                assert_eq!(
                    (p.sound_name.as_str(), p.x, p.y, p.z, p.pitch),
                    ("random.orb", 80, 512, -24, 63)
                );
            }
            ("play", true, "player_info") => {
                let p: cb::PlayerListItem = round_trip(&label, &bytes);
                match p.action {
                    PlayerListAction::Add(players) => {
                        assert_eq!(players.len(), 2);
                        assert_eq!(
                            (players[0].uuid, players[0].name.as_str(), players[0].ping),
                            (uuid_a, "Notch", 35)
                        );
                        assert_eq!(players[0].properties[0].signature.as_deref(), Some("c2ln"));
                        assert!(players[0].display_name.is_some());
                        assert_eq!(
                            (
                                players[1].properties[0].signature.as_deref(),
                                players[1].display_name.as_deref()
                            ),
                            (None, None)
                        );
                    }
                    PlayerListAction::UpdateGameMode(players) => {
                        assert_eq!(players, vec![(uuid_a, 3)])
                    }
                    PlayerListAction::UpdateLatency(players) => {
                        assert_eq!(players, vec![(uuid_a, 500), (uuid_b, 1)])
                    }
                    PlayerListAction::UpdateDisplayName(players) => {
                        assert!(players[0].1.is_some());
                        assert_eq!(players[1], (uuid_b, None));
                    }
                    PlayerListAction::Remove(uuids) => assert_eq!(uuids, vec![uuid_a, uuid_b]),
                }
            }
            ("play", true, "scoreboard_objective") => {
                let p: cb::ScoreboardObjective = round_trip(&label, &bytes);
                if p.mode == 0 {
                    assert_eq!(p.display, Some(("§e§lBED WARS".into(), "integer".into())));
                }
            }
            ("play", true, "scoreboard_team") => {
                let p: cb::Teams = round_trip(&label, &bytes);
                match p.mode {
                    0 => {
                        let info = p.info.unwrap();
                        assert_eq!(
                            (info.prefix.as_str(), info.suffix.as_str(), info.color),
                            ("§7[12✫] ", " §8|§f1.50", 15)
                        );
                        assert_eq!(p.players.unwrap(), vec!["Notch", "jeb_"]);
                    }
                    2 => assert_eq!(p.info.unwrap().name_tag_visibility, "hideForOtherTeams"),
                    3 => assert_eq!(p.players.unwrap(), vec!["Dinnerbone"]),
                    _ => {}
                }
            }
            ("play", true, "custom_payload") => {
                let p: cb::PluginMessage = round_trip(&label, &bytes);
                assert_eq!(p.channel, "MC|Brand");
            }
            ("play", true, "kick_disconnect") => {
                round_trip::<cb::Disconnect>(&label, &bytes);
            }
            ("play", true, "title") => {
                let p: Title = round_trip(&label, &bytes);
                if let Title::Times {
                    fade_in,
                    stay,
                    fade_out,
                } = p
                {
                    assert_eq!((fade_in, stay, fade_out), (10, 70, 20));
                }
            }
            ("play", true, "playerlist_header") => {
                let p: cb::PlayerListHeaderFooter = round_trip(&label, &bytes);
                assert!(p.footer.contains("store.hypixel.net"));
            }
            ("play", false, "chat") => {
                let p: sb::Chat = round_trip(&label, &bytes);
                assert_eq!(p.message, "/sc bw Notch");
            }
            ("play", false, "custom_payload") => {
                let p: sb::PluginMessage = round_trip(&label, &bytes);
                assert_eq!(
                    (p.channel.as_str(), p.data.as_slice()),
                    ("MC|Brand", &b"\x05lunar"[..])
                );
            }
            other => panic!("no check for fixture {other:?}"),
        }
        checked += 1;
    }
    assert_eq!(checked, 40);
}
