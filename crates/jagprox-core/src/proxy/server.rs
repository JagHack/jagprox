//! Accepting players: server-list pings, logins (online-mode check, Microsoft account
//! for the Hypixel side, Hypixel login) and handing over to the session relay.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use mcproto::conn::{self, Connection};
use mcproto::crypto::ServerKey;
use mcproto::packets::{handshake, parse_uuid, uuid_string};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

use super::ProxyEvent;
use super::session;
use crate::config::Config;
use crate::log::log;
use crate::msauth::{self, Accounts, AuthError};

const ICON: &[u8] = include_bytes!("../../../../server-icon.png");

fn motd() -> String {
    format!(
        "{}§8[§5jag§dprox§8] §d1.8-1.21\n{}§5§lHypixel Proxy §8- §7by §dJagHack",
        " ".repeat(20),
        " ".repeat(13)
    )
}

fn text_json(text: &str) -> String {
    json!({ "text": text }).to_string()
}

type ClientConn = Connection<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf>;

/// Ends a login with `reason` on the player's disconnect screen.
async fn kick(client: &mut ClientConn, reason: String) -> Result<(), String> {
    let _ = conn::login_disconnect(client, &text_json(&reason)).await;
    Err(reason)
}

pub struct ProxyOptions {
    pub data_dir: PathBuf,
    pub config: Config,
    /// JagProx login (JWT), for API lookups and gametrack.
    pub jwt: Option<String>,
    pub events: broadcast::Sender<ProxyEvent>,
}

/// A running proxy's shared state.
pub struct Proxy {
    pub(crate) options: ProxyOptions,
    key: ServerKey,
    accounts: Accounts,
    status_json: String,
    /// Username of the connected player; one player per proxy.
    active: Mutex<Option<String>>,
    /// A Microsoft sign-in waiting for its code: (Minecraft name, code, link).
    pending_sign_in: Mutex<Option<(String, String, String)>>,
}

impl Proxy {
    pub fn new(options: ProxyOptions) -> Result<Arc<Self>, String> {
        let key = ServerKey::generate().map_err(|e| e.to_string())?;
        let favicon = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(ICON)
        );
        let status_json = json!({
            "version": { "name": mcproto::VERSION_NAME, "protocol": mcproto::PROTOCOL_VERSION },
            "players": { "max": 1, "online": 0, "sample": [] },
            "description": { "text": motd() },
            "favicon": favicon,
        })
        .to_string();
        let accounts = Accounts::new(&options.data_dir);
        Ok(Arc::new(Self {
            options,
            key,
            accounts,
            status_json,
            active: Mutex::new(None),
            pending_sign_in: Mutex::new(None),
        }))
    }

    pub fn events(&self) -> &broadcast::Sender<ProxyEvent> {
        &self.options.events
    }

    pub fn config(&self) -> &Config {
        &self.options.config
    }

    fn emit(&self, event: ProxyEvent) {
        let _ = self.options.events.send(event);
    }

    /// Listens until the task is cancelled. Fails if the port can't be opened.
    pub async fn run(self: Arc<Self>) -> Result<(), String> {
        let host = self.config().host();
        let port = self.config().port();
        let listener = TcpListener::bind((host.as_str(), port))
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    format!("port {port} is already in use by another application")
                } else {
                    format!("could not listen on {host}:{port}: {e}")
                }
            })?;
        log(format!("JagProx server started on {host}:{port}."));
        self.emit(ProxyEvent::Started { host, port });
        loop {
            let (stream, addr) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => {
                    log(format!("Proxy server error: {e}"));
                    continue;
                }
            };
            let proxy = self.clone();
            tokio::spawn(async move {
                if let Err(e) = proxy.handle(stream).await {
                    crate::log::debug(format!("Connection from {addr} ended: {e}"));
                }
            });
        }
    }

    async fn handle(self: Arc<Self>, stream: TcpStream) -> Result<(), String> {
        let _ = stream.set_nodelay(true);
        let (r, w) = stream.into_split();
        let mut client = Connection::new(r, w);
        let hs = tokio::time::timeout(Duration::from_secs(10), conn::read_handshake(&mut client))
            .await
            .map_err(|_| "handshake timed out".to_string())?
            .map_err(|e| e.to_string())?;
        match hs.next_state {
            handshake::STATUS => {
                let status = self.status_json.replace(
                    r#""online":0"#,
                    &format!(r#""online":{}"#, self.player_count()),
                );
                conn::serve_status(&mut client, &status)
                    .await
                    .map_err(|e| e.to_string())
            }
            handshake::LOGIN => self.login(client, hs.protocol_version).await,
            other => Err(format!("unknown next state {other}")),
        }
    }

    fn player_count(&self) -> usize {
        usize::from(
            self.active
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some(),
        )
    }

    async fn login(
        self: Arc<Self>,
        mut client: Connection<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf>,
        protocol: i32,
    ) -> Result<(), String> {
        let name = conn::read_login_start(&mut client)
            .await
            .map_err(|e| e.to_string())?;
        if protocol != mcproto::PROTOCOL_VERSION {
            let reason = if protocol < mcproto::PROTOCOL_VERSION {
                "Outdated client! Please use 1.8.9"
            } else {
                "Outdated server! I'm still on 1.8.9"
            };
            return kick(&mut client, reason.into()).await;
        }
        let already_connected = {
            let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
            let taken = active.is_some();
            if !taken {
                *active = Some(name.clone());
            }
            taken
        };
        if already_connected {
            return kick(
                &mut client,
                "Another player is already connected to this proxy.".into(),
            )
            .await;
        }
        let result = self.clone().login_inner(client, name.clone()).await;
        *self.active.lock().unwrap_or_else(|e| e.into_inner()) = None;
        result
    }

    async fn login_inner(
        self: Arc<Self>,
        mut client: Connection<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf>,
        name: String,
    ) -> Result<(), String> {
        macro_rules! kick {
            ($reason:expr) => {{
                let reason: String = $reason;
                let _ = conn::login_disconnect(&mut client, &text_json(&reason)).await;
                return Err(reason);
            }};
        }
        let offline = self.config().offline_mode();

        // 1. The player proves they own the name (online mode).
        let (uuid, name) = if offline {
            (mcproto::crypto::offline_uuid(&name), name)
        } else {
            let hash = match conn::server_encrypt(&mut client, &self.key).await {
                Ok(hash) => hash,
                Err(e) => return Err(e.to_string()),
            };
            match msauth::has_joined(&name, &hash).await {
                Ok(Some(profile)) => (parse_uuid(&profile.uuid).unwrap_or_default(), profile.name),
                Ok(None) => kick!("Failed to verify username!".into()),
                Err(e) => kick!(format!("Could not verify your account with Mojang: {e}")),
            }
        };
        log(format!("Client connected to proxy: {name}"));

        // 2. The Microsoft account for the Hypixel side.
        let session = if offline {
            None
        } else {
            match self.accounts.session_for(&name).await {
                Ok(session) => Some(session),
                Err(AuthError::NoAccount(_)) => {
                    let (code, link) = self.start_sign_in(&name).await;
                    kick!(format!(
                        "§dJagProx §7needs you to sign in to Microsoft as §f{name}§7.\n\n§7Go to §b{link}\n§7and enter the code §f§l{code}\n\n§7Then reconnect."
                    ))
                }
                Err(e) => kick!(format!("§cMicrosoft sign-in failed: {e}")),
            }
        };

        // 3. Log in to Hypixel.
        let (host, port) = self.config().upstream();
        let upstream = match tokio::time::timeout(
            Duration::from_secs(10),
            TcpStream::connect((host.as_str(), port)),
        )
        .await
        {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => kick!(format!("§cCould not reach {host}: {e}")),
            Err(_) => kick!(format!("§cCould not reach {host}: timed out")),
        };
        let _ = upstream.set_nodelay(true);
        let (ur, uw) = upstream.into_split();
        let mut server = Connection::new(ur, uw);
        let login_name = session.as_ref().map_or(name.clone(), |s| s.name.clone());
        let join_session = session.clone();
        let login = conn::client_login(&mut server, &host, port, &login_name, |hash| async move {
            match join_session {
                Some(s) => msauth::session_join(&s, &hash)
                    .await
                    .map_err(|e| e.to_string()),
                None => Err(
                    "this server needs a Microsoft account, but the proxy is in offline mode"
                        .into(),
                ),
            }
        })
        .await;
        let login = match login {
            Ok(login) => login,
            Err(mcproto::Error::Disconnected(reason)) => {
                let _ = conn::login_disconnect(&mut client, &reason).await;
                return Err(format!("Hypixel refused the login: {reason}"));
            }
            Err(e) => kick!(format!("§cCould not log in to {host}: {e}")),
        };
        log(format!("Client connected to target: {}", login.username));

        // 4. Finish the player's login with the same compression as Hypixel, so most
        // packets pass through without being re-compressed.
        if let Some(threshold) = login.compression
            && let Err(e) = conn::server_set_compression(&mut client, threshold).await
        {
            return Err(e.to_string());
        }
        let client_uuid = if offline {
            uuid_string(uuid)
        } else {
            login.uuid.clone()
        };
        conn::server_login_success(&mut client, &client_uuid, &login.username)
            .await
            .map_err(|e| e.to_string())?;

        self.emit(ProxyEvent::Playing(true));
        let result = session::run(
            self.clone(),
            client,
            server,
            login.username.clone(),
            client_uuid,
            session,
        )
        .await;
        log(format!("Client disconnected: {}", login.username));
        self.emit(ProxyEvent::Playing(false));
        result
    }

    /// Starts (or reuses) a Microsoft sign-in for `name` and returns (code, link).
    async fn start_sign_in(self: &Arc<Self>, name: &str) -> (String, String) {
        if let Some((pending_name, code, link)) = self
            .pending_sign_in
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            && pending_name.eq_ignore_ascii_case(name)
        {
            return (code, link);
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let proxy = self.clone();
        let wanted = name.to_string();
        tokio::spawn(async move {
            let mut tx = Some(tx);
            let result = proxy
                .accounts
                .sign_in(|code| {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send((code.user_code.clone(), code.link()));
                    }
                })
                .await;
            *proxy
                .pending_sign_in
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            match result {
                Ok(session) if session.name.eq_ignore_ascii_case(&wanted) => {
                    log(format!("Signed in to Microsoft as {}.", session.name));
                    proxy.emit(ProxyEvent::SignedIn(session.name));
                }
                Ok(session) => {
                    log(format!(
                        "Signed in to Microsoft as {}, but you connected as {wanted}. Connect with the account you signed in with.",
                        session.name
                    ));
                    proxy.emit(ProxyEvent::SignedIn(session.name));
                }
                Err(e) => log(format!("[ERROR] Microsoft sign-in failed: {e}")),
            }
        });
        match rx.await {
            Ok((code, link)) => {
                log(format!(
                    "Sign in to Microsoft for {name}: open {link} (code {code})"
                ));
                *self
                    .pending_sign_in
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) =
                    Some((name.to_string(), code.clone(), link.clone()));
                self.emit(ProxyEvent::SignIn {
                    name: name.to_string(),
                    code: code.clone(),
                    link: link.clone(),
                });
                (code, link)
            }
            Err(_) => (
                "(unavailable)".into(),
                "https://www.microsoft.com/link".into(),
            ),
        }
    }
}
