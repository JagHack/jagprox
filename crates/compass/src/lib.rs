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
use std::time::{Duration, Instant};

use protocol::{ClientMsg, ServerMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore, mpsc};

const MAX_CONNECTIONS: usize = 2048;
const MAX_PENDING: usize = 16;
const FIRST_LINE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long an issued relay token, or the first data socket waiting on it, lives.
const RELAY_TTL: Duration = Duration::from_secs(60);

/// A registered broadcaster's control channel and its pending spectators.
struct Broadcaster {
    tx: mpsc::Sender<ServerMsg>,
    /// Spectator name (lowercased) -> a channel to that spectator's control conn.
    pending: HashMap<String, mpsc::Sender<ServerMsg>>,
}

/// A relay token's state: issued at accept, then holding the first data socket.
enum RelaySlot {
    Issued(Instant),
    Waiting(TcpStream),
}

#[derive(Default)]
pub struct Hub {
    broadcasters: Mutex<HashMap<String, Broadcaster>>,
    relays: Mutex<HashMap<String, RelaySlot>>,
}

fn key(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// A Minecraft username: `^[A-Za-z0-9_]{1,16}$`.
fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A 128-bit OS-random relay token, hex-encoded; holding it is the only proof a
/// data connection belongs to an approved pair.
fn token() -> std::io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Accepts connections until the listener errors permanently.
pub async fn serve(listener: TcpListener) {
    let hub = Arc::new(Hub::default());
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        // Waiting for a slot before accepting leaves excess clients in the backlog
        // instead of exhausting file descriptors.
        let Ok(permit) = slots.clone().acquire_owned().await else {
            return;
        };
        let (stream, peer) = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("compass: accept error: {e}");
                // e.g. EMFILE: retrying immediately would spin.
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        // Detects half-open peers so a dead broadcaster can't hold its name forever.
        let _ = socket2::SockRef::from(&stream).set_tcp_keepalive(
            &socket2::TcpKeepalive::new()
                .with_time(Duration::from_secs(60))
                .with_interval(Duration::from_secs(10)),
        );
        let hub = hub.clone();
        tokio::spawn(async move {
            let _permit = permit;
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
    let first = tokio::time::timeout(FIRST_LINE_TIMEOUT, read_line_raw(&mut stream))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "no first line"))??;
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

/// Pairs a data connection with its twin by an issued token, then pipes raw bytes
/// both ways. A token pairs exactly once; unknown or expired tokens are dropped.
async fn relay_pair(hub: Arc<Hub>, token: String, mut stream: TcpStream) -> std::io::Result<()> {
    let mut twin = {
        let mut relays = hub.relays.lock().await;
        match relays.remove(&token) {
            Some(RelaySlot::Waiting(first)) => first,
            Some(RelaySlot::Issued(at)) if at.elapsed() < RELAY_TTL => {
                relays.insert(token.clone(), RelaySlot::Waiting(stream));
                drop(relays);
                tokio::time::sleep(RELAY_TTL).await;
                // Still here means the twin never came; the 128-bit token can't
                // have been reissued, so this only ever drops our own socket.
                hub.relays.lock().await.remove(&token);
                return Ok(());
            }
            _ => return Ok(()),
        }
    };
    let _ = tokio::io::copy_bidirectional(&mut twin, &mut stream).await;
    Ok(())
}

/// A control connection: a broadcaster that stays registered, or a spectator waiting
/// for approval. `first` is the message already read.
async fn control(hub: Arc<Hub>, stream: TcpStream, first: ClientMsg) -> std::io::Result<()> {
    let (mut read, mut write) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<ServerMsg>(32);

    let mut writer = tokio::spawn(async move {
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

    let mut conn = Conn::default();
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
        if let Err(e) = handle_control(&hub, &tx, &mut conn, msg).await {
            break Err(e);
        }
    };

    // Only remove entries that are still ours: a newer connection may own the name.
    {
        let mut bcs = hub.broadcasters.lock().await;
        if let Some(name) = &conn.name
            && bcs.get(name).is_some_and(|bc| bc.tx.same_channel(&tx))
        {
            bcs.remove(name);
        }
        if let Some((target, from)) = &conn.join
            && let Some(bc) = bcs.get_mut(target)
            && bc.pending.get(from).is_some_and(|p| p.same_channel(&tx))
        {
            bc.pending.remove(from);
        }
    }
    drop(tx);
    // Give queued replies a moment to flush, but never wait on a peer that won't read.
    if tokio::time::timeout(Duration::from_secs(5), &mut writer)
        .await
        .is_err()
    {
        writer.abort();
    }
    result
}

/// What a control connection has claimed, so it can be cleaned up when it ends.
#[derive(Default)]
struct Conn {
    /// The broadcaster name (lowercased) this connection registered.
    name: Option<String>,
    /// The (target, from) pending slot (both lowercased) this connection holds.
    join: Option<(String, String)>,
}

fn denied(reason: &str) -> ServerMsg {
    ServerMsg::Denied {
        reason: reason.into(),
    }
}

fn error(msg: &str) -> ServerMsg {
    ServerMsg::Error { msg: msg.into() }
}

// Every send below is `try_send` with no await: a peer that never reads loses its
// own messages instead of stalling the hub lock for everyone.
async fn handle_control(
    hub: &Arc<Hub>,
    tx: &mpsc::Sender<ServerMsg>,
    conn: &mut Conn,
    msg: ClientMsg,
) -> std::io::Result<()> {
    match msg {
        ClientMsg::Register { name, .. } => {
            if !valid_name(&name) {
                let _ = tx.try_send(error("invalid name"));
                return Ok(());
            }
            if conn.name.is_some() {
                let _ = tx.try_send(error("this connection is already registered"));
                return Ok(());
            }
            let k = key(&name);
            let mut bcs = hub.broadcasters.lock().await;
            // A closed channel means that connection's writer died; let the name go.
            if bcs.get(&k).is_some_and(|bc| !bc.tx.is_closed()) {
                let _ = tx.try_send(error("that name is already broadcasting"));
                return Ok(());
            }
            bcs.insert(
                k.clone(),
                Broadcaster {
                    tx: tx.clone(),
                    pending: HashMap::new(),
                },
            );
            drop(bcs);
            conn.name = Some(k);
            let _ = tx.try_send(ServerMsg::Registered);
        }
        ClientMsg::Join { target, from } => {
            if !valid_name(&target) || !valid_name(&from) {
                let _ = tx.try_send(denied("invalid name"));
                return Ok(());
            }
            if conn.join.is_some() {
                let _ = tx.try_send(denied("already waiting on a join request"));
                return Ok(());
            }
            let (t, f) = (key(&target), key(&from));
            let mut bcs = hub.broadcasters.lock().await;
            let Some(bc) = bcs.get_mut(&t) else {
                drop(bcs);
                let _ = tx.try_send(ServerMsg::Denied {
                    reason: format!("{target} isn't broadcasting right now"),
                });
                return Ok(());
            };
            if bc.pending.contains_key(&f) {
                drop(bcs);
                let _ = tx.try_send(denied("a join request from that name is already waiting"));
                return Ok(());
            }
            if bc.pending.len() >= MAX_PENDING {
                drop(bcs);
                let _ = tx.try_send(denied("too many pending join requests"));
                return Ok(());
            }
            if bc.tx.try_send(ServerMsg::JoinRequest { from }).is_err() {
                drop(bcs);
                let _ = tx.try_send(denied("the broadcaster isn't responding"));
                return Ok(());
            }
            bc.pending.insert(f.clone(), tx.clone());
            conn.join = Some((t, f));
        }
        ClientMsg::Accept { from } => {
            let Some(name) = conn.name.clone() else {
                return Ok(());
            };
            if !valid_name(&from) {
                return Ok(());
            }
            let spectator = {
                let mut bcs = hub.broadcasters.lock().await;
                bcs.get_mut(&name)
                    .and_then(|bc| bc.pending.remove(&key(&from)))
            };
            if let Some(spectator_tx) = spectator {
                let token = token()?;
                {
                    let mut relays = hub.relays.lock().await;
                    // Forget tokens nobody used; waiting sockets clean up themselves.
                    relays.retain(|_, s| match s {
                        RelaySlot::Issued(at) => at.elapsed() < RELAY_TTL,
                        RelaySlot::Waiting(_) => true,
                    });
                    relays.insert(token.clone(), RelaySlot::Issued(Instant::now()));
                }
                let _ = tx.try_send(ServerMsg::StartRelay {
                    token: token.clone(),
                    host: true,
                    peer: from.clone(),
                });
                let _ = spectator_tx.try_send(ServerMsg::StartRelay {
                    token,
                    host: false,
                    peer: name,
                });
            }
        }
        ClientMsg::Deny { from } => {
            let Some(name) = conn.name.clone() else {
                return Ok(());
            };
            if !valid_name(&from) {
                return Ok(());
            }
            let spectator = {
                let mut bcs = hub.broadcasters.lock().await;
                bcs.get_mut(&name)
                    .and_then(|bc| bc.pending.remove(&key(&from)))
            };
            if let Some(spectator_tx) = spectator {
                let _ = spectator_tx.try_send(denied("the broadcaster declined"));
            }
        }
        ClientMsg::Ping => {
            let _ = tx.try_send(ServerMsg::Pong);
        }
        ClientMsg::Relay { .. } => {
            let _ = tx.try_send(error("relay must be a fresh connection"));
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
