//! Client for the JagProx broadcasting rendezvous server ("compass"). Mirrors the
//! server's newline-delimited JSON protocol (see `crates/compass/src/protocol.rs`).

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

/// A message from a JagProx proxy to the rendezvous server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    Register { name: String, uuid: String },
    Join { target: String, from: String },
    Accept { from: String },
    Deny { from: String },
    Relay { token: String },
    Ping,
}

/// A message from the rendezvous server to a JagProx proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Registered,
    JoinRequest {
        from: String,
    },
    StartRelay {
        token: String,
        host: bool,
        peer: String,
    },
    Denied {
        reason: String,
    },
    Error {
        msg: String,
    },
    Pong,
}

/// Connects to the rendezvous server (`host:port`).
pub async fn connect(addr: &str) -> std::io::Result<TcpStream> {
    let stream = TcpStream::connect(addr).await?;
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

/// Writes one control message as a JSON line.
pub async fn write_msg(write: &mut OwnedWriteHalf, msg: &ClientMsg) -> std::io::Result<()> {
    let mut line =
        serde_json::to_vec(msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    line.push(b'\n');
    write.write_all(&line).await?;
    write.flush().await
}

/// Reads one `\n`-delimited server message, buffering any extra bytes in `buf`.
pub async fn read_msg(read: &mut OwnedReadHalf, buf: &mut Vec<u8>) -> std::io::Result<ServerMsg> {
    loop {
        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let text = &line[..line.len() - 1];
            return serde_json::from_slice(text)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e));
        }
        let mut chunk = [0u8; 1024];
        let n = read.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "rendezvous server closed the connection",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Opens a fresh data connection and claims a relay token; the returned stream is a
/// raw pipe to the peer, ready to carry the Minecraft protocol.
pub async fn open_relay(addr: &str, token: &str) -> std::io::Result<TcpStream> {
    let mut stream = connect(addr).await?;
    let msg = ClientMsg::Relay {
        token: token.to_string(),
    };
    let mut line =
        serde_json::to_vec(&msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    line.push(b'\n');
    stream.write_all(&line).await?;
    stream.flush().await?;
    Ok(stream)
}
