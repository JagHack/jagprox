//! One player's session: two relay loops (player → Hypixel, Hypixel → player), a writer
//! task per side, and the session's module state and timers. Only the packets JagProx
//! looks at are decoded; everything else is forwarded as the exact bytes that arrived.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mcproto::conn::Connection;
use mcproto::frame::{Frame, FrameReader, FrameWriter};
use mcproto::packets::{Packet, clientbound as cb, serverbound as sb, vanilla_brand};
use serde_json::json;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::modules::{self, ServerPacket, State};
use super::server::Proxy;
use super::{CHAT_PREFIX, commands};
use crate::api::{Backend, Hypixel};
use crate::log::log;
use crate::msauth::McSession;

/// Something to send to one side.
#[derive(Debug)]
pub enum Out {
    /// A frame read from the other side, forwarded as-is when possible.
    Frame(Frame),
    /// A packet (id + data).
    Packet(Vec<u8>),
}

struct Inner {
    proxy: Arc<Proxy>,
    username: String,
    /// Dashed UUID.
    uuid: String,
    mc_session: Option<McSession>,
    to_client: mpsc::Sender<Out>,
    to_server: mpsc::Sender<Out>,
    state: Mutex<State>,
    tasks: Mutex<Vec<AbortHandle>>,
    /// Set while this session is broadcasting its game to spectators.
    broadcast: Mutex<Option<Arc<super::broadcast::Broadcast>>>,
    /// The tracked world, fed on every clientbound packet so a broadcast started
    /// mid-game already has the chunks/entities Hypixel sent at join. Shared with
    /// the active `Broadcast`, if any.
    game: Arc<Mutex<super::broadcast::GameState>>,
}

/// Handles modules and commands use to talk to both sides. Cheap to clone.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

impl Session {
    pub fn proxy(&self) -> &Arc<Proxy> {
        &self.inner.proxy
    }

    pub fn hypixel(&self) -> &Hypixel {
        self.inner.proxy.hypixel()
    }

    pub fn backend(&self) -> Option<Backend> {
        let backend = self.inner.proxy.hypixel().backend().clone();
        backend.jwt().is_some().then_some(backend)
    }

    pub fn username(&self) -> &str {
        &self.inner.username
    }

    pub fn uuid(&self) -> &str {
        &self.inner.uuid
    }

    pub fn mc_session(&self) -> Option<&McSession> {
        self.inner.mc_session.as_ref()
    }

    /// The modules' state. Never hold this across an `.await`.
    pub fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The active broadcast, if this session is broadcasting.
    pub fn broadcast(&self) -> Option<Arc<super::broadcast::Broadcast>> {
        self.inner
            .broadcast
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Starts or stops broadcasting.
    pub fn set_broadcast(&self, broadcast: Option<Arc<super::broadcast::Broadcast>>) {
        *self
            .inner
            .broadcast
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = broadcast;
    }

    /// Feeds one clientbound payload into the always-on world tracker. Used while
    /// not broadcasting; a `Broadcast` updates the same shared state itself.
    pub fn observe_world(&self, payload: &[u8]) {
        self.inner
            .game
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .observe(payload);
    }

    /// A handle to the tracked world, shared with a `Broadcast` so it snapshots the
    /// world that was already loaded before broadcasting started.
    pub fn game_handle(&self) -> Arc<Mutex<super::broadcast::GameState>> {
        self.inner.game.clone()
    }

    /// Runs a task that is cancelled when the session ends.
    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        let handle = tokio::spawn(task).abort_handle();
        let mut tasks = self.inner.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.retain(|t| !t.is_finished());
        tasks.push(handle);
    }

    pub fn send_to_client(&self, payload: Vec<u8>) {
        let _ = self.inner.to_client.try_send(Out::Packet(payload));
    }

    pub fn send_to_server(&self, payload: Vec<u8>) {
        let _ = self.inner.to_server.try_send(Out::Packet(payload));
    }

    /// A JagProx chat line for the player (JS `proxyChat`).
    pub fn chat(&self, message: &str) {
        let component = json!({ "text": format!("{CHAT_PREFIX}{message}") });
        self.chat_at(&component.to_string(), 0);
    }

    /// A raw JSON chat component at a chat position.
    pub fn chat_at(&self, json: &str, position: i8) {
        self.send_to_client(
            cb::Chat {
                message: json.to_string(),
                position,
            }
            .to_payload(),
        );
    }

    /// Chat or a command, sent to Hypixel as if the player typed it.
    pub fn send_server_chat(&self, message: &str) {
        self.send_to_server(
            sb::Chat {
                message: message.to_string(),
            }
            .to_payload(),
        );
    }

    fn abort_tasks(&self) {
        for task in self
            .inner
            .tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            task.abort();
        }
    }
}

const QUEUE: usize = 4096;

/// Aborts the session's tasks when dropped, including when the proxy is stopped
/// and the session itself is aborted.
struct AbortOnDrop(Vec<AbortHandle>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

/// Writes everything queued for one side, batching what's ready into one write.
async fn writer_loop(
    mut writer: FrameWriter<OwnedWriteHalf>,
    mut rx: mpsc::Receiver<Out>,
) -> mcproto::Result<()> {
    while let Some(first) = rx.recv().await {
        let mut next = Some(first);
        while let Some(out) = next.take().or_else(|| rx.try_recv().ok()) {
            match out {
                Out::Frame(frame) => writer.queue_frame(&frame)?,
                Out::Packet(payload) => writer.queue_packet(&payload)?,
            }
        }
        writer.flush().await?;
    }
    Ok(())
}

pub(super) async fn run(
    proxy: Arc<Proxy>,
    client: Connection<OwnedReadHalf, OwnedWriteHalf>,
    server: Connection<OwnedReadHalf, OwnedWriteHalf>,
    username: String,
    uuid: String,
    mc_session: Option<McSession>,
) -> Result<(), String> {
    let (to_client, client_rx) = mpsc::channel(QUEUE);
    let (to_server, server_rx) = mpsc::channel(QUEUE);
    let session = Session {
        inner: Arc::new(Inner {
            proxy: proxy.clone(),
            username,
            uuid,
            mc_session,
            to_client,
            to_server,
            state: Mutex::new(State::default()),
            tasks: Mutex::new(Vec::new()),
            broadcast: Mutex::new(None),
            game: Arc::new(Mutex::new(super::broadcast::GameState::new())),
        }),
    };
    proxy.set_current(Some(session.clone()));

    let client_writer = tokio::spawn(writer_loop(client.writer, client_rx));
    let server_writer = tokio::spawn(writer_loop(server.writer, server_rx));
    let mut upstream = tokio::spawn(server_to_client(session.clone(), server.reader));
    let mut downstream = tokio::spawn(client_to_server(session.clone(), client.reader));
    session.spawn(modules::super_friends::run(session.clone()));
    let _guard = AbortOnDrop(vec![
        upstream.abort_handle(),
        downstream.abort_handle(),
        client_writer.abort_handle(),
        server_writer.abort_handle(),
    ]);
    let tasks = session.clone();
    let _session_tasks = scopeguard(move || {
        tasks.proxy().set_current(None);
        tasks.abort_tasks();
    });

    let reason = tokio::select! {
        r = &mut upstream => { downstream.abort(); r }
        r = &mut downstream => { upstream.abort(); r }
    };
    let reason = match reason {
        Ok(Err(e)) => e,
        Ok(Ok(())) => "closed".into(),
        Err(e) if e.is_cancelled() => "closed".into(),
        Err(e) => format!("relay crashed: {e}"),
    };

    // Session over: stop its timers, reset the modules, and let the writers send
    // what's queued (e.g. a kick) before both sockets close.
    proxy.set_current(None);
    session.set_broadcast(None);
    session.abort_tasks();
    modules::reset(&mut session.state());
    drop(session);
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        let _ = client_writer.await;
        let _ = server_writer.await;
    })
    .await;
    if reason != "closed" {
        log(format!("Connection error: {reason}"));
    }
    Ok(())
}

/// Hypixel → player.
async fn server_to_client(
    session: Session,
    mut reader: FrameReader<OwnedReadHalf>,
) -> Result<(), String> {
    loop {
        let frame = match reader.read_frame().await {
            Ok(frame) => frame,
            Err(mcproto::Error::Closed) => {
                let reason = json!({ "text": "§cLost connection to the server." }).to_string();
                let _ = session
                    .inner
                    .to_client
                    .send(Out::Packet(cb::Disconnect { reason }.to_payload()))
                    .await;
                return Err("Hypixel closed the connection".into());
            }
            Err(e) => return Err(e.to_string()),
        };
        // Track the world on every packet so a broadcast started mid-game already
        // has the chunks Hypixel sent at join; while broadcasting, also fan the
        // live stream out to spectators (both update the same shared GameState).
        if let Ok(payload) = frame.payload() {
            if let Some(broadcast) = session.broadcast() {
                broadcast.observe(&payload);
            } else {
                session.observe_world(&payload);
            }
        }
        let out = inspect_server_frame(&session, frame);
        if session.inner.to_client.send(out).await.is_err() {
            return Ok(());
        }
    }
}

/// Runs the modules on a packet they care about; anything else (or anything that
/// fails to decode) is forwarded untouched.
/// Runs `f` when dropped.
fn scopeguard(f: impl FnOnce()) -> impl Drop {
    struct Guard<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Guard<F> {
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }
    Guard(Some(f))
}

fn inspect_server_frame(session: &Session, frame: Frame) -> Out {
    let Ok(id) = frame.id() else {
        return Out::Frame(frame);
    };
    let decoded = match frame.payload() {
        Ok(payload) => ServerPacket::decode(id, &payload),
        Err(_) => None,
    };
    let mut packet = match decoded {
        None => return Out::Frame(frame),
        Some(Ok(packet)) => packet,
        Some(Err(e)) => {
            crate::log::debug(format!(
                "Could not decode packet {id:#04x}, forwarding it unchanged: {e}"
            ));
            return Out::Frame(frame);
        }
    };
    // A bug in one module must never take down the session.
    let changed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        modules::on_server_packet(session, &mut packet)
    }));
    match changed {
        Ok(true) => packet.encode().map_or(Out::Frame(frame), Out::Packet),
        Ok(false) => Out::Frame(frame),
        Err(_) => {
            log(format!(
                "[ERROR] A module failed on packet {id:#04x}; it was forwarded unchanged."
            ));
            Out::Frame(frame)
        }
    }
}

/// Player → Hypixel.
async fn client_to_server(
    session: Session,
    mut reader: FrameReader<OwnedReadHalf>,
) -> Result<(), String> {
    loop {
        let frame = match reader.read_frame().await {
            Ok(frame) => frame,
            Err(mcproto::Error::Closed) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        // While broadcasting, drive the broadcaster's model from their own movement.
        if let Some(broadcast) = session.broadcast() {
            feed_broadcaster_movement(&broadcast, &frame);
        }
        let out = match frame.id() {
            Ok(sb::PluginMessage::ID) => match frame
                .payload()
                .and_then(|p| sb::PluginMessage::from_payload(&p))
            {
                Ok(mut message) if message.channel == "MC|Brand" => {
                    message.data = vanilla_brand();
                    Some(Out::Packet(message.to_payload()))
                }
                _ => Some(Out::Frame(frame)),
            },
            Ok(sb::Chat::ID) => match frame.payload().and_then(|p| sb::Chat::from_payload(&p)) {
                Ok(chat) => on_player_chat(&session, &chat.message).then_some(Out::Frame(frame)),
                Err(_) => Some(Out::Frame(frame)),
            },
            _ => Some(Out::Frame(frame)),
        };
        if let Some(out) = out
            && session.inner.to_server.send(out).await.is_err()
        {
            return Ok(());
        }
    }
}

/// Feeds the broadcaster's own movement/arm-swing into their spectators' model.
fn feed_broadcaster_movement(broadcast: &super::broadcast::Broadcast, frame: &Frame) {
    let Ok(id) = frame.id() else { return };
    let Ok(payload) = frame.payload() else { return };
    match id {
        sb::PlayerPosition::ID => {
            if let Ok(p) = sb::PlayerPosition::from_payload(&payload) {
                broadcast.broadcaster_move(Some((p.x, p.y, p.z)), None);
            }
        }
        sb::PlayerLook::ID => {
            if let Ok(p) = sb::PlayerLook::from_payload(&payload) {
                broadcast.broadcaster_move(None, Some((p.yaw, p.pitch)));
            }
        }
        sb::PlayerPositionAndLook::ID => {
            if let Ok(p) = sb::PlayerPositionAndLook::from_payload(&payload) {
                broadcast.broadcaster_move(Some((p.x, p.y, p.z)), Some((p.yaw, p.pitch)));
            }
        }
        sb::ArmAnimation::ID => broadcast.broadcaster_swung(),
        // Serverbound Held Item Change (0x09): the broadcaster picked a hotbar slot.
        0x09 => {
            let mut r: &[u8] = &payload;
            let _ = mcproto::buf::Read::varint(&mut r);
            if let Ok(slot) = mcproto::buf::Read::i16(&mut r)
                && (0..=8).contains(&slot)
            {
                broadcast.broadcaster_held(slot as u8);
            }
        }
        _ => {}
    }
}

/// Returns whether the chat message should still go to Hypixel.
fn on_player_chat(session: &Session, message: &str) -> bool {
    if !message.starts_with('/') {
        return true;
    }
    let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        commands::handle(session, message)
    }))
    .unwrap_or_else(|_| {
        log(format!("[ERROR] commands.handle failed on \"{message}\""));
        false
    });
    if handled {
        return false;
    }
    if message.to_lowercase().starts_with("/play ") {
        session.state().last_play_command = Some(message.to_string());
        log(format!("Captured last play command: {message}"));
    }
    true
}
