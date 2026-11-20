//! The packets JagProx reads or writes, protocol 47 (1.8.x). Names in comments are
//! the ones node-minecraft-protocol uses, which the original JS code refers to.

use crate::buf::{Read, Write};
use crate::error::{Error, Result};

/// A packet that can be decoded from and encoded to its data (after the id).
pub trait Packet: Sized {
    const ID: i32;
    fn decode(r: &mut &[u8]) -> Result<Self>;
    fn encode(&self, w: &mut Vec<u8>);

    /// id + data, ready for [`crate::frame::FrameWriter::queue_packet`].
    fn to_payload(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.varint(Self::ID);
        self.encode(&mut out);
        out
    }

    /// Decodes a full payload (id + data), checking the id and that nothing is left.
    fn from_payload(payload: &[u8]) -> Result<Self> {
        let mut r = payload;
        let id = r.varint()?;
        if id != Self::ID {
            return Err(Error::Malformed(format!(
                "expected packet {:#04x}, got {id:#04x}",
                Self::ID
            )));
        }
        let packet = Self::decode(&mut r)?;
        if !r.is_empty() {
            return Err(Error::Malformed(format!(
                "{} trailing bytes in packet {id:#04x}",
                r.len()
            )));
        }
        Ok(packet)
    }
}

macro_rules! packet {
    ($name:ident = $id:expr, { $($field:ident : $ty:ty => $read:ident / $write:ident),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }

        impl Packet for $name {
            const ID: i32 = $id;
            #[allow(unused_variables)]
            fn decode(r: &mut &[u8]) -> Result<Self> {
                Ok(Self { $($field: r.$read()?),* })
            }
            #[allow(unused_variables)]
            fn encode(&self, w: &mut Vec<u8>) {
                $(w.$write(field_ref!(self.$field, $write));)*
            }
        }
    };
}

/// Strings and byte arrays are written by reference, numbers by value.
macro_rules! field_ref {
    ($e:expr, string) => {
        &$e
    };
    ($e:expr, byte_array) => {
        &$e
    };
    ($e:expr, $other:ident) => {
        $e
    };
}

// ---------------------------------------------------------------------------
// Handshaking, status, login
// ---------------------------------------------------------------------------

pub mod handshake {
    use super::*;

    pub const STATUS: i32 = 1;
    pub const LOGIN: i32 = 2;

    // set_protocol
    packet!(Handshake = 0x00, {
        protocol_version: i32 => varint / varint,
        server_address: String => string / string,
        server_port: u16 => u16 / u16,
        next_state: i32 => varint / varint,
    });
}

pub mod status {
    use super::*;

    // ping_start
    packet!(Request = 0x00, {});
    // ping
    packet!(Ping = 0x01, { payload: i64 => i64 / i64 });
    // server_info
    packet!(Response = 0x00, { json: String => string / string });
    // ping (clientbound)
    packet!(Pong = 0x01, { payload: i64 => i64 / i64 });
}

pub mod login {
    use super::*;

    // login_start
    packet!(LoginStart = 0x00, { username: String => string / string });
    // encryption_begin (serverbound)
    packet!(EncryptionResponse = 0x01, {
        shared_secret: Vec<u8> => byte_array / byte_array,
        verify_token: Vec<u8> => byte_array / byte_array,
    });
    // disconnect
    packet!(Disconnect = 0x00, { reason: String => string / string });
    // encryption_begin (clientbound)
    packet!(EncryptionRequest = 0x01, {
        server_id: String => string / string,
        public_key: Vec<u8> => byte_array / byte_array,
        verify_token: Vec<u8> => byte_array / byte_array,
    });
    // success
    packet!(LoginSuccess = 0x02, {
        uuid: String => string / string,
        username: String => string / string,
    });
    // compress
    packet!(SetCompression = 0x03, { threshold: i32 => varint / varint });
}

// ---------------------------------------------------------------------------
// Play, server → client
// ---------------------------------------------------------------------------

pub mod clientbound {
    use super::*;

    pub const KEEP_ALIVE: i32 = 0x00;

    // login
    packet!(JoinGame = 0x01, {
        entity_id: i32 => i32 / i32,
        game_mode: u8 => u8 / u8,
        dimension: i8 => i8 / i8,
        difficulty: u8 => u8 / u8,
        max_players: u8 => u8 / u8,
        level_type: String => string / string,
        reduced_debug_info: bool => bool / bool,
    });

    // chat
    packet!(Chat = 0x02, {
        message: String => string / string,
        position: i8 => i8 / i8,
    });

    // respawn
    packet!(Respawn = 0x07, {
        dimension: i32 => i32 / i32,
        difficulty: u8 => u8 / u8,
        game_mode: u8 => u8 / u8,
        level_type: String => string / string,
    });

    // position
    packet!(PlayerPositionAndLook = 0x08, {
        x: f64 => f64 / f64,
        y: f64 => f64 / f64,
        z: f64 => f64 / f64,
        yaw: f32 => f32 / f32,
        pitch: f32 => f32 / f32,
        flags: i8 => i8 / i8,
    });

    /// `named_entity_spawn`: only the fields JagProx reads; never re-encoded (the
    /// original frame is forwarded).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SpawnPlayer {
        pub entity_id: i32,
        pub uuid: u128,
    }

    impl SpawnPlayer {
        pub const ID: i32 = 0x0C;

        pub fn decode_payload(payload: &[u8]) -> Result<Self> {
            let mut r = payload;
            if r.varint()? != Self::ID {
                return Err(Error::Malformed("not a spawn player packet".into()));
            }
            Ok(Self {
                entity_id: r.varint()?,
                uuid: r.uuid()?,
            })
        }
    }

    // entity_destroy
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DestroyEntities {
        pub entity_ids: Vec<i32>,
    }

    impl Packet for DestroyEntities {
        const ID: i32 = 0x13;
        fn decode(r: &mut &[u8]) -> Result<Self> {
            let count = r.length()?;
            let mut entity_ids = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                entity_ids.push(r.varint()?);
            }
            Ok(Self { entity_ids })
        }
        fn encode(&self, w: &mut Vec<u8>) {
            w.length(self.entity_ids.len());
            for id in &self.entity_ids {
                w.varint(*id);
            }
        }
    }

    // named_sound_effect: position is block coordinates × 8; pitch 63 = normal.
    packet!(SoundEffect = 0x29, {
        sound_name: String => string / string,
        x: i32 => i32 / i32,
        y: i32 => i32 / i32,
        z: i32 => i32 / i32,
        volume: f32 => f32 / f32,
        pitch: u8 => u8 / u8,
    });

    /// One property of a player's profile (skin textures).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Property {
        pub name: String,
        pub value: String,
        pub signature: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PlayerAdd {
        pub uuid: u128,
        pub name: String,
        pub properties: Vec<Property>,
        pub game_mode: i32,
        pub ping: i32,
        /// JSON chat component.
        pub display_name: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum PlayerListAction {
        /// `add_player`
        Add(Vec<PlayerAdd>),
        /// `update_game_mode`
        UpdateGameMode(Vec<(u128, i32)>),
        /// `update_latency`
        UpdateLatency(Vec<(u128, i32)>),
        /// `update_display_name`
        UpdateDisplayName(Vec<(u128, Option<String>)>),
        /// `remove_player`
        Remove(Vec<u128>),
    }

    // player_info
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PlayerListItem {
        pub action: PlayerListAction,
    }

    fn optional_string(r: &mut &[u8]) -> Result<Option<String>> {
        if r.bool()? {
            Ok(Some(r.string()?))
        } else {
            Ok(None)
        }
    }

    fn write_optional_string(w: &mut Vec<u8>, s: &Option<String>) {
        w.bool(s.is_some());
        if let Some(s) = s {
            w.string(s);
        }
    }

    fn list<T>(r: &mut &[u8], mut item: impl FnMut(&mut &[u8]) -> Result<T>) -> Result<Vec<T>> {
        let count = r.length()?;
        let mut out = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            out.push(item(r)?);
        }
        Ok(out)
    }

    impl Packet for PlayerListItem {
        const ID: i32 = 0x38;

        fn decode(r: &mut &[u8]) -> Result<Self> {
            let action = match r.varint()? {
                0 => PlayerListAction::Add(list(r, |r| {
                    Ok(PlayerAdd {
                        uuid: r.uuid()?,
                        name: r.string()?,
                        properties: list(r, |r| {
                            Ok(Property {
                                name: r.string()?,
                                value: r.string()?,
                                signature: optional_string(r)?,
                            })
                        })?,
                        game_mode: r.varint()?,
                        ping: r.varint()?,
                        display_name: optional_string(r)?,
                    })
                })?),
                1 => PlayerListAction::UpdateGameMode(list(r, |r| Ok((r.uuid()?, r.varint()?)))?),
                2 => PlayerListAction::UpdateLatency(list(r, |r| Ok((r.uuid()?, r.varint()?)))?),
                3 => PlayerListAction::UpdateDisplayName(list(r, |r| {
                    Ok((r.uuid()?, optional_string(r)?))
                })?),
                4 => PlayerListAction::Remove(list(r, |r| r.uuid())?),
                other => {
                    return Err(Error::Malformed(format!(
                        "unknown player list action {other}"
                    )));
                }
            };
            Ok(Self { action })
        }

        fn encode(&self, w: &mut Vec<u8>) {
            match &self.action {
                PlayerListAction::Add(players) => {
                    w.varint(0);
                    w.length(players.len());
                    for p in players {
                        w.uuid(p.uuid);
                        w.string(&p.name);
                        w.length(p.properties.len());
                        for prop in &p.properties {
                            w.string(&prop.name);
                            w.string(&prop.value);
                            write_optional_string(w, &prop.signature);
                        }
                        w.varint(p.game_mode);
                        w.varint(p.ping);
                        write_optional_string(w, &p.display_name);
                    }
                }
                PlayerListAction::UpdateGameMode(players)
                | PlayerListAction::UpdateLatency(players) => {
                    w.varint(
                        if matches!(self.action, PlayerListAction::UpdateGameMode(_)) {
                            1
                        } else {
                            2
                        },
                    );
                    w.length(players.len());
                    for (uuid, value) in players {
                        w.uuid(*uuid);
                        w.varint(*value);
                    }
                }
                PlayerListAction::UpdateDisplayName(players) => {
                    w.varint(3);
                    w.length(players.len());
                    for (uuid, name) in players {
                        w.uuid(*uuid);
                        write_optional_string(w, name);
                    }
                }
                PlayerListAction::Remove(uuids) => {
                    w.varint(4);
                    w.length(uuids.len());
                    for uuid in uuids {
                        w.uuid(*uuid);
                    }
                }
            }
        }
    }

    // scoreboard_objective
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ScoreboardObjective {
        pub name: String,
        /// 0 create, 1 remove, 2 update.
        pub mode: i8,
        /// `(display text, type)` for modes 0 and 2.
        pub display: Option<(String, String)>,
    }

    impl Packet for ScoreboardObjective {
        const ID: i32 = 0x3B;
        fn decode(r: &mut &[u8]) -> Result<Self> {
            let name = r.string()?;
            let mode = r.i8()?;
            let display = if mode == 0 || mode == 2 {
                Some((r.string()?, r.string()?))
            } else {
                None
            };
            Ok(Self {
                name,
                mode,
                display,
            })
        }
        fn encode(&self, w: &mut Vec<u8>) {
            w.string(&self.name);
            w.i8(self.mode);
            if let Some((value, kind)) = &self.display {
                w.string(value);
                w.string(kind);
            }
        }
    }

    /// Team appearance, sent with modes 0 (create) and 2 (update).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TeamInfo {
        pub display_name: String,
        pub prefix: String,
        pub suffix: String,
        pub friendly_fire: i8,
        pub name_tag_visibility: String,
        pub color: i8,
    }

    // scoreboard_team
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Teams {
        pub team: String,
        /// 0 create, 1 remove, 2 update info, 3 add players, 4 remove players.
        pub mode: i8,
        pub info: Option<TeamInfo>,
        /// Players for modes 0, 3 and 4.
        pub players: Option<Vec<String>>,
    }

    impl Packet for Teams {
        const ID: i32 = 0x3E;
        fn decode(r: &mut &[u8]) -> Result<Self> {
            let team = r.string()?;
            let mode = r.i8()?;
            let info = if mode == 0 || mode == 2 {
                Some(TeamInfo {
                    display_name: r.string()?,
                    prefix: r.string()?,
                    suffix: r.string()?,
                    friendly_fire: r.i8()?,
                    name_tag_visibility: r.string()?,
                    color: r.i8()?,
                })
            } else {
                None
            };
            let players = if matches!(mode, 0 | 3 | 4) {
                Some(list(r, |r| r.string())?)
            } else {
                None
            };
            Ok(Self {
                team,
                mode,
                info,
                players,
            })
        }
        fn encode(&self, w: &mut Vec<u8>) {
            w.string(&self.team);
            w.i8(self.mode);
            if let Some(info) = &self.info {
                w.string(&info.display_name);
                w.string(&info.prefix);
                w.string(&info.suffix);
                w.i8(info.friendly_fire);
                w.string(&info.name_tag_visibility);
                w.i8(info.color);
            }
            if let Some(players) = &self.players {
                w.length(players.len());
                for p in players {
                    w.string(p);
                }
            }
        }
    }

    /// `custom_payload`
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PluginMessage {
        pub channel: String,
        pub data: Vec<u8>,
    }

    impl Packet for PluginMessage {
        const ID: i32 = 0x3F;
        fn decode(r: &mut &[u8]) -> Result<Self> {
            Ok(Self {
                channel: r.string()?,
                data: r.rest().to_vec(),
            })
        }
        fn encode(&self, w: &mut Vec<u8>) {
            w.string(&self.channel);
            w.put(&self.data);
        }
    }

    // kick_disconnect
    packet!(Disconnect = 0x40, { reason: String => string / string });

    /// `title`
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Title {
        Title(String),
        Subtitle(String),
        Times {
            fade_in: i32,
            stay: i32,
            fade_out: i32,
        },
        Clear,
        Reset,
    }

    impl Packet for Title {
        const ID: i32 = 0x45;
        fn decode(r: &mut &[u8]) -> Result<Self> {
            Ok(match r.varint()? {
                0 => Title::Title(r.string()?),
                1 => Title::Subtitle(r.string()?),
                2 => Title::Times {
                    fade_in: r.i32()?,
                    stay: r.i32()?,
                    fade_out: r.i32()?,
                },
                3 => Title::Clear,
                4 => Title::Reset,
                other => return Err(Error::Malformed(format!("unknown title action {other}"))),
            })
        }
        fn encode(&self, w: &mut Vec<u8>) {
            match self {
                Title::Title(text) => {
                    w.varint(0);
                    w.string(text);
                }
                Title::Subtitle(text) => {
                    w.varint(1);
                    w.string(text);
                }
                Title::Times {
                    fade_in,
                    stay,
                    fade_out,
                } => {
                    w.varint(2);
                    w.i32(*fade_in);
                    w.i32(*stay);
                    w.i32(*fade_out);
                }
                Title::Clear => w.varint(3),
                Title::Reset => w.varint(4),
            }
        }
    }

    // playerlist_header
    packet!(PlayerListHeaderFooter = 0x47, {
        header: String => string / string,
        footer: String => string / string,
    });

    // scoreboard_score
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UpdateScore {
        /// The score's entry name (a player or a line of text), ≤ 40 chars.
        pub name: String,
        /// 0 = create/update, 1 = remove.
        pub action: i8,
        pub objective: String,
        /// Present unless the action is "remove".
        pub value: Option<i32>,
    }

    impl Packet for UpdateScore {
        const ID: i32 = 0x3C;
        fn decode(r: &mut &[u8]) -> Result<Self> {
            let name = r.string()?;
            let action = r.i8()?;
            let objective = r.string()?;
            let value = (action != 1).then(|| r.varint()).transpose()?;
            Ok(Self {
                name,
                action,
                objective,
                value,
            })
        }
        fn encode(&self, w: &mut Vec<u8>) {
            w.string(&self.name);
            w.i8(self.action);
            w.string(&self.objective);
            if self.action != 1
                && let Some(value) = self.value
            {
                w.varint(value);
            }
        }
    }

    // scoreboard_display_objective
    packet!(DisplayScoreboard = 0x3D, {
        position: i8 => i8 / i8,
        name: String => string / string,
    });

    /// `custom_payload` id in this direction.
    pub const PLUGIN_MESSAGE: i32 = PluginMessage::ID;
}

// ---------------------------------------------------------------------------
// Play, client → server
// ---------------------------------------------------------------------------

pub mod serverbound {
    use super::*;

    pub const KEEP_ALIVE: i32 = 0x00;

    // chat
    packet!(Chat = 0x01, { message: String => string / string });

    /// `custom_payload`
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PluginMessage {
        pub channel: String,
        pub data: Vec<u8>,
    }

    impl Packet for PluginMessage {
        const ID: i32 = 0x17;
        fn decode(r: &mut &[u8]) -> Result<Self> {
            Ok(Self {
                channel: r.string()?,
                data: r.rest().to_vec(),
            })
        }
        fn encode(&self, w: &mut Vec<u8>) {
            w.string(&self.channel);
            w.put(&self.data);
        }
    }
}

/// `MC|Brand` payload saying "vanilla" (a VarInt-prefixed string).
pub fn vanilla_brand() -> Vec<u8> {
    let mut data = Vec::new();
    data.string("vanilla");
    data
}

/// Formats a 128-bit UUID with dashes.
pub fn uuid_string(uuid: u128) -> String {
    let h = format!("{uuid:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Parses a UUID with or without dashes.
pub fn parse_uuid(s: &str) -> Option<u128> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    u128::from_str_radix(&hex, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_formatting() {
        let uuid = parse_uuid("069a79f444e94726a5befca90e38aaf5").unwrap();
        assert_eq!(uuid_string(uuid), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
        assert_eq!(
            parse_uuid("069a79f4-44e9-4726-a5be-fca90e38aaf5"),
            Some(uuid)
        );
        assert_eq!(parse_uuid("nope"), None);
    }

    #[test]
    fn brand_is_varint_prefixed() {
        assert_eq!(vanilla_brand(), b"\x07vanilla");
    }

    #[test]
    fn from_payload_checks_id_and_trailing_bytes() {
        let mut payload = clientbound::Chat {
            message: "{}".into(),
            position: 0,
        }
        .to_payload();
        assert!(clientbound::Chat::from_payload(&payload).is_ok());
        assert!(clientbound::Respawn::from_payload(&payload).is_err());
        payload.push(0);
        assert!(clientbound::Chat::from_payload(&payload).is_err());
    }
}
