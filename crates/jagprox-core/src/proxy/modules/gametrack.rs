//! Port of `gametrackClientHandler.js`: tells the backend which game you're in and
//! records wins and losses from the end-of-game chat.

use std::time::{Duration, Instant};

use super::State;
use crate::chat_parsers::detect_game_result;
use crate::log::log;
use crate::proxy::session::Session;

const DEBOUNCE: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Gametrack {
    current_game: Option<String>,
    last_event: Option<Instant>,
}

pub fn on_game_changed(st: &mut State, s: &Session, key: &str) {
    if key.is_empty() || key == "limbo" {
        st.gametrack.current_game = None;
        return;
    }
    log(format!(
        "[GameTrack] Game changed to {key}. Starting session."
    ));
    st.gametrack.current_game = Some(key.to_string());
    let Some(backend) = s.backend() else { return };
    let (session, mode) = (s.clone(), key.to_string());
    s.spawn(async move {
        if let Err(e) = backend.gametrack_start(session.uuid(), &mode).await {
            log(format!("[GameTrack] Failed to start session: {e}"));
            session.chat(&format!("§c[GameTrack] §8Error starting session: {e}"));
        }
    });
}

/// Every server chat line (plain text of the component).
pub fn on_chat(s: &Session, text: &str) {
    let mode = {
        let mut st = s.state();
        let Some(mode) = st.gametrack.current_game.clone() else {
            return;
        };
        if st
            .gametrack
            .last_event
            .is_some_and(|t| t.elapsed() < DEBOUNCE)
        {
            return;
        }
        let Some(result) = detect_game_result(text) else {
            return;
        };
        st.gametrack.last_event = Some(Instant::now());
        log(format!(
            "[GameTrack] Detected {} in {mode}.",
            result.as_str()
        ));
        (mode, result)
    };
    let Some(backend) = s.backend() else { return };
    let session = s.clone();
    s.spawn(async move {
        let (mode, result) = mode;
        match backend
            .gametrack_event(session.uuid(), &mode, result.as_str())
            .await
        {
            Ok(_) => session.chat(&format!(
                "§d[GameTrack] §8Recorded game result: §5{}§8.",
                result.as_str().to_uppercase()
            )),
            Err(e) => {
                session.chat(&format!("§c[GameTrack] §8Error recording event: {e}"));
                log(format!("[GameTrack] Failed to send gametrack event: {e}"));
            }
        }
    });
}
