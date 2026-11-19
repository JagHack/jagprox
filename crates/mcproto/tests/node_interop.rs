//! Logins against node-minecraft-protocol, the library the JS proxy used, over real
//! sockets. Skipped (with a note) when `node` or the repo's node_modules are missing.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use mcproto::conn::{self, Connection};
use mcproto::crypto::ServerKey;
use mcproto::packets::{Packet, clientbound, handshake, serverbound};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn node_available() -> bool {
    let has_node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let has_module = repo_root().join("node_modules/minecraft-protocol").exists();
    if !(has_node && has_module) {
        eprintln!("skipping: needs `node` and `npm install` in the repo root");
    }
    has_node && has_module
}

fn node(script: &str) -> Command {
    let mut cmd = Command::new("node");
    cmd.arg("-e")
        .arg(script)
        .current_dir(repo_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd
}

/// node-minecraft-protocol's client logs in to our server side, which uses
/// encryption (as in online mode) and compression, then receives a chat message.
#[tokio::test]
async fn node_client_logs_in_to_rust_server() {
    if !node_available() {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let script = format!(
        r#"
        const mc = require('minecraft-protocol');
        const c = mc.createClient({{ host: '127.0.0.1', port: {port}, username: 'NodeBot', version: '1.8.9', auth: 'offline' }});
        c.on('chat', p => {{ console.log('CHAT ' + p.message); c.write('chat', {{ message: '/hello ' + 'x'.repeat(90) }}); }});
        c.on('error', e => {{ console.log('ERR ' + e.message); process.exit(1); }});
        c.on('end', () => process.exit(0));
        setTimeout(() => {{ console.log('TIMEOUT'); process.exit(2); }}, 15000);
        "#
    );
    let mut child = node(&script).spawn().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();

    let (stream, _) = tokio::time::timeout(Duration::from_secs(15), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let (r, w) = stream.into_split();
    let mut conn = Connection::new(r, w);
    let hs = conn::read_handshake(&mut conn).await.unwrap();
    assert_eq!((hs.protocol_version, hs.next_state), (47, handshake::LOGIN));
    let name = conn::read_login_start(&mut conn).await.unwrap();
    assert_eq!(name, "NodeBot");
    let key = ServerKey::generate().unwrap();
    conn::server_encrypt(&mut conn, &key).await.unwrap();
    conn::server_set_compression(&mut conn, 256).await.unwrap();
    conn::server_login_success(&mut conn, "069a79f4-44e9-4726-a5be-fca90e38aaf5", &name)
        .await
        .unwrap();

    let join = clientbound::JoinGame {
        entity_id: 1,
        game_mode: 0,
        dimension: 0,
        difficulty: 1,
        max_players: 20,
        level_type: "default".into(),
        reduced_debug_info: false,
    };
    // Long enough to be compressed.
    let message = format!(r#"{{"text":"hi {}"}}"#, "✫".repeat(200));
    conn.writer.queue_packet(&join.to_payload()).unwrap();
    conn.writer
        .queue_packet(
            &clientbound::Chat {
                message: message.clone(),
                position: 0,
            }
            .to_payload(),
        )
        .unwrap();
    conn.writer.flush().await.unwrap();

    let line = tokio::time::timeout(Duration::from_secs(15), stdout.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        line,
        format!("CHAT {message}"),
        "node decoded our encrypted, compressed chat"
    );

    // And we decode what node sends back (encrypted, compressed setting active).
    let chat = loop {
        let frame = tokio::time::timeout(Duration::from_secs(15), conn.reader.read_frame())
            .await
            .unwrap()
            .unwrap();
        if frame.id().unwrap() == serverbound::Chat::ID {
            break serverbound::Chat::from_payload(&frame.payload().unwrap()).unwrap();
        }
    };
    assert_eq!(chat.message, format!("/hello {}", "x".repeat(90)));
    let _ = child.kill().await;
}

/// Our client side logs in to node-minecraft-protocol's server (offline mode, with
/// its default compression) and reads the join and chat packets it sends.
#[tokio::test]
async fn rust_client_logs_in_to_node_server() {
    if !node_available() {
        return;
    }
    let script = r#"
        const mc = require('minecraft-protocol');
        const server = mc.createServer({ 'online-mode': false, host: '127.0.0.1', port: 0, version: '1.8.9' });
        server.on('listening', () => console.log('PORT ' + server.socketServer.address().port));
        server.on('login', client => {
            client.write('login', { entityId: 7, gameMode: 1, dimension: 0, difficulty: 2, maxPlayers: 50, levelType: 'default', reducedDebugInfo: false });
            client.write('chat', { message: JSON.stringify({ text: 'welcome ' + 'y'.repeat(400) }), position: 0 });
            client.on('chat', p => console.log('GOT ' + p.message));
        });
        setTimeout(() => process.exit(2), 15000);
    "#;
    let mut child = node(script).spawn().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(15), stdout.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let port: u16 = line
        .strip_prefix("PORT ")
        .expect("node printed its port")
        .parse()
        .unwrap();

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let (r, w) = stream.into_split();
    let mut conn = Connection::new(r, w);
    let login = conn::client_login(&mut conn, "127.0.0.1", port, "RustBot", |_| async {
        Err("offline server must not ask for encryption".to_string())
    })
    .await
    .unwrap();
    assert_eq!(login.username, "RustBot");
    assert_eq!(login.compression, Some(256));

    let mut saw_join = false;
    let chat = loop {
        let frame = tokio::time::timeout(Duration::from_secs(15), conn.reader.read_frame())
            .await
            .unwrap()
            .unwrap();
        let payload = frame.payload().unwrap();
        match frame.id().unwrap() {
            clientbound::JoinGame::ID => {
                let join = clientbound::JoinGame::from_payload(&payload).unwrap();
                assert_eq!((join.entity_id, join.max_players), (7, 50));
                saw_join = true;
            }
            clientbound::Chat::ID => break clientbound::Chat::from_payload(&payload).unwrap(),
            _ => {} // keep-alives etc.
        }
    };
    assert!(saw_join);
    assert!(chat.message.contains(&"y".repeat(400)));

    conn.writer
        .send(
            &serverbound::Chat {
                message: "hello from rust".into(),
            }
            .to_payload(),
        )
        .await
        .unwrap();
    let line = tokio::time::timeout(Duration::from_secs(15), stdout.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(line, "GOT hello from rust");
    let _ = child.kill().await;
}
