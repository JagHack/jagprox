//! Discord Rich Presence (port of `modules/discordRpcHandler.js`). The IPC client is
//! blocking, so it lives on its own thread and takes commands over a channel.

use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use jagprox_core::log::log;

const CLIENT_ID: &str = "1420457169400238121";

enum Command {
    Enable(bool),
    Playing(bool),
}

/// Handle to the Discord thread.
pub struct Discord {
    tx: mpsc::Sender<Command>,
}

impl Discord {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("discord-rpc".into())
            .spawn(move || run(rx))
            .expect("start the Discord thread");
        Self { tx }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.tx.send(Command::Enable(enabled));
    }

    /// Switches between "Idling / In Launcher" and "Playing / In Game".
    pub fn set_playing(&self, playing: bool) {
        let _ = self.tx.send(Command::Playing(playing));
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn apply(client: &mut DiscordIpcClient, playing: bool, since: i64) -> bool {
    let (details, state) = if playing {
        ("Playing", "In Game")
    } else {
        ("Idling", "In Launcher")
    };
    let activity = Activity::new()
        .details(details)
        .state(state)
        .assets(Assets::new().large_image("icon").large_text("JagProx"))
        .timestamps(Timestamps::new().start(since));
    client.set_activity(activity).is_ok()
}

fn run(rx: mpsc::Receiver<Command>) {
    let mut client: Option<DiscordIpcClient> = None;
    // The activity is remembered while disconnected and applied on the next connect.
    let mut playing = false;
    let mut since = now();

    while let Ok(command) = rx.recv() {
        match command {
            Command::Enable(true) if client.is_none() => {
                let mut c = DiscordIpcClient::new(CLIENT_ID);
                match c.connect() {
                    Ok(()) => {
                        log("Discord RPC connected");
                        apply(&mut c, playing, since);
                        client = Some(c);
                    }
                    Err(e) => log(format!("Failed to connect Discord RPC: {e}")),
                }
            }
            Command::Enable(false) => {
                if let Some(mut c) = client.take() {
                    let _ = c.close();
                    log("Discord RPC disconnected");
                }
            }
            Command::Enable(true) => {}
            Command::Playing(now_playing) => {
                playing = now_playing;
                since = now();
                if let Some(c) = &mut client
                    && !apply(c, playing, since)
                {
                    log("Discord RPC disconnected");
                    client = None;
                }
            }
        }
    }
}
