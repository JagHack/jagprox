//! Handshake, status and login on both sides of the proxy: as a server for the
//! player's client, and as a client towards Hypixel. HTTP calls to Mojang's session
//! server are left to the caller (passed in as closures).

use tokio::io::{AsyncRead, AsyncWrite};

use crate::crypto::{self, ServerKey};
use crate::error::{Error, Result};
use crate::frame::{FrameReader, FrameWriter};
use crate::packets::{Packet, handshake, login, status};

/// Both directions of one connection.
pub struct Connection<R, W> {
    pub reader: FrameReader<R>,
    pub writer: FrameWriter<W>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Connection<R, W> {
    pub fn new(read: R, write: W) -> Self {
        Self {
            reader: FrameReader::new(read),
            writer: FrameWriter::new(write),
        }
    }

    async fn read<P: Packet>(&mut self, state: &'static str) -> Result<P> {
        let frame = self.reader.read_frame().await?;
        let payload = frame.payload()?;
        let id = frame.id()?;
        if id != P::ID {
            return Err(Error::Unexpected { state, id });
        }
        P::from_payload(&payload)
    }

    async fn send<P: Packet>(&mut self, packet: &P) -> Result<()> {
        self.writer.send(&packet.to_payload()).await
    }

    fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.reader.enable_encryption(secret);
        self.writer.enable_encryption(secret);
    }
}

// ---------------------------------------------------------------------------
// Server side (the player's client connects to the proxy)
// ---------------------------------------------------------------------------

/// Reads the handshake every connection starts with.
pub async fn read_handshake<R, W>(conn: &mut Connection<R, W>) -> Result<handshake::Handshake>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    conn.read::<handshake::Handshake>("handshake").await
}

/// Answers a server-list ping: the status request, then the optional ping.
pub async fn serve_status<R, W>(conn: &mut Connection<R, W>, response_json: &str) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    conn.read::<status::Request>("status").await?;
    conn.send(&status::Response {
        json: response_json.to_string(),
    })
    .await?;
    match conn.read::<status::Ping>("status").await {
        Ok(ping) => {
            conn.send(&status::Pong {
                payload: ping.payload,
            })
            .await
        }
        Err(Error::Closed) => Ok(()),
        Err(e) => Err(e),
    }
}

pub async fn read_login_start<R, W>(conn: &mut Connection<R, W>) -> Result<String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    Ok(conn.read::<login::LoginStart>("login").await?.username)
}

/// Runs the encryption exchange with a connecting client and switches the
/// connection to encrypted. Returns the server hash for the session server's
/// `hasJoined` check.
pub async fn server_encrypt<R, W>(conn: &mut Connection<R, W>, key: &ServerKey) -> Result<String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let verify_token: [u8; 4] = crypto::random_bytes();
    conn.send(&login::EncryptionRequest {
        server_id: String::new(),
        public_key: key.public_der().to_vec(),
        verify_token: verify_token.to_vec(),
    })
    .await?;
    let response = conn.read::<login::EncryptionResponse>("login").await?;
    if key.decrypt(&response.verify_token)? != verify_token {
        return Err(Error::Crypto("verify token mismatch".into()));
    }
    let secret: [u8; 16] = key
        .decrypt(&response.shared_secret)?
        .try_into()
        .map_err(|_| Error::Crypto("shared secret is not 16 bytes".into()))?;
    conn.enable_encryption(&secret);
    Ok(crypto::server_hash("", &secret, key.public_der()))
}

/// Ends a login with a message shown on the client's disconnect screen.
pub async fn login_disconnect<R, W>(conn: &mut Connection<R, W>, reason_json: &str) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    conn.send(&login::Disconnect {
        reason: reason_json.to_string(),
    })
    .await
}

/// Tells the client to compress packets of at least `threshold` bytes, and does so.
pub async fn server_set_compression<R, W>(conn: &mut Connection<R, W>, threshold: i32) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    conn.send(&login::SetCompression { threshold }).await?;
    if threshold >= 0 {
        conn.reader.enable_compression();
        conn.writer.enable_compression(threshold as usize);
    }
    Ok(())
}

/// Completes the login; the connection is in the play state afterwards.
pub async fn server_login_success<R, W>(
    conn: &mut Connection<R, W>,
    uuid: &str,
    username: &str,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    conn.send(&login::LoginSuccess {
        uuid: uuid.to_string(),
        username: username.to_string(),
    })
    .await
}

// ---------------------------------------------------------------------------
// Client side (the proxy connects to Hypixel)
// ---------------------------------------------------------------------------

/// The result of logging in to a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientLogin {
    /// Dashed UUID the server assigned.
    pub uuid: String,
    pub username: String,
    /// Compression threshold the server asked for, if any.
    pub compression: Option<i32>,
}

/// Logs in to a server. `join` is called with the server hash when the server is in
/// online mode; it must tell Mojang's session server that we're joining (`/join`).
pub async fn client_login<R, W, F, Fut>(
    conn: &mut Connection<R, W>,
    host: &str,
    port: u16,
    username: &str,
    join: F,
) -> Result<ClientLogin>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = std::result::Result<(), String>>,
{
    conn.send(&handshake::Handshake {
        protocol_version: crate::PROTOCOL_VERSION,
        server_address: host.to_string(),
        server_port: port,
        next_state: handshake::LOGIN,
    })
    .await?;
    conn.send(&login::LoginStart {
        username: username.to_string(),
    })
    .await?;

    let mut join = Some(join);
    let mut compression = None;
    loop {
        let frame = conn.reader.read_frame().await?;
        let payload = frame.payload()?;
        match frame.id()? {
            login::Disconnect::ID => {
                return Err(Error::Disconnected(
                    login::Disconnect::from_payload(&payload)?.reason,
                ));
            }
            login::EncryptionRequest::ID => {
                let request = login::EncryptionRequest::from_payload(&payload)?;
                let secret: [u8; 16] = crypto::random_bytes();
                let hash = crypto::server_hash(&request.server_id, &secret, &request.public_key);
                let join = join.take().ok_or(Error::Unexpected {
                    state: "login",
                    id: login::EncryptionRequest::ID,
                })?;
                join(hash).await.map_err(Error::Auth)?;
                conn.send(&login::EncryptionResponse {
                    shared_secret: crypto::encrypt_with_public_der(&request.public_key, &secret)?,
                    verify_token: crypto::encrypt_with_public_der(
                        &request.public_key,
                        &request.verify_token,
                    )?,
                })
                .await?;
                conn.enable_encryption(&secret);
            }
            login::SetCompression::ID => {
                let threshold = login::SetCompression::from_payload(&payload)?.threshold;
                if threshold >= 0 {
                    conn.reader.enable_compression();
                    conn.writer.enable_compression(threshold as usize);
                    compression = Some(threshold);
                }
            }
            login::LoginSuccess::ID => {
                let success = login::LoginSuccess::from_payload(&payload)?;
                return Ok(ClientLogin {
                    uuid: success.uuid,
                    username: success.username,
                    compression,
                });
            }
            id => return Err(Error::Unexpected { state: "login", id }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::clientbound;

    /// The proxy's two halves talking to each other over a real socket: client login
    /// with encryption and compression against the server side.
    #[tokio::test]
    async fn client_and_server_logins_interoperate() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let key = ServerKey::generate().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (r, w) = stream.into_split();
            let mut conn = Connection::new(r, w);
            let hs = read_handshake(&mut conn).await.unwrap();
            assert_eq!((hs.protocol_version, hs.next_state), (47, handshake::LOGIN));
            let name = read_login_start(&mut conn).await.unwrap();
            let hash = server_encrypt(&mut conn, &key).await.unwrap();
            server_set_compression(&mut conn, 64).await.unwrap();
            server_login_success(&mut conn, "069a79f4-44e9-4726-a5be-fca90e38aaf5", &name)
                .await
                .unwrap();
            let chat = clientbound::Chat {
                message: "x".repeat(500),
                position: 0,
            };
            conn.writer.send(&chat.to_payload()).await.unwrap();
            hash
        });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (r, w) = stream.into_split();
        let mut conn = Connection::new(r, w);
        let joined = std::sync::Arc::new(std::sync::Mutex::new(None));
        let joined2 = joined.clone();
        let result = client_login(
            &mut conn,
            "localhost",
            addr.port(),
            "Steve",
            |hash| async move {
                *joined2.lock().unwrap() = Some(hash);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(result.username, "Steve");
        assert_eq!(result.compression, Some(64));

        let frame = conn.reader.read_frame().await.unwrap();
        let chat = clientbound::Chat::from_payload(&frame.payload().unwrap()).unwrap();
        assert_eq!(chat.message.len(), 500);

        let server_hash = server.await.unwrap();
        assert_eq!(
            joined.lock().unwrap().as_deref(),
            Some(server_hash.as_str()),
            "both sides computed the same hash"
        );
    }

    #[tokio::test]
    async fn client_login_reports_a_kick() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (r, w) = stream.into_split();
            let mut conn = Connection::new(r, w);
            read_handshake(&mut conn).await.unwrap();
            read_login_start(&mut conn).await.unwrap();
            login_disconnect(&mut conn, r#"{"text":"You are banned"}"#)
                .await
                .unwrap();
        });
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (r, w) = stream.into_split();
        let mut conn = Connection::new(r, w);
        let err = client_login(&mut conn, "localhost", addr.port(), "Steve", |_| async {
            Ok(())
        })
        .await
        .unwrap_err();
        assert!(
            matches!(err, Error::Disconnected(ref r) if r.contains("banned")),
            "{err}"
        );
    }
}
