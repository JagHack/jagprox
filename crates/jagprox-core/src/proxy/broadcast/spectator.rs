//! The spectator side: bridge a freshly-logged-in Minecraft client onto a
//! broadcaster's feed over the relay, instead of to Hypixel.
//!
//! The client has already completed login with our proxy (online-mode verified).
//! We dial the rendezvous server, ask to join the target, and on approval open a
//! relay and log in to the broadcaster's host as a client. The host answers with
//! compression + login success + the world; we finish our own client's login with
//! the same compression and then forward frames both ways. The two sides share the
//! same compression setting, so frames pass through without being re-encoded.

use mcproto::conn::{self, Connection};
use mcproto::frame::{FrameReader, FrameWriter};
use serde_json::json;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use super::compass::{self, ClientMsg, ServerMsg};
use crate::log::log;

type ClientConn = Connection<OwnedReadHalf, OwnedWriteHalf>;

/// Bridges `client` to `target`'s broadcast. `client` is already past login on our
/// side. On failure the client is kicked with `reason`.
pub async fn run(
    mut client: ClientConn,
    name: String,
    client_uuid: String,
    target: String,
    compass_addr: String,
) -> Result<(), String> {
    macro_rules! kick {
        ($reason:expr) => {{
            let reason: String = $reason;
            let _ = conn::login_disconnect(&mut client, &json!({ "text": reason }).to_string()).await;
            return Err(reason);
        }};
    }

    // Ask the rendezvous server to join the target, and wait for approval.
    let token = match request_join(&compass_addr, &target, &name).await {
        Ok(token) => token,
        Err(e) => kick!(format!("§cCouldn't start spectating {target}: {e}")),
    };

    let relay = match compass::open_relay(&compass_addr, &token).await {
        Ok(r) => r,
        Err(e) => kick!(format!("§cCouldn't connect to {target}'s broadcast: {e}")),
    };
    let (rr, rw) = relay.into_split();
    let mut host = Connection::new(rr, rw);

    // Log in to the broadcaster's host as a client (offline: no encryption).
    let login = conn::client_login(&mut host, "broadcast", 0, &name, |_hash| async {
        Err("the broadcast host unexpectedly asked for encryption".to_string())
    })
    .await;
    let login = match login {
        Ok(l) => l,
        Err(mcproto::Error::Disconnected(reason)) => kick!(reason),
        Err(e) => kick!(format!("§cBroadcast login failed: {e}")),
    };

    // Finish our own client's login with the same compression as the host, then the
    // two sides match and frames forward byte-for-byte.
    if let Some(threshold) = login.compression
        && let Err(e) = conn::server_set_compression(&mut client, threshold).await
    {
        kick!(format!("§cBroadcast setup failed: {e}"));
    }
    if let Err(e) = conn::server_login_success(&mut client, &client_uuid, &name).await {
        kick!(format!("§cBroadcast setup failed: {e}"));
    }
    log(format!("{name} is now spectating {target}."));

    // Forward frames both ways until either side closes.
    let Connection {
        reader: client_r,
        writer: client_w,
    } = client;
    let Connection {
        reader: host_r,
        writer: host_w,
    } = host;
    let down = tokio::spawn(forward(host_r, client_w));
    let up = tokio::spawn(forward(client_r, host_w));
    tokio::select! {
        _ = down => {}
        _ = up => {}
    }
    log(format!("{name} stopped spectating {target}."));
    Ok(())
}

/// Connects to the rendezvous server, requests to join `target`, and returns the
/// relay token once the broadcaster approves.
async fn request_join(addr: &str, target: &str, from: &str) -> Result<String, String> {
    let stream = compass::connect(addr).await.map_err(|e| e.to_string())?;
    let (mut read, mut write) = stream.into_split();
    compass::write_msg(
        &mut write,
        &ClientMsg::Join {
            target: target.to_string(),
            from: from.to_string(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    loop {
        match compass::read_msg(&mut read, &mut buf).await.map_err(|e| e.to_string())? {
            ServerMsg::StartRelay { token, host: false, .. } => return Ok(token),
            ServerMsg::StartRelay { host: true, .. } => {}
            ServerMsg::Denied { reason } => return Err(reason),
            ServerMsg::Error { msg } => return Err(msg),
            _ => continue,
        }
    }
}

/// Forwards raw frames from one side to the other until the source closes.
async fn forward(mut from: FrameReader<OwnedReadHalf>, mut to: FrameWriter<OwnedWriteHalf>) {
    loop {
        match from.read_frame().await {
            Ok(frame) => {
                if to.queue_frame(&frame).is_err() || to.flush().await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
