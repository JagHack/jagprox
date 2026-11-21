//! The whole proxy end to end without Hypixel: a node-minecraft-protocol client
//! connects through the Rust proxy to a node-minecraft-protocol server (offline mode,
//! localhost). Skipped when `node` or the repo's node_modules are missing.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use jagprox_core::config::Config;
use jagprox_core::proxy::{Proxy, ProxyEvent, ProxyOptions};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{ChildStdout, Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn node_available() -> bool {
    let has_node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let ok = has_node && repo_root().join("node_modules/minecraft-protocol").exists();
    if !ok {
        eprintln!("skipping: needs `node` and `npm install` in the repo root");
    }
    ok
}

fn node(script: &str) -> (tokio::process::Child, Lines<BufReader<ChildStdout>>) {
    let mut child = Command::new("node")
        .arg("-e")
        .arg(script)
        .current_dir(repo_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap()).lines();
    (child, lines)
}

async fn expect_line(lines: &mut Lines<BufReader<ChildStdout>>, who: &str) -> String {
    tokio::time::timeout(Duration::from_secs(20), lines.next_line())
        .await
        .unwrap_or_else(|_| panic!("{who} timed out"))
        .unwrap()
        .unwrap_or_else(|| panic!("{who} exited"))
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test(flavor = "multi_thread")]
async fn node_client_plays_through_the_rust_proxy() {
    if !node_available() {
        return;
    }

    // The "Hypixel" side.
    let (_server, mut server_out) = node(
        r#"
        const mc = require('minecraft-protocol');
        const server = mc.createServer({ 'online-mode': false, host: '127.0.0.1', port: 0, version: '1.8.9' });
        server.on('listening', () => console.log('PORT ' + server.socketServer.address().port));
        server.on('login', client => {
            console.log('LOGIN ' + client.username);
            client.write('login', { entityId: 7, gameMode: 1, dimension: 0, difficulty: 2, maxPlayers: 50, levelType: 'default', reducedDebugInfo: false });
            client.write('custom_payload', { channel: 'MC|Brand', data: Buffer.from('\x12Hypixel BungeeCord') });
            client.write('chat', { message: JSON.stringify({ text: 'Welcome ' + '§a✫'.repeat(300) }), position: 0 });
            client.on('chat', p => console.log('SERVER GOT CHAT ' + p.message));
            client.on('custom_payload', p => console.log('SERVER GOT BRAND ' + p.channel + ' ' + JSON.stringify(p.data.toString('latin1'))));
        });
        setTimeout(() => process.exit(2), 30000);
        "#,
    );
    let line = expect_line(&mut server_out, "server").await;
    let upstream_port: u16 = line.strip_prefix("PORT ").unwrap().parse().unwrap();

    // The proxy.
    let proxy_port = free_port();
    let data_dir = std::env::temp_dir().join(format!("jagprox-e2e-{}", std::process::id()));
    let config = Config::in_memory(json!({
        "host": "127.0.0.1",
        "port": proxy_port,
        "upstream_host": "127.0.0.1",
        "upstream_port": upstream_port,
        "offline_mode": true,
    }));
    let (events, mut event_rx) = tokio::sync::broadcast::channel(64);
    let proxy = Proxy::new(ProxyOptions {
        data_dir,
        config,
        jwt: None,
        events,
    })
    .unwrap();
    tokio::spawn(proxy.run());
    assert!(matches!(
        event_rx.recv().await.unwrap(),
        ProxyEvent::Started { .. }
    ));

    // The player's client.
    let (_client, mut client_out) = node(&format!(
        r#"
        const mc = require('minecraft-protocol');
        const c = mc.createClient({{ host: '127.0.0.1', port: {proxy_port}, username: 'NodeBot', version: '1.8.9', auth: 'offline' }});
        c.on('login', p => {{
            console.log('CLIENT JOINED ' + p.entityId);
            c.write('custom_payload', {{ channel: 'MC|Brand', data: Buffer.from('\x05lunar') }});
        }});
        c.on('custom_payload', p => {{ if (p.channel === 'MC|Brand') console.log('CLIENT GOT BRAND ' + JSON.stringify(p.data.toString('latin1'))); }});
        c.on('chat', p => {{ console.log('CLIENT GOT CHAT ' + p.message.length); c.write('chat', {{ message: 'hello through the proxy' }}); }});
        c.on('error', e => {{ console.log('CLIENT ERROR ' + e.message); }});
        setTimeout(() => process.exit(2), 30000);
        "#
    ));

    assert_eq!(
        expect_line(&mut server_out, "server").await,
        "LOGIN NodeBot"
    );
    assert_eq!(
        expect_line(&mut client_out, "client").await,
        "CLIENT JOINED 7"
    );
    assert_eq!(
        expect_line(&mut client_out, "client").await,
        r#"CLIENT GOT BRAND "\u0007vanilla""#,
        "server brand hidden"
    );
    let chat = expect_line(&mut client_out, "client").await;
    // JS string length counts UTF-16 units; every character here is in the BMP.
    let expected_len = json!({ "text": format!("Welcome {}", "§a✫".repeat(300)) })
        .to_string()
        .chars()
        .count();
    assert_eq!(
        chat,
        format!("CLIENT GOT CHAT {expected_len}"),
        "compressed chat forwarded intact"
    );

    let mut server_lines = vec![
        expect_line(&mut server_out, "server").await,
        expect_line(&mut server_out, "server").await,
    ];
    server_lines.sort();
    assert_eq!(
        server_lines,
        vec![
            r#"SERVER GOT BRAND MC|Brand "\u0007vanilla""#.to_string(),
            "SERVER GOT CHAT hello through the proxy".to_string()
        ],
        "client brand hidden, chat forwarded"
    );

    // The proxy reported the session and mirrored the chat line.
    let mut saw_playing = false;
    let mut saw_chat = false;
    while let Ok(Ok(event)) =
        tokio::time::timeout(Duration::from_millis(200), event_rx.recv()).await
    {
        match event {
            ProxyEvent::Playing(true) => saw_playing = true,
            ProxyEvent::Chat(text) => saw_chat |= text.starts_with("Welcome §a✫"),
            _ => {}
        }
    }
    assert!(saw_playing && saw_chat);
}

/// The modules and commands against a fake Hypixel that plays out a queue lobby.
#[tokio::test(flavor = "multi_thread")]
async fn modules_react_like_on_hypixel() {
    if !node_available() {
        return;
    }
    let (_server, mut server_out) = node(
        r#"
        const mc = require('minecraft-protocol');
        const server = mc.createServer({ 'online-mode': false, host: '127.0.0.1', port: 0, version: '1.8.9' });
        server.on('listening', () => console.log('PORT ' + server.socketServer.address().port));
        const chat = (c, text) => c.write('chat', { message: JSON.stringify({ text }), position: 0 });
        server.on('login', c => {
            c.write('login', { entityId: 1, gameMode: 0, dimension: 0, difficulty: 1, maxPlayers: 16, levelType: 'default', reducedDebugInfo: false });
            // Hidden opponents, in Hypixel's order: tab add, team, then removed from
            // the tab right away. The second one's team arrives before its tab entry.
            const hidden1 = '853c80ef-3c37-49fd-aa49-938b674adae6';
            const hidden2 = '069a79f4-44e9-4726-a5be-fca90e38aaf5';
            c.write('player_info', { action: 'add_player', data: [{ uuid: hidden1, name: '§kHidden1', properties: [], gamemode: 0, ping: 1 }] });
            c.write('scoreboard_team', { team: '§6[MVP§c++§6]', mode: 0, name: 'x', prefix: '§6[MVP§c++§6] ', suffix: '', friendlyFire: 0, nameTagVisibility: 'always', color: 6, players: ['§r§kHidden1'] });
            c.write('player_info', { action: 'remove_player', data: [{ uuid: hidden1 }] });
            c.write('scoreboard_team', { team: '§b', mode: 0, name: 'y', prefix: '§b', suffix: '', friendlyFire: 0, nameTagVisibility: 'always', color: 11, players: ['§r§kHidden2'] });
            c.write('player_info', { action: 'add_player', data: [{ uuid: hidden2, name: '§kHidden2', properties: [], gamemode: 0, ping: 1 }] });
            c.write('player_info', { action: 'remove_player', data: [{ uuid: hidden2 }] });
            // The game starts.
            c.write('scoreboard_objective', { name: 'sb', action: 0, displayText: '§e§lBED WARS', type: 'integer' });
            setTimeout(() => chat(c, 'Protect your bed and destroy the enemy beds.'), 200);
            let secondGame = false;
            c.on('chat', p => {
                console.log('SERVER GOT ' + p.message);
                // After game 1's gg: switch server and play a second Bed Wars game.
                if (p.message === '/ac gg' && !secondGame) {
                    secondGame = true;
                    c.write('respawn', { dimension: 1, difficulty: 1, gamemode: 0, levelType: 'default' });
                    c.write('playerlist_header', { header: '{"text":"Hypixel"}', footer: '{"text":"store.hypixel.net"}' });
                    c.write('scoreboard_objective', { name: 'sb', action: 0, displayText: '§e§lBED WARS', type: 'integer' });
                    setTimeout(() => c.write('title', { action: 0, text: '{"text":"§6§lVICTORY!"}' }), 100);
                }
                if (p.message === '/wtfmap') {
                    chat(c, 'You are currently playing on Lighthouse');
                    setTimeout(() => {
                        c.write('position', { x: 1, y: 70, z: 1, yaw: 0, pitch: 0, flags: 0 });
                        c.write('playerlist_header', { header: '{"text":"Hypixel"}', footer: '{"text":"store.hypixel.net"}' });
                    }, 100);
                }
                if (p.message === '/who') {
                    chat(c, 'ONLINE: Alice, Bob, NodeBot');
                    setTimeout(() => c.write('title', { action: 0, text: '{"text":"§6§lVICTORY!"}' }), 100);
                }
            });
        });
        setTimeout(() => process.exit(2), 30000);
        "#,
    );
    let line = expect_line(&mut server_out, "server").await;
    let upstream_port: u16 = line.strip_prefix("PORT ").unwrap().parse().unwrap();

    let proxy_port = free_port();
    let config = Config::in_memory(json!({
        "host": "127.0.0.1",
        "port": proxy_port,
        "upstream_host": "127.0.0.1",
        "upstream_port": upstream_port,
        "offline_mode": true,
        "queue_stats": { "bw": true },
        "auto_gg": { "enabled": true, "message": "gg", "delay": 0 },
    }));
    let (events, mut event_rx) = tokio::sync::broadcast::channel(256);
    let proxy = Proxy::new(ProxyOptions {
        data_dir: PathBuf::new(),
        config,
        jwt: None,
        events,
    })
    .unwrap();
    tokio::spawn(proxy.run());
    assert!(matches!(
        event_rx.recv().await.unwrap(),
        ProxyEvent::Started { .. }
    ));

    let (_client, mut client_out) = node(&format!(
        r#"
        const mc = require('minecraft-protocol');
        const c = mc.createClient({{ host: '127.0.0.1', port: {proxy_port}, username: 'NodeBot', version: '1.8.9', auth: 'offline' }});
        const text = m => {{ try {{ const j = JSON.parse(m); return (j.text || '') + (j.extra || []).map(e => e.text || '').join(''); }} catch (e) {{ return m; }} }};
        c.on('chat', p => console.log('CHAT ' + text(p.message)));
        c.on('playerlist_header', p => console.log('FOOTER ' + p.footer));
        c.on('login', () => setTimeout(() => {{
            c.write('chat', {{ message: '/q bw4s' }});
            setTimeout(() => c.write('chat', {{ message: '/rq' }}), 100);
            setTimeout(() => c.write('chat', {{ message: '/jagprox' }}), 200);
        }}, 1500));
        setTimeout(() => process.exit(2), 30000);
        "#
    ));

    let mut want_client = vec![
        "CHAT §8[§5jag§dprox§8] §r§eOne of your opponents is a §6MVP§c++".to_string(),
        "CHAT §8[§5jag§dprox§8] §r§eOne of your opponents is a §bMVP/MVP§c+".to_string(),
        "FOOTER".to_string(),
        "FOOTER".to_string(),
        "CHAT §8[§5jag§dprox§8] §r§dAuto-checking §f2 §dplayers for §5Bed Wars§d...".to_string(),
        "CHAT §8[§5jag§dprox§8] §r§eJoining Bedwars 4v4v4v4...".to_string(),
        "CHAT §8[§5jag§dprox§8] §r§dRe-queuing §8» §f/play bedwars_four_four".to_string(),
        "CHAT §8[§5jag§dprox§8] §r§5§m---".to_string(),
    ];
    let mut want_server = vec![
        "SERVER GOT /wtfmap".to_string(),
        "SERVER GOT /who".to_string(),
        "SERVER GOT /ac gg".to_string(),
        "SERVER GOT /ac gg".to_string(),
        "SERVER GOT /play bedwars_four_four".to_string(),
    ];
    let mut footers = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !(want_client.is_empty() && want_server.is_empty()) {
        tokio::select! {
            line = client_out.next_line() => {
                let line = line.unwrap().expect("client exited");
                if line.starts_with("FOOTER") {
                    footers += 1;
                    // Only game 1's footer may show its map; game 2's is after a server switch.
                    assert_eq!(line.contains("You are currently playing on §5Lighthouse"), footers == 1, "{line}");
                }
                if let Some(i) = want_client.iter().position(|w| line.starts_with(w.as_str())) {
                    want_client.remove(i);
                }
            }
            line = server_out.next_line() => {
                let line = line.unwrap().expect("server exited");
                assert_ne!(line, "SERVER GOT /jagprox", "JagProx commands must not reach the server");
                if let Some(i) = want_server.iter().position(|w| *w == line) {
                    want_server.remove(i);
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("still waiting for client {want_client:?} and server {want_server:?}");
            }
        }
    }
}
