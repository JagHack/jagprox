//! End-to-end test of the rendezvous flow: register, join, accept, and a piped relay.

use jagprox_compass::protocol::{ClientMsg, ServerMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn send(stream: &mut TcpStream, msg: &ClientMsg) {
    let mut line = serde_json::to_vec(msg).unwrap();
    line.push(b'\n');
    stream.write_all(&line).await.unwrap();
}

async fn recv(stream: &mut TcpStream) -> ServerMsg {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await.unwrap();
        assert_ne!(n, 0, "connection closed before a message arrived");
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
    }
    serde_json::from_slice(&line).unwrap()
}

#[tokio::test]
async fn register_join_accept_and_relay() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(jagprox_compass::serve(listener));

    // Broadcaster registers and keeps its control connection.
    let mut broadcaster = TcpStream::connect(addr).await.unwrap();
    send(
        &mut broadcaster,
        &ClientMsg::Register {
            name: "Host".into(),
            uuid: "u".into(),
        },
    )
    .await;
    assert!(matches!(recv(&mut broadcaster).await, ServerMsg::Registered));

    // Spectator asks to join.
    let mut spectator = TcpStream::connect(addr).await.unwrap();
    send(
        &mut spectator,
        &ClientMsg::Join {
            target: "host".into(), // case-insensitive
            from: "Watcher".into(),
        },
    )
    .await;

    // Broadcaster sees the request and accepts.
    match recv(&mut broadcaster).await {
        ServerMsg::JoinRequest { from } => assert_eq!(from, "Watcher"),
        other => panic!("expected JoinRequest, got {other:?}"),
    }
    send(
        &mut broadcaster,
        &ClientMsg::Accept {
            from: "Watcher".into(),
        },
    )
    .await;

    // Both get StartRelay with the same token; host flag distinguishes them.
    let (host_token, host_flag) = match recv(&mut broadcaster).await {
        ServerMsg::StartRelay { token, host, .. } => (token, host),
        other => panic!("broadcaster expected StartRelay, got {other:?}"),
    };
    let (spec_token, spec_flag) = match recv(&mut spectator).await {
        ServerMsg::StartRelay { token, host, .. } => (token, host),
        other => panic!("spectator expected StartRelay, got {other:?}"),
    };
    assert_eq!(host_token, spec_token, "same pairing token");
    assert!(host_flag && !spec_flag, "broadcaster is the host");

    // Each opens a data connection with the token; the server pipes them.
    let mut host_data = TcpStream::connect(addr).await.unwrap();
    send(&mut host_data, &ClientMsg::Relay { token: host_token }).await;
    let mut spec_data = TcpStream::connect(addr).await.unwrap();
    send(&mut spec_data, &ClientMsg::Relay { token: spec_token }).await;

    // Bytes written on one side arrive on the other, both directions.
    host_data.write_all(b"from-host").await.unwrap();
    let mut buf = [0u8; 9];
    spec_data.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"from-host");

    spec_data.write_all(b"from-spec").await.unwrap();
    host_data.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"from-spec");
}

#[tokio::test]
async fn join_offline_broadcaster_is_denied() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(jagprox_compass::serve(listener));

    let mut spectator = TcpStream::connect(addr).await.unwrap();
    send(
        &mut spectator,
        &ClientMsg::Join {
            target: "Nobody".into(),
            from: "Watcher".into(),
        },
    )
    .await;
    assert!(matches!(
        recv(&mut spectator).await,
        ServerMsg::Denied { .. }
    ));
}
