//! Port of `superFriendTracker.js`: while you're connected, polls the Hypixel status of
//! everyone in `super_friends` once a minute and tells you when one of them joins a
//! game you track them for.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use crate::constants::game_mode;
use crate::log::log;
use crate::proxy::session::Session;

const FIRST_POLL: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Hypixel status `gameType` for a `/superf` game argument; unknown arguments are
/// compared with the game type directly (so "SURVIVAL_GAMES" also works).
fn game_type_for(mode: &str) -> String {
    game_mode(&mode.to_lowercase())
        .map_or_else(|| mode.to_string(), |g| g.api_name.to_string())
        .to_uppercase()
}

pub fn matches_tracked_modes(game_type: &str, modes: &[String]) -> bool {
    modes.iter().any(|mode| {
        let lower = mode.to_lowercase();
        lower == "any" || lower == "all" || game_type_for(mode) == game_type.to_uppercase()
    })
}

/// Runs for the whole session (cancelled with it).
pub async fn run(s: Session) {
    let mut last_seen: HashMap<String, String> = HashMap::new();
    tokio::time::sleep(FIRST_POLL).await;
    loop {
        let friends: Vec<(String, Vec<String>)> = s
            .proxy()
            .config()
            .get("super_friends")
            .and_then(|v| v.as_object().cloned())
            .map(|m| {
                m.into_iter()
                    .map(|(name, modes)| {
                        let modes = modes
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|m| m.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (name, modes)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (name, modes) in friends {
            if let Err(e) = check_friend(&s, &name, &modes, &mut last_seen).await {
                log(format!("Super friend poll failed: {e}"));
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn check_friend(
    s: &Session,
    name: &str,
    modes: &[String],
    last_seen: &mut HashMap<String, String>,
) -> Result<(), crate::api::ApiError> {
    let Some(profile) = s.hypixel().mojang_uuid(name).await? else {
        return Ok(());
    };
    let session = s.hypixel().session_status(&profile.uuid).await?;
    if session.is_null() {
        return Ok(());
    }
    let text = |k: &str| session.get(k).and_then(Value::as_str).map(str::to_string);
    let online = session.get("online").and_then(Value::as_bool) == Some(true);
    let undefined = || "undefined".to_string();
    let key = if online {
        format!(
            "{}|{}|{}",
            text("gameType").unwrap_or_else(undefined),
            text("mode").unwrap_or_else(undefined),
            text("map").unwrap_or_else(undefined)
        )
    } else {
        "offline".into()
    };
    let previous = last_seen.insert(name.to_string(), key.clone());

    // The first poll only records a baseline; announce changes after that.
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous == key || !online {
        return Ok(());
    }
    let game_type = text("gameType").unwrap_or_else(undefined);
    if !matches_tracked_modes(&game_type, modes) {
        return Ok(());
    }
    let details: Vec<String> = [text("mode"), text("map")]
        .into_iter()
        .flatten()
        .filter(|t| !t.is_empty())
        .collect();
    let details = if details.is_empty() {
        String::new()
    } else {
        format!(" §8(§f{}§8)", details.join(" §8- §f"))
    };
    s.chat(&format!(
        "§dSuper friend §5{} §djoined §f{game_type}{details}",
        profile.username
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn matches_game_aliases_against_hypixel_game_types() {
        assert!(matches_tracked_modes("BEDWARS", &modes(&["bw"])));
        assert!(matches_tracked_modes(
            "SKYWARS",
            &modes(&["skywars", "duels"])
        ));
        assert!(matches_tracked_modes("PIT", &modes(&["any"])));
        assert!(!matches_tracked_modes("DUELS", &modes(&["bedwars"])));
        assert!(!matches_tracked_modes("DUELS", &modes(&[])));
    }
}
