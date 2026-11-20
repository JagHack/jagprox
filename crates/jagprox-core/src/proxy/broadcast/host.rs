//! The broadcaster side of one spectator: acts as a Minecraft server over the relay
//! pipe, logs the spectator in (offline — the relay is already a trusted peer link),
//! sends them a snapshot of the world and the broadcaster's model, then streams live
//! packets to them and keeps the connection alive.

use std::sync::Arc;
use std::time::Duration;

use mcproto::buf::Write as _;
use mcproto::conn::{self, Connection};
use mcproto::frame::{FrameReader, FrameWriter};
use mcproto::packets::{Packet, clientbound as cb, serverbound as sb, uuid_string};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use super::Broadcast;
use crate::log::log;

/// The entity id the spectator's own (invisible) player uses. Far above any id
/// Hypixel hands out, so it never collides with a tracked entity.
pub const SPECTATOR_EID: i32 = 0x4000_0000;

/// Compression threshold used on the relay link (chunks are large).
const COMPRESSION: i32 = 256;

/// A clientbound Keep Alive (0x00) with the given id.
fn keep_alive(id: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    out.varint(cb::KEEP_ALIVE);
    out.varint(id);
    out
}

/// Player Abilities (0x39): invulnerable + allow-flying + flying, so the spectator
/// floats freely but can't interact.
pub fn abilities() -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.varint(0x39);
    out.u8(0x01 | 0x02 | 0x04); // invulnerable | flying | allow flying
    out.f32(0.1); // flying speed
    out.f32(0.1); // field-of-view modifier
    out
}

/// A clientbound Player Position And Look (0x08) placing the spectator absolutely.
pub fn position(x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> Vec<u8> {
    cb::PlayerPositionAndLook {
        x,
        y,
        z,
        yaw,
        pitch,
        flags: 0,
    }
    .to_payload()
}

/// Serves one spectator over `relay` until either side disconnects.
pub async fn run(relay: TcpStream, broadcast: Arc<Broadcast>) -> Result<(), String> {
    let (r, w) = relay.into_split();
    let mut conn = Connection::new(r, w);

    // Login over the relay (offline: no encryption, the link is already trusted).
    let hs = conn::read_handshake(&mut conn).await.map_err(|e| e.to_string())?;
    if hs.next_state != mcproto::packets::handshake::LOGIN {
        return Err("spectator did not request login".into());
    }
    let name = conn::read_login_start(&mut conn).await.map_err(|e| e.to_string())?;
    conn::server_set_compression(&mut conn, COMPRESSION).await.map_err(|e| e.to_string())?;
    let uuid = mcproto::crypto::offline_uuid(&name);
    conn::server_login_success(&mut conn, &uuid_string(uuid), &name)
        .await
        .map_err(|e| e.to_string())?;
    log(format!("Spectator {name} connected to broadcast."));

    let Connection {
        reader,
        mut writer,
    } = conn;

    // Enter play: JoinGame + abilities + the world snapshot + the broadcaster model,
    // captured atomically so no live packet is missed or duplicated.
    for p in broadcast.spectator_join_sequence() {
        writer.queue_packet(&p).map_err(|e| e.to_string())?;
    }
    let (snapshot, live) = broadcast.add_spectator();
    for p in &snapshot {
        writer.queue_packet(p).map_err(|e| e.to_string())?;
    }
    writer.flush().await.map_err(|e| e.to_string())?;

    // Two loops: push live packets + keep-alives to the spectator, and drain whatever
    // the spectator sends back (keep-alive echoes, their camera movement, commands).
    let writer_task = tokio::spawn(writer_loop(writer, live));
    let reader_task = tokio::spawn(reader_loop(reader, broadcast.clone(), name.clone()));

    tokio::select! {
        _ = writer_task => {}
        _ = reader_task => {}
    }
    log(format!("Spectator {name} left the broadcast."));
    Ok(())
}

async fn writer_loop(
    mut writer: FrameWriter<OwnedWriteHalf>,
    mut live: tokio::sync::mpsc::Receiver<Vec<u8>>,
) {
    let mut keepalive = tokio::time::interval(Duration::from_secs(10));
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut id: i32 = 1;
    loop {
        tokio::select! {
            msg = live.recv() => {
                let Some(first) = msg else { break };
                if writer.queue_packet(&first).is_err() { break; }
                // Drain whatever else is ready into the same flush.
                while let Ok(more) = live.try_recv() {
                    if writer.queue_packet(&more).is_err() { return; }
                }
                if writer.flush().await.is_err() { break; }
            }
            _ = keepalive.tick() => {
                if writer.send(&keep_alive(id)).await.is_err() { break; }
                id = id.wrapping_add(1);
            }
        }
    }
}

async fn reader_loop(mut reader: FrameReader<OwnedReadHalf>, broadcast: Arc<Broadcast>, name: String) {
    loop {
        let frame = match reader.read_frame().await {
            Ok(f) => f,
            Err(_) => break,
        };
        let Ok(id) = frame.id() else { continue };
        // The only serverbound packet we act on is chat, so a spectator can use
        // broadcast chat / spectate commands. Everything else (keep-alive echoes,
        // their own movement, window clicks) is harmless and ignored.
        if id == sb::Chat::ID
            && let Ok(payload) = frame.payload()
            && let Ok(chat) = sb::Chat::from_payload(&payload)
        {
            broadcast.on_spectator_chat(&name, &chat.message);
        }
    }
}
