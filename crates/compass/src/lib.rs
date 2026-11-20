//! JagProx broadcasting rendezvous + relay server ("compass").
//!
//! Broadcasters register and keep a control connection open. A spectator opens a
//! control connection and asks to `join` a broadcaster by name; the broadcaster is
//! shown the request and `accept`s or `deny`s it. On accept, both sides are told to
//! open a fresh data connection carrying a shared one-time token; the server pairs
//! the two by token and pipes raw bytes between them. All Minecraft traffic flows
//! over that pipe — Hypixel only ever talks to the broadcaster.

pub mod protocol;

use std::collections::HashMap;
use std::sync::Arc;

use protocol::{ClientMsg, ServerMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};

/// A registered broadcaster's control channel and its pending spectators.
struct Broadcaster {
    tx: mpsc::Sender<ServerMsg>,
    /// Spectator name (lowercased) -> a channel to that spectator's control conn.
    pending: HashMap<String, mpsc::Sender<ServerMsg>>,
}

#[derive(Default)]
pub struct Hub {
    broadcasters: Mutex<HashMap<String, Broadcaster>>,
    /// token -> the first data socket to arrive, waiting for its twin.
    relays: Mutex<HashMap<String, TcpStream>>,
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// A short non-cryptographic pairing nonce (a relay only ever joins two already
/// approved peers, so this just needs to be unguessable-enough and unique).
fn token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut x = nanos as u64
        ^ (std::process::id() as u64).rotate_left(32)
        ^ COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    format!("{x:016x}")
}

/// Accepts connections until the listener errors permanently.
pub async fn serve(listener: TcpListener) {
    let hub = Arc::new(Hub::default());
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("compass: accept error: {e}");
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        let hub = hub.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(hub, stream).await {
                eprintln!("compass: connection {peer} ended: {e}");
            }
        });
    }
}

/// Reads one `\n`-terminated line one byte at a time, so no bytes past the newline
/// are consumed (a relay data connection sends its token line, then raw game bytes).
async fn read_line_raw(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut line = Vec::with_capacity(128);
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
        if line.len() > 8192 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "control line too long",
            ));
        }
    }
    String::from_utf8(line)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "control line not UTF-8"))
}

async fn handle(hub: Arc<Hub>, mut stream: TcpStream) -> std::io::Result<()> {
    let first = read_line_raw(&mut stream).await?;
    if first.is_empty() {
        return Ok(());
    }
    let msg: ClientMsg = serde_json::from_str(&first)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    match msg {
        ClientMsg::Relay { token } => relay_pair(hub, token, stream).await,
        other => control(hub, stream, other).await,
    }
}

/// Pairs a data connection with its twin by token, then pipes raw bytes both ways.
async fn relay_pair(hub: Arc<Hub>, token: String, mut stream: TcpStream) -> std::io::Result<()> {
    let twin = hub.relays.lock().await.remove(&token);
    match twin {
        Some(mut first) => {
            let _ = tokio::io::copy_bidirectional(&mut first, &mut stream).await;
            Ok(())
        }
        None => {
            hub.relays.lock().await.insert(token.clone(), stream);
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            hub.relays.lock().await.remove(&token);
            Ok(())
        }
    }
}

/// A control connection: a broadcaster that stays registered, or a spectator waiting
/// for approval. `first` is the message already read.
async fn control(hub: Arc<Hub>, stream: TcpStream, first: ClientMsg) -> std::io::Result<()> {
    let (mut read, mut write) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<ServerMsg>(32);

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let mut line = match serde_json::to_vec(&msg) {
                Ok(l) => l,
                Err(_) => continue,
            };
            line.push(b'\n');
            if write.write_all(&line).await.is_err() {
                break;
            }
        }
    });

    let mut my_name: Option<String> = None;
    let mut message = Some(first);
    let mut buf = Vec::with_capacity(512);

    let result = loop {
        let msg = match message.take() {
            Some(m) => m,
            None => match read_line(&mut read, &mut buf).await {
                Ok(Some(m)) => m,
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
            },
        };
        if let Err(e) = handle_control(&hub, &tx, &mut my_name, msg).await {
            break Err(e);
        }
    };

    if let Some(name) = &my_name {
        hub.broadcasters.lock().await.remove(name);
    }
    drop(tx);
    let _ = writer.await;
    result
}

async fn handle_control(
    hub: &Arc<Hub>,
    tx: &mpsc::Sender<ServerMsg>,
    my_name: &mut Option<String>,
    msg: ClientMsg,
) -> std::io::Result<()> {
    match msg {
        ClientMsg::Register { name, .. } => {
            let k = key(&name);
            if k.is_empty() {
                let _ = tx
                    .send(ServerMsg::Error {
                        msg: "empty name".into(),
                    })
                    .await;
                return Ok(());
            }
            hub.broadcasters.lock().await.insert(
                k.clone(),
                Broadcaster {
                    tx: tx.clone(),
                    pending: HashMap::new(),
                },
            );
            *my_name = Some(k);
            let _ = tx.send(ServerMsg::Registered).await;
        }
        ClientMsg::Join { target, from } => {
            let mut bcs = hub.broadcasters.lock().await;
            match bcs.get_mut(&key(&target)) {
                Some(bc) => {
                    bc.pending.insert(key(&from), tx.clone());
                    let _ = bc.tx.send(ServerMsg::JoinRequest { from }).await;
                }
                None => {
                    let _ = tx
                        .send(ServerMsg::Denied {
                            reason: format!("{target} isn't broadcasting right now"),
                        })
                        .await;
                }
            }
        }
        ClientMsg::Accept { from } => {
            let Some(name) = my_name.clone() else {
                return Ok(());
            };
            let spectator = {
                let mut bcs = hub.broadcasters.lock().await;
                bcs.get_mut(&name)
                    .and_then(|bc| bc.pending.remove(&key(&from)))
            };
            if let Some(spectator_tx) = spectator {
                let token = token();
                let _ = tx
                    .send(ServerMsg::StartRelay {
                        token: token.clone(),
                        host: true,
                        peer: from.clone(),
                    })
                    .await;
                let _ = spectator_tx
                    .send(ServerMsg::StartRelay {
                        token,
                        host: false,
                        peer: name,
                    })
                    .await;
            }
        }
        ClientMsg::Deny { from } => {
            if let Some(name) = my_name.clone() {
                let mut bcs = hub.broadcasters.lock().await;
                if let Some(bc) = bcs.get_mut(&name)
                    && let Some(spectator_tx) = bc.pending.remove(&key(&from))
                {
                    let _ = spectator_tx
                        .send(ServerMsg::Denied {
                            reason: "the broadcaster declined".into(),
                        })
                        .await;
                }
            }
        }
        ClientMsg::Ping => {
            let _ = tx.send(ServerMsg::Pong).await;
        }
        ClientMsg::Relay { .. } => {
            let _ = tx
                .send(ServerMsg::Error {
                    msg: "relay must be a fresh connection".into(),
                })
                .await;
        }
    }
    Ok(())
}

/// Reads one JSON line from a persistent control connection (over-reading past a
/// newline is fine here: control connections never carry raw relay bytes).
async fn read_line(
    read: &mut tokio::net::tcp::OwnedReadHalf,
    buf: &mut Vec<u8>,
) -> std::io::Result<Option<ClientMsg>> {
    loop {
        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let text = &line[..line.len() - 1];
            if text.is_empty() {
                continue;
            }
            let msg = serde_json::from_slice(text)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            return Ok(Some(msg));
        }
        let mut chunk = [0u8; 1024];
        let n = read.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 64 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "control buffer overflow",
            ));
        }
    }
}
