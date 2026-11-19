//! Port of `autoGGHandler.js`: says "gg" (or your message) once when a game-over
//! title appears.

use std::time::Duration;

use mcproto::packets::clientbound::Title;
use serde_json::Value;

use super::{ServerPacket, State};
use crate::formatter::{extract_text, strip_colors};
use crate::log::log;
use crate::proxy::session::Session;

const GAME_OVER_KEYWORDS: &[&str] = &[
    "VICTORY!",
    "YOU WIN!",
    "GAME OVER",
    "GAMEOVER!",
    "DRAW!",
    "1ST PLACE",
    "#1 VICTORY",
    "WINNER",
    "DEFEAT",
];

#[derive(Default)]
pub struct AutoGg {
    sent_this_game: bool,
}

impl AutoGg {
    pub fn reset(&mut self) {
        self.sent_this_game = false;
    }
}

pub fn handle(st: &mut State, s: &Session, packet: &ServerPacket) {
    let ServerPacket::Title(Title::Title(text)) = packet else {
        return;
    };
    let Some(settings) = s.proxy().config().get("auto_gg") else {
        return;
    };
    if settings.get("enabled").and_then(Value::as_bool) != Some(true) || st.auto_gg.sent_this_game {
        return;
    }
    let clean = match serde_json::from_str::<Value>(text) {
        Ok(v) => strip_colors(&extract_text(&v)),
        Err(_) => strip_colors(text),
    };
    let upper = clean.trim().to_uppercase();
    if !GAME_OVER_KEYWORDS.iter().any(|k| upper.contains(k)) {
        return;
    }

    // Win/loss recording is gametrack's job; this only sends the message.
    st.auto_gg.sent_this_game = true;
    let delay = settings
        .get("delay")
        .and_then(Value::as_f64)
        .unwrap_or(1500.0)
        .max(0.0);
    let message = settings
        .get("message")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .unwrap_or("gg")
        .to_string();
    let session = s.clone();
    s.spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay as u64)).await;
        session.send_server_chat(&format!("/ac {message}"));
        log(format!("AutoGG (Title) sent: \"{message}\""));
    });
}
