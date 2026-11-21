//! Port of `formatter.js`: chat component text, legacy § codes and Hypixel ranks.

use serde_json::Value;

/// Chat component color/format name to its § code.
fn color_code(name: &str) -> Option<&'static str> {
    Some(match name {
        "black" => "§0",
        "dark_blue" => "§1",
        "dark_green" => "§2",
        "dark_aqua" => "§3",
        "dark_red" => "§4",
        "dark_purple" => "§5",
        "gold" => "§6",
        "gray" => "§7",
        "dark_gray" => "§8",
        "blue" => "§9",
        "green" => "§a",
        "aqua" => "§b",
        "red" => "§c",
        "light_purple" => "§d",
        "yellow" => "§e",
        "white" => "§f",
        "obfuscated" => "§k",
        "bold" => "§l",
        "strikethrough" => "§m",
        "underline" => "§n",
        "italic" => "§o",
        "reset" => "§r",
        _ => return None,
    })
}

/// Hypixel's `rankPlusColor` / `monthlyRankColor` values.
pub fn plus_color(name: &str) -> Option<&'static str> {
    Some(match name {
        "RED" => "§c",
        "GOLD" => "§6",
        "LIGHT_PURPLE" => "§d",
        "DARK_PURPLE" => "§5",
        "DARK_BLUE" => "§1",
        "DARK_GREEN" => "§2",
        "DARK_AQUA" => "§3",
        "DARK_RED" => "§4",
        "DARK_GRAY" => "§8",
        "GRAY" => "§7",
        "BLUE" => "§9",
        "GREEN" => "§a",
        "AQUA" => "§b",
        "YELLOW" => "§e",
        "WHITE" => "§f",
        "BLACK" => "§0",
        _ => return None,
    })
}

/// Removes every `§x` pair (JS `/§./g`; `.` never matches a line break).
pub fn strip_colors(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            let mut lookahead = chars.clone();
            match lookahead.next() {
                Some(n) if !matches!(n, '\n' | '\r' | '\u{2028}' | '\u{2029}') => {
                    chars = lookahead;
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

/// Plain text of a chat component (or string), including all `extra` children.
pub fn extract_text(chat: &Value) -> String {
    let mut out = String::new();
    extract_into(chat, &mut out);
    out
}

fn extract_into(chat: &Value, out: &mut String) {
    match chat {
        Value::Null => {}
        Value::String(s) => out.push_str(s),
        _ => {
            if let Some(text) = js_text(chat.get("text")) {
                out.push_str(&text);
            }
            if let Some(Value::Array(extra)) = chat.get("extra") {
                for part in extra {
                    extract_into(part, out);
                }
            }
        }
    }
}

/// JS `obj.text || ""` followed by string concatenation.
fn js_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) if n.as_f64().is_some_and(|f| f != 0.0) => Some(n.to_string()),
        Value::Bool(true) => Some("true".into()),
        _ => None,
    }
}

/// Parses a JSON chat string to plain text with color codes removed.
/// Returns `None` if the string isn't valid JSON.
pub fn chat_to_clean_text(json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json).ok()?;
    Some(strip_colors(&extract_text(&value)).trim().to_string())
}

/// Turns a chat component back into legacy `§` text.
pub fn reconstruct_legacy_text(component: &Value) -> String {
    let mut out = String::new();
    legacy_into(component, &mut out);
    out
}

/// Same as [`reconstruct_legacy_text`] for a JSON string; non-JSON is returned as-is.
pub fn reconstruct_legacy_text_str(json: &str) -> String {
    match serde_json::from_str::<Value>(json) {
        Ok(value) => reconstruct_legacy_text(&value),
        Err(_) => json.to_string(),
    }
}

fn legacy_into(part: &Value, out: &mut String) {
    if let Value::String(s) = part {
        out.push_str(s);
        return;
    }
    if let Some(code) = part
        .get("color")
        .and_then(Value::as_str)
        .and_then(color_code)
    {
        out.push_str(code);
    }
    let flag = |key: &str| crate::js::truthy(part.get(key));
    if flag("bold") {
        out.push_str("§l");
    }
    if flag("italic") {
        out.push_str("§o");
    }
    if flag("underlined") {
        out.push_str("§n");
    }
    if flag("strikethrough") {
        out.push_str("§m");
    }
    if flag("obfuscated") {
        out.push_str("§k");
    }
    if let Some(text) = js_text(part.get("text")) {
        out.push_str(&text);
    }
    if let Some(Value::Array(extra)) = part.get("extra") {
        for child in extra {
            legacy_into(child, out);
        }
    }
}

fn non_empty_str<'a>(player: &'a Value, key: &str) -> Option<&'a str> {
    player
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// The single source of truth for a Hypixel player's displayed rank.
/// Staff ranks (`player.rank`) win over purchased ranks.
pub fn get_rank(player: Option<&Value>) -> String {
    let Some(player) = player.filter(|p| !p.is_null()) else {
        return "NONE".into();
    };
    if let Some(rank) = non_empty_str(player, "rank").filter(|r| *r != "NORMAL") {
        return rank.into();
    }
    if non_empty_str(player, "monthlyPackageRank") == Some("SUPERSTAR") {
        return "MVP_PLUS_PLUS".into();
    }
    for key in ["newPackageRank", "packageRank"] {
        if let Some(rank) = non_empty_str(player, key).filter(|r| *r != "NONE") {
            return rank.into();
        }
    }
    "NONE".into()
}

fn mapped_color(player: &Value, key: &str, default: &'static str) -> &'static str {
    player
        .get(key)
        .and_then(Value::as_str)
        .and_then(plus_color)
        .unwrap_or(default)
}

/// Rank prefix such as `§b[MVP§6+§b]`, or `§7` for players without a rank.
pub fn format_rank(player: Option<&Value>) -> String {
    let Some(p) = player.filter(|p| !p.is_null()) else {
        return "§7".into();
    };
    let plus = mapped_color(p, "rankPlusColor", "§c");
    let superstar = mapped_color(p, "monthlyRankColor", "§6");
    match get_rank(Some(p)).as_str() {
        "MVP_PLUS_PLUS" => format!("{superstar}[MVP{plus}++{superstar}]"),
        "MVP_PLUS" => format!("§b[MVP{plus}+§b]"),
        "MVP" => "§b[MVP]".into(),
        "VIP_PLUS" => "§a[VIP§6+§a]".into(),
        "VIP" => "§a[VIP]".into(),
        "YOUTUBER" | "YOUTUBE" => "§c[§fYOUTUBE§c]".into(),
        "ADMIN" => "§c[ADMIN]".into(),
        "GAME_MASTER" => "§2[GM]".into(),
        "MODERATOR" => "§2[MOD]".into(),
        "HELPER" => "§9[HELPER]".into(),
        _ => "§7".into(),
    }
}

/// Color code of the player's name for their rank.
pub fn player_name_color(player: Option<&Value>) -> &'static str {
    let Some(p) = player.filter(|p| !p.is_null()) else {
        return "§7";
    };
    match get_rank(Some(p)).as_str() {
        "MVP_PLUS_PLUS" => mapped_color(p, "monthlyRankColor", "§6"),
        "MVP_PLUS" | "MVP" => "§b",
        "VIP_PLUS" | "VIP" => "§a",
        "YOUTUBER" | "YOUTUBE" | "ADMIN" => "§c",
        "GAME_MASTER" | "MODERATOR" => "§2",
        "HELPER" => "§9",
        _ => "§7",
    }
}

/// Splits legacy `§` text into (color, bold, text) runs for the log views, so every
/// front end shows the same `MC_COLORS`. `None` is the view's default color; a color
/// code or §r ends bold, as in Minecraft; other format codes are dropped.
pub fn legacy_segments(text: &str) -> Vec<(Option<[u8; 3]>, bool, String)> {
    let mut out = Vec::new();
    let (mut color, mut bold, mut cur) = (None, false, String::new());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let Some(code) = chars.peek().copied().filter(|_| ch == '§') else {
            cur.push(ch);
            continue;
        };
        chars.next();
        if !cur.is_empty() {
            out.push((color, bold, std::mem::take(&mut cur)));
        }
        match code.to_ascii_lowercase() {
            hex @ ('0'..='9' | 'a'..='f') => {
                color = hex
                    .to_digit(16)
                    .map(|i| crate::constants::MC_COLORS[i as usize].1);
                bold = false;
            }
            'l' => bold = true,
            'r' => (color, bold) = (None, false),
            _ => {}
        }
    }
    if !cur.is_empty() {
        out.push((color, bold, cur));
    }
    out
}

#[cfg(test)]
mod legacy_segments_tests {
    use super::legacy_segments;

    #[test]
    fn colors_bold_and_reset() {
        assert_eq!(
            legacy_segments("a§4§lb§rc§"),
            vec![
                (None, false, "a".to_string()),
                (Some([170, 0, 0]), true, "b".to_string()),
                (None, false, "c§".to_string()),
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_rank_prefers_staff_rank_over_purchased_rank() {
        assert_eq!(
            get_rank(Some(
                &json!({"rank": "ADMIN", "newPackageRank": "MVP_PLUS"})
            )),
            "ADMIN"
        );
        assert_eq!(
            get_rank(Some(
                &json!({"rank": "NORMAL", "newPackageRank": "MVP_PLUS"})
            )),
            "MVP_PLUS"
        );
        assert_eq!(
            get_rank(Some(
                &json!({"monthlyPackageRank": "SUPERSTAR", "newPackageRank": "MVP_PLUS"})
            )),
            "MVP_PLUS_PLUS"
        );
        assert_eq!(get_rank(Some(&json!({}))), "NONE");
        assert_eq!(get_rank(None), "NONE");
    }

    #[test]
    fn format_rank_renders_the_plus_color() {
        assert_eq!(
            format_rank(Some(
                &json!({"newPackageRank": "MVP_PLUS", "rankPlusColor": "GOLD"})
            )),
            "§b[MVP§6+§b]"
        );
        assert_eq!(
            format_rank(Some(&json!({"newPackageRank": "VIP"}))),
            "§a[VIP]"
        );
        assert_eq!(format_rank(Some(&json!({}))), "§7");
    }

    #[test]
    fn extract_text_walks_nested_extra_arrays() {
        assert_eq!(
            extract_text(&json!({"text": "a", "extra": [{"text": "b", "extra": ["c"]}]})),
            "abc"
        );
        assert_eq!(
            chat_to_clean_text(r#"{"text":"§aHi §bthere"}"#).as_deref(),
            Some("Hi there")
        );
        assert_eq!(chat_to_clean_text("not json"), None);
    }

    #[test]
    fn reconstruct_legacy_text_understands_underlined() {
        assert_eq!(
            reconstruct_legacy_text(&json!({"text": "x", "color": "red", "underlined": true})),
            "§c§nx"
        );
    }

    #[test]
    fn strip_colors_keeps_trailing_section_sign() {
        assert_eq!(strip_colors("§aA§"), "A§");
        assert_eq!(strip_colors("a§\nb"), "a§\nb");
    }
}
