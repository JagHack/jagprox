//! Port of `utils/chatParsers.js`: pure parsers for Hypixel chat lines. Input is
//! always color-stripped plain text.

use crate::formatter::strip_colors;

fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `"[MVP+] Name"` → `"Name"`; also drops one trailing dot and stray whitespace.
/// (JS: `raw.replace(/\[.*?\]\s?/g, '').trim().replace(/\.$/, '')`.)
pub fn clean_player_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let close = after.find(|c: char| c == ']' || is_line_break(c));
        match close {
            Some(i) if after[i..].starts_with(']') => {
                out.push_str(&rest[..open]);
                let mut tail = &after[i + 1..];
                if let Some(c) = tail.chars().next().filter(|c| c.is_whitespace()) {
                    tail = &tail[c.len_utf8()..];
                }
                rest = tail;
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    let trimmed = out.trim();
    trimmed.strip_suffix('.').unwrap_or(trimmed).to_string()
}

/// JS `.split(/,\s*/)`.
fn split_commas(s: &str) -> Vec<&str> {
    s.split(',')
        .enumerate()
        .map(|(i, p)| if i == 0 { p } else { p.trim_start() })
        .collect()
}

/// `"ONLINE: a, b, c"` (from `/who`) → `["a", "b", "c"]`, or `None` if the line isn't one.
pub fn parse_online_line(clean: &str) -> Option<Vec<String>> {
    let list = clean.strip_prefix("ONLINE: ")?;
    Some(
        split_commas(list)
            .into_iter()
            .map(clean_player_name)
            .filter(|n| !n.is_empty())
            .collect(),
    )
}

/// `"Team #1: a, b"` (team-mode `/who` output) → `["a", "b"]`, or `None`.
pub fn parse_team_line(clean: &str) -> Option<Vec<String>> {
    if !clean.starts_with("Team #") {
        return None;
    }
    let Some(colon) = clean.find(':') else {
        return Some(Vec::new());
    };
    Some(
        split_commas(&clean[colon + 1..])
            .into_iter()
            .map(clean_player_name)
            .filter(|n| !n.is_empty())
            .collect(),
    )
}

/// `"Party Members: [VIP] A ● B ●"` / `"Party Leader: [MVP+] C ●"` → names, or `None`.
pub fn parse_party_line(clean: &str) -> Option<Vec<String>> {
    let rest = ["Party Leader:", "Party Moderators:", "Party Members:"]
        .iter()
        .find_map(|prefix| clean.strip_prefix(prefix))?;
    if rest.contains(is_line_break) {
        return None;
    }
    let list = rest.trim_start();
    Some(
        list.split(['●', ','])
            .map(clean_player_name)
            .filter_map(|part| part.split_whitespace().last().map(str::to_string))
            .filter(|n| !n.is_empty())
            .collect(),
    )
}

/// Win or loss from an end-of-game line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    Win,
    Loss,
}

impl GameResult {
    pub fn as_str(self) -> &'static str {
        match self {
            GameResult::Win => "win",
            GameResult::Loss => "loss",
        }
    }
}

/// End-of-game `"... WINNER!"` lines.
///   `"Name WINNER!"`     (one word before)  → win (solo modes name the winner; that's us when we're tracking)
///   `"Red Team WINNER!"` (2+ words before)  → loss
///   `"WINNER! Name"`     (text after)       → win
///
/// Player chat (contains ':') and lobby/replay/spectator lines are ignored.
pub fn detect_game_result(text: &str) -> Option<GameResult> {
    let upper = strip_colors(text).to_uppercase();
    let upper = upper.trim();
    if !upper.contains("WINNER!") || text.contains(':') {
        return None;
    }
    if upper.contains("LOBBY") || upper.contains("REPLAY") || upper.contains("SPECTATOR") {
        return None;
    }

    let (before, after) = upper.split_once("WINNER!")?;
    if !after.trim().is_empty() {
        return Some(GameResult::Win);
    }
    match before.trim().split(' ').filter(|w| !w.is_empty()).count() {
        0 => None,
        1 => Some(GameResult::Win),
        _ => Some(GameResult::Loss),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_line_splits_who_output_and_strips_ranks_and_trailing_dots() {
        assert_eq!(
            parse_online_line("ONLINE: Alice, [MVP+] Bob, Carol."),
            Some(vec!["Alice".into(), "Bob".into(), "Carol".into()])
        );
        assert_eq!(parse_online_line("Something else"), None);
    }

    #[test]
    fn team_line_reads_team_mode_who_lines() {
        assert_eq!(
            parse_team_line("Team #1: Alice, Bob"),
            Some(vec!["Alice".into(), "Bob".into()])
        );
        assert_eq!(parse_team_line("ONLINE: Alice"), None);
    }

    #[test]
    fn party_line_handles_separators_and_rank_prefixes() {
        assert_eq!(
            parse_party_line("Party Leader: [MVP+] Leader ●"),
            Some(vec!["Leader".into()])
        );
        assert_eq!(
            parse_party_line("Party Members: [VIP] Alice ● Bob ● [MVP++] Carol ●"),
            Some(vec!["Alice".into(), "Bob".into(), "Carol".into()])
        );
        assert_eq!(parse_party_line("Party Members (3)"), None);
    }

    #[test]
    fn detect_game_result_classifies_winner_lines() {
        assert_eq!(
            detect_game_result("Red Team WINNER!"),
            Some(GameResult::Loss)
        );
        assert_eq!(detect_game_result("Steve WINNER!"), Some(GameResult::Win));
        assert_eq!(detect_game_result("WINNER! Steve"), Some(GameResult::Win));
        assert_eq!(
            detect_game_result("[MVP+] Steve: WINNER!"),
            None,
            "player chat is ignored"
        );
        assert_eq!(detect_game_result("Click to return to LOBBY WINNER!"), None);
        assert_eq!(detect_game_result("Just a normal message"), None);
    }

    #[test]
    fn clean_player_name_edge_cases() {
        assert_eq!(clean_player_name("[MVP+] Bob"), "Bob");
        assert_eq!(clean_player_name("[a] [b]Bob."), "Bob");
        assert_eq!(clean_player_name("[unclosed Bob"), "[unclosed Bob");
    }
}
