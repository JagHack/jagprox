//! Live game broadcasting: let a friend spectate your Hypixel game through your
//! proxy, with Hypixel only ever seeing you.
//!
//! The proxy tracks your whole world as Hypixel sends it ([`state::GameState`]).
//! A spectator's proxy finds yours through the JagProx rendezvous server
//! ([`compass`]), is relayed a connection to it, is given a snapshot of the world
//! and then the live stream ([`host`]), and swaps its Minecraft client over from
//! Hypixel to that feed ([`spectator`]). The [`Broadcast`] manager below owns the
//! rendezvous control connection and fans the live stream out to every spectator.

pub mod compass;
pub mod host;
pub mod spectator;
pub mod state;
pub mod transform;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use mcproto::packets::{Packet, clientbound as cb, parse_uuid};
use serde_json::json;
use tokio::sync::mpsc;

use compass::{ClientMsg, ServerMsg};
pub use state::GameState;

use super::session::Session;
use crate::log::log;

/// The default rendezvous server.
///
/// A raw IP rather than a hostname: the compass broker speaks plain TCP on
/// 7676, and `jagprox.jaghack.com` is Cloudflare-proxied (HTTP/HTTPS only), so
/// that name can't carry this port. Override it with a `compass:` line in
/// `config.yml` if you run your own broker.
pub const DEFAULT_COMPASS: &str = "77.42.31.200:7676";

struct Inner {
    /// One channel per connected spectator, carrying ready-to-send payloads.
    spectators: Vec<mpsc::Sender<Vec<u8>>>,
}

impl Inner {
    /// Sends a payload to every spectator, dropping any whose connection has closed.
    fn fanout(&mut self, payload: &[u8]) {
        self.spectators.retain(|tx| {
            !matches!(
                tx.try_send(payload.to_vec()),
                Err(mpsc::error::TrySendError::Closed(_))
            )
        });
    }
}

/// A running broadcast: the rendezvous control channel and the live fan-out.
pub struct Broadcast {
    inner: Mutex<Inner>,
    /// The tracked world, shared with the owning session (which feeds it on every
    /// clientbound packet, including the chunks loaded before broadcasting began).
    game: Arc<Mutex<GameState>>,
    uuid: u128,
    /// Commands to send up the rendezvous control connection (accept/deny/ping).
    control_tx: mpsc::Sender<ClientMsg>,
    /// JSON chat component + position to show the broadcaster.
    chat_tx: mpsc::UnboundedSender<(String, i8)>,
    closed: AtomicBool,
}

impl Broadcast {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn game(&self) -> std::sync::MutexGuard<'_, GameState> {
        self.game.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Feeds one clientbound payload from Hypixel: track it, then forward to spectators.
    ///
    /// The world lock is held across the fan-out so [`add_spectator`](Self::add_spectator)
    /// can't snapshot and register between the two and miss or duplicate a packet.
    pub fn observe(&self, payload: &[u8]) {
        let mut game = self.game();
        game.observe(payload);
        if let Some(forward) = transform::for_spectator(payload) {
            self.lock().fanout(&forward);
        }
    }

    /// The broadcaster moved and/or turned; move their model for every spectator.
    /// Either part may be absent (the three serverbound movement packets carry
    /// position only, look only, or both).
    pub fn broadcaster_move(&self, pos: Option<(f64, f64, f64)>, look: Option<(f32, f32)>) {
        let mut game = self.game();
        let cur = game.position.unwrap_or((0.0, 64.0, 0.0, 0.0, 0.0));
        let (x, y, z) = pos.unwrap_or((cur.0, cur.1, cur.2));
        let (yaw, pitch) = look.unwrap_or((cur.3, cur.4));
        game.position = Some((x, y, z, yaw, pitch));
        let packets = transform::move_broadcaster(game.self_eid, x, y, z, yaw, pitch);
        let mut inner = self.lock();
        for p in packets {
            inner.fanout(&p);
        }
    }

    /// The broadcaster swung their arm.
    pub fn broadcaster_swung(&self) {
        let game = self.game();
        let p = transform::swing_broadcaster(game.self_eid);
        self.lock().fanout(&p);
    }

    /// The play packets to send a joining spectator before the world snapshot.
    pub fn spectator_join_sequence(&self) -> Vec<Vec<u8>> {
        let dimension = self.game().dimension as i8;
        let join = cb::JoinGame {
            entity_id: host::SPECTATOR_EID,
            game_mode: 2, // adventure
            dimension,
            difficulty: 2,
            max_players: 1,
            level_type: "flat".into(),
            reduced_debug_info: false,
        }
        .to_payload();
        vec![join, host::abilities()]
    }

    /// Snapshots the world and registers a new spectator, atomically so no live
    /// packet is missed or duplicated. The snapshot already ends with the broadcaster
    /// model and a position for the spectator.
    pub fn add_spectator(&self) -> (Vec<Vec<u8>>, mpsc::Receiver<Vec<u8>>) {
        // Hold the world lock across snapshot + register so no live packet handled
        // by `observe` slips in between (it takes the same lock).
        let game = self.game();
        let mut out = game.snapshot();
        let eid = game.self_eid;
        let pos = game.position.unwrap_or((0.0, 64.0, 0.0, 0.0, 0.0));
        out.push(transform::spawn_broadcaster(
            eid, self.uuid, pos.0, pos.1, pos.2, pos.3, pos.4,
        ));
        // Place the spectator a couple of blocks above the broadcaster to start.
        out.push(host::position(pos.0, pos.1 + 2.0, pos.2, pos.3, pos.4));
        let (tx, rx) = mpsc::channel(2048);
        self.lock().spectators.push(tx);
        (out, rx)
    }

    /// Approve a pending spectator (from a `/bc accept` click).
    pub fn accept(&self, from: &str) {
        let _ = self.control_tx.try_send(ClientMsg::Accept {
            from: from.to_string(),
        });
    }

    /// Reject a pending spectator.
    pub fn deny(&self, from: &str) {
        let _ = self.control_tx.try_send(ClientMsg::Deny {
            from: from.to_string(),
        });
    }

    /// Show the broadcaster a clickable approve/deny prompt for a join request.
    fn prompt_join(&self, from: &str) {
        let line = json!({
            "text": format!("§d{from} §7wants to watch your game  "),
            "extra": [
                { "text": "§a§l[Accept]", "clickEvent": { "action": "run_command", "value": format!("/bc accept {from}") } },
                { "text": "  §c§l[Deny]", "clickEvent": { "action": "run_command", "value": format!("/bc deny {from}") } },
            ]
        })
        .to_string();
        let _ = self.chat_tx.send((line, 0));
    }

    fn broadcaster_chat(&self, text: &str) {
        let _ = self.chat_tx.send((json!({ "text": text }).to_string(), 0));
    }

    /// A spectator said something; surface it to the broadcaster.
    pub fn on_spectator_chat(&self, name: &str, message: &str) {
        if message.starts_with('/') {
            return; // spectator commands are handled by their own proxy
        }
        self.broadcaster_chat(&format!("§d[Watch] §f{name}§7: §f{message}"));
    }

    /// Number of connected spectators.
    pub fn spectator_count(&self) -> usize {
        self.lock().spectators.len()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Stops broadcasting: disconnect current spectators and refuse new ones. The
    /// rendezvous control task notices on its next message and exits.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.lock().spectators.clear();
    }
}

/// Starts broadcasting for the current session: connects to the rendezvous server,
/// registers, and drives the control connection (approving spectators, spawning a
/// host session per relay). Returns the [`Broadcast`] handle the session feeds.
pub async fn start(session: &Session, compass_addr: &str) -> Result<Arc<Broadcast>, String> {
    let name = session.username().to_string();
    let uuid = parse_uuid(session.uuid()).unwrap_or(0);

    let stream = compass::connect(compass_addr)
        .await
        .map_err(|e| format!("couldn't reach the broadcast server: {e}"))?;
    let (mut read, mut write) = stream.into_split();

    compass::write_msg(
        &mut write,
        &ClientMsg::Register {
            name: name.clone(),
            uuid: session.uuid().to_string(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    loop {
        match compass::read_msg(&mut read, &mut buf)
            .await
            .map_err(|e| e.to_string())?
        {
            ServerMsg::Registered => break,
            ServerMsg::Pong => continue,
            ServerMsg::Error { msg } => return Err(msg),
            _ => continue,
        }
    }

    let (control_tx, mut control_rx) = mpsc::channel::<ClientMsg>(32);
    let (chat_tx, mut chat_rx) = mpsc::unbounded_channel::<(String, i8)>();
    let broadcast = Arc::new(Broadcast {
        inner: Mutex::new(Inner {
            spectators: Vec::new(),
        }),
        game: session.game_handle(),
        uuid,
        control_tx,
        chat_tx,
        closed: AtomicBool::new(false),
    });

    // Writer task: send control commands up to the rendezvous server.
    session.spawn(async move {
        while let Some(msg) = control_rx.recv().await {
            if compass::write_msg(&mut write, &msg).await.is_err() {
                break;
            }
        }
    });

    // Chat task: render prompts/messages to the broadcaster's client.
    let chat_session = session.clone();
    session.spawn(async move {
        while let Some((json, pos)) = chat_rx.recv().await {
            chat_session.chat_at(&json, pos);
        }
    });

    // Control reader: handle join requests and relay hand-offs.
    let addr = compass_addr.to_string();
    let b = broadcast.clone();
    let loop_session = session.clone();
    session.spawn(async move {
        let mut buf = buf;
        loop {
            let msg = match compass::read_msg(&mut read, &mut buf).await {
                Ok(m) => m,
                Err(_) => {
                    log("Broadcast: disconnected from the rendezvous server.");
                    break;
                }
            };
            match msg {
                ServerMsg::JoinRequest { from } => b.prompt_join(&from),
                ServerMsg::StartRelay {
                    token,
                    host: true,
                    peer,
                } => {
                    if b.is_closed() {
                        continue;
                    }
                    b.broadcaster_chat(&format!("§a{peer} is now watching your game."));
                    match compass::open_relay(&addr, &token).await {
                        Ok(relay) => {
                            let host_b = b.clone();
                            loop_session.spawn(async move {
                                if let Err(e) = host::run(relay, host_b).await {
                                    log(format!("Broadcast: spectator session ended: {e}"));
                                }
                            });
                        }
                        Err(e) => log(format!("Broadcast: couldn't open relay: {e}")),
                    }
                }
                ServerMsg::StartRelay { .. } => {}
                ServerMsg::Denied { reason } => log(format!("Broadcast: {reason}")),
                ServerMsg::Error { msg } => log(format!("Broadcast error: {msg}")),
                ServerMsg::Registered | ServerMsg::Pong => {}
            }
        }
    });

    Ok(broadcast)
}

#[cfg(test)]
impl Broadcast {
    /// A broadcast with no rendezvous connection, for feeding a world and serving
    /// host sessions directly in tests.
    fn detached(uuid: u128) -> Arc<Self> {
        let (control_tx, _control_rx) = mpsc::channel(8);
        let (chat_tx, _chat_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            inner: Mutex::new(Inner {
                spectators: Vec::new(),
            }),
            game: Arc::new(Mutex::new(GameState::new())),
            uuid,
            control_tx,
            chat_tx,
            closed: AtomicBool::new(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcproto::conn::{self, Connection};
    use mcproto::frame::Frame;
    use mcproto::world;
    use tokio::net::TcpListener;

    /// A broadcaster feeds a world, a spectator connects over the real rendezvous
    /// server + relay, and receives the world snapshot: the full host login +
    /// snapshot handshake over a real socket, end to end.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spectator_receives_world_snapshot_over_relay() {
        // Real rendezvous server.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let compass_addr = listener.local_addr().unwrap().to_string();
        tokio::spawn(jagprox_compass::serve(listener));

        // Broadcaster: feed a world (JoinGame + a chunk + a mob).
        let broadcast = Broadcast::detached(0xABCD);
        broadcast.observe(
            &cb::JoinGame {
                entity_id: 7,
                game_mode: 0,
                dimension: 0,
                difficulty: 2,
                max_players: 1,
                level_type: "default".into(),
                reduced_debug_info: false,
            }
            .to_payload(),
        );
        broadcast.observe(&world::chunk_data_payload(0, 0, true, 0b1, &[0u8; 64]));
        let mut mob = Vec::new();
        mcproto::buf::Write::varint(&mut mob, world::id::SPAWN_MOB);
        mcproto::buf::Write::varint(&mut mob, 200);
        mcproto::buf::Write::u8(&mut mob, 54);
        for v in [32 * 5i32, 32 * 64, 32 * 5] {
            mcproto::buf::Write::i32(&mut mob, v);
        }
        for _ in 0..3 {
            mcproto::buf::Write::i8(&mut mob, 0);
        }
        for _ in 0..3 {
            mcproto::buf::Write::i16(&mut mob, 0);
        }
        mcproto::buf::Write::u8(&mut mob, 0x7f);
        broadcast.observe(&mob);

        // Broadcaster control connection: register, accept the join, and on relay
        // hand-off run the host session.
        let bc_stream = compass::connect(&compass_addr).await.unwrap();
        let (mut bc_read, mut bc_write) = bc_stream.into_split();
        compass::write_msg(
            &mut bc_write,
            &ClientMsg::Register {
                name: "Host".into(),
                uuid: "abcd".into(),
            },
        )
        .await
        .unwrap();
        let mut bc_buf = Vec::new();
        assert!(matches!(
            compass::read_msg(&mut bc_read, &mut bc_buf).await.unwrap(),
            ServerMsg::Registered
        ));
        let addr2 = compass_addr.clone();
        let host_broadcast = broadcast.clone();
        tokio::spawn(async move {
            loop {
                match compass::read_msg(&mut bc_read, &mut bc_buf).await {
                    Ok(ServerMsg::JoinRequest { from }) => {
                        compass::write_msg(&mut bc_write, &ClientMsg::Accept { from })
                            .await
                            .unwrap();
                    }
                    Ok(ServerMsg::StartRelay {
                        token, host: true, ..
                    }) => {
                        let relay = compass::open_relay(&addr2, &token).await.unwrap();
                        let _ = host::run(relay, host_broadcast.clone()).await;
                        return;
                    }
                    _ => return,
                }
            }
        });

        // Spectator: join, get the relay, and log in to the host as a client.
        let spec_stream = compass::connect(&compass_addr).await.unwrap();
        let (mut sr, mut sw) = spec_stream.into_split();
        compass::write_msg(
            &mut sw,
            &ClientMsg::Join {
                target: "Host".into(),
                from: "Watcher".into(),
            },
        )
        .await
        .unwrap();
        let mut sbuf = Vec::new();
        let token = loop {
            match compass::read_msg(&mut sr, &mut sbuf).await.unwrap() {
                ServerMsg::StartRelay {
                    token, host: false, ..
                } => break token,
                ServerMsg::Denied { reason } => panic!("denied: {reason}"),
                _ => continue,
            }
        };
        let relay = compass::open_relay(&compass_addr, &token).await.unwrap();
        let (rr, rw) = relay.into_split();
        let mut host_conn = Connection::new(rr, rw);
        let login = conn::client_login(&mut host_conn, "broadcast", 0, "Watcher", |_| async {
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(login.username, "Watcher");

        // Read play packets; the snapshot must include our JoinGame, the chunk and
        // a teleport placing the mob.
        let mut saw_join = false;
        let mut saw_chunk = false;
        let mut saw_mob = false;
        for _ in 0..200 {
            let frame: Frame = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                host_conn.reader.read_frame(),
            )
            .await
            .expect("timed out waiting for snapshot")
            .expect("relay closed early");
            let payload = frame.payload().unwrap();
            match frame.id().unwrap() {
                cb::JoinGame::ID => saw_join = true,
                world::id::CHUNK_DATA => saw_chunk = true,
                world::id::ENTITY_TELEPORT
                    if world::EntityTeleport::from_payload(&payload)
                        .map(|t| t.eid == 200)
                        .unwrap_or(false) =>
                {
                    saw_mob = true;
                }
                _ => {}
            }
            if saw_join && saw_chunk && saw_mob {
                break;
            }
        }
        assert!(saw_join, "spectator never received JoinGame");
        assert!(saw_chunk, "spectator never received the chunk");
        assert!(saw_mob, "spectator never received the mob placement");
    }
}
