//! One player's session: two relay loops (player → Hypixel, Hypixel → player) and a
//! writer task per side. Only the packets JagProx looks at are decoded; everything
//! else is forwarded as the exact bytes that arrived.

use std::sync::Arc;
use std::time::Duration;

use mcproto::conn::Connection;
use mcproto::frame::{Frame, FrameReader, FrameWriter};
use mcproto::packets::{Packet, clientbound as cb, serverbound as sb, vanilla_brand};
use serde_json::json;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;

use super::server::Proxy;
use super::{CHAT_PREFIX, ProxyEvent};
use crate::formatter;
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

/// Handles modules and commands use to talk to both sides.
#[derive(Clone)]
pub struct Session {
    pub proxy: Arc<Proxy>,
    pub username: String,
    /// Dashed UUID.
    pub uuid: String,
    pub mc_session: Option<McSession>,
    to_client: mpsc::Sender<Out>,
    to_server: mpsc::Sender<Out>,
}

impl Session {
    pub fn send_to_client(&self, payload: Vec<u8>) {
        let _ = self.to_client.try_send(Out::Packet(payload));
    }

    pub fn send_to_server(&self, payload: Vec<u8>) {
        let _ = self.to_server.try_send(Out::Packet(payload));
    }

    /// A JagProx chat line for the player (JS `proxyChat`).
    pub fn chat(&self, message: &str) {
        let component = json!({ "text": format!("{CHAT_PREFIX}{message}") });
        self.send_to_client(
            cb::Chat {
                message: component.to_string(),
                position: 0,
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
}

const QUEUE: usize = 4096;

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
            if writer.has_pending() && next.is_none() && rx.is_empty() {
                break;
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
        proxy,
        username,
        uuid,
        mc_session,
        to_client,
        to_server,
    };

    let client_writer = tokio::spawn(writer_loop(client.writer, client_rx));
    let server_writer = tokio::spawn(writer_loop(server.writer, server_rx));
    let mut upstream = tokio::spawn(server_to_client(session.clone(), server.reader));
    let mut downstream = tokio::spawn(client_to_server(session.clone(), client.reader));

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
    // Let the writers send what's queued (e.g. a kick), then close both sockets.
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
                    .to_client
                    .send(Out::Packet(cb::Disconnect { reason }.to_payload()))
                    .await;
                return Err("Hypixel closed the connection".into());
            }
            Err(e) => return Err(e.to_string()),
        };
        let out = match handle_server_packet(&session, frame) {
            Ok(out) => out,
            Err((frame, e)) => {
                crate::log::debug(format!(
                    "Could not inspect a packet, forwarding it unchanged: {e}"
                ));
                Some(Out::Frame(frame))
            }
        };
        if let Some(out) = out
            && session.to_client.send(out).await.is_err()
        {
            return Ok(());
        }
    }
}

/// Decodes the packets JagProx cares about. Returns what to forward (`None` drops it).
fn handle_server_packet(
    session: &Session,
    frame: Frame,
) -> Result<Option<Out>, (Frame, mcproto::Error)> {
    let id = match frame.id() {
        Ok(id) => id,
        Err(e) => return Err((frame, e)),
    };
    match id {
        cb::Chat::ID => {
            let chat = match frame.payload().and_then(|p| cb::Chat::from_payload(&p)) {
                Ok(chat) => chat,
                Err(e) => return Err((frame, e)),
            };
            if chat.position == 0 || chat.position == 1 {
                let text = formatter::reconstruct_legacy_text_str(&chat.message);
                let _ = session.proxy.events().send(ProxyEvent::Chat(text));
            }
            Ok(Some(Out::Frame(frame)))
        }
        cb::PluginMessage::ID => {
            let mut message = match frame
                .payload()
                .and_then(|p| cb::PluginMessage::from_payload(&p))
            {
                Ok(m) => m,
                Err(e) => return Err((frame, e)),
            };
            if message.channel == "MC|Brand" {
                message.data = vanilla_brand();
                return Ok(Some(Out::Packet(message.to_payload())));
            }
            Ok(Some(Out::Frame(frame)))
        }
        _ => Ok(Some(Out::Frame(frame))),
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
        let out = match frame.id() {
            Ok(sb::PluginMessage::ID) => match frame
                .payload()
                .and_then(|p| sb::PluginMessage::from_payload(&p))
            {
                Ok(mut message) if message.channel == "MC|Brand" => {
                    message.data = vanilla_brand();
                    Out::Packet(message.to_payload())
                }
                _ => Out::Frame(frame),
            },
            Ok(sb::Chat::ID) => {
                if let Ok(chat) = frame.payload().and_then(|p| sb::Chat::from_payload(&p))
                    && chat.message.to_lowercase().starts_with("/play ")
                {
                    log(format!("Captured last play command: {}", chat.message));
                }
                Out::Frame(frame)
            }
            _ => Out::Frame(frame),
        };
        if session.to_server.send(out).await.is_err() {
            return Ok(());
        }
    }
}
