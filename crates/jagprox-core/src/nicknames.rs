//! Port of `utils/nicknames.js`: client-side nicknames in chat and the tab list.

use indexmap::IndexMap;
use serde_json::Value;

/// Real name → nickname, in config order.
pub type Nicknames = IndexMap<String, String>;

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Replaces `name` only as a whole Minecraft name (letters, digits, underscore), so
/// "Bob" doesn't hit "Bobby". Same scan as the JS regex
/// `(?<![A-Za-z0-9_])name(?![A-Za-z0-9_])` with the `g` flag.
fn replace_whole_name(text: &str, name: &str, nickname: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < text.len() {
        if text[i..].starts_with(name) {
            let before_ok = !text[..i].chars().next_back().is_some_and(is_name_char);
            let end = i + name.len();
            let after_ok = !text[end..].chars().next().is_some_and(is_name_char);
            if before_ok && after_ok {
                out.push_str(&text[copied..i]);
                out.push_str(nickname);
                i = end;
                copied = end;
                continue;
            }
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    out.push_str(&text[copied..]);
    out
}

pub fn replace_names_in_text(text: &str, nicknames: &Nicknames) -> String {
    let mut result = text.to_string();
    for (real_name, nickname) in nicknames {
        if real_name.is_empty() || !result.contains(real_name.as_str()) {
            continue;
        }
        result = replace_whole_name(&result, real_name, nickname);
    }
    result
}

/// Rewrites every real name in a chat component tree, in place. Plain-string entries
/// in `extra` are left alone, as in the original.
pub fn replace_names_in_component(component: &mut Value, nicknames: &Nicknames) {
    let Value::Object(map) = component else {
        return;
    };
    if let Some(Value::String(text)) = map.get_mut("text") {
        *text = replace_names_in_text(text, nicknames);
    }
    if let Some(Value::Array(extra)) = map.get_mut("extra") {
        for part in extra {
            replace_names_in_component(part, nicknames);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn nicks(pairs: &[(&str, &str)]) -> Nicknames {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn replaces_whole_names_only() {
        assert_eq!(
            replace_names_in_text("Bob and Bobby", &nicks(&[("Bob", "Nick")])),
            "Nick and Bobby"
        );
    }

    #[test]
    fn special_characters_in_names_are_literal() {
        assert_eq!(
            replace_names_in_text("a+ said hi", &nicks(&[("a+", "X")])),
            "X said hi"
        );
        assert_eq!(
            replace_names_in_text("text (", &nicks(&[("(", "X")])),
            "text X"
        );
    }

    #[test]
    fn replaces_every_configured_name_in_a_component_tree() {
        let mut component = json!({ "text": "Alice vs ", "extra": [{ "text": "Bob" }] });
        replace_names_in_component(&mut component, &nicks(&[("Alice", "A"), ("Bob", "B")]));
        assert_eq!(
            component,
            json!({ "text": "A vs ", "extra": [{ "text": "B" }] })
        );
    }

    #[test]
    fn handles_multibyte_text_around_names() {
        assert_eq!(
            replace_names_in_text("✫Bob✫ Bob_", &nicks(&[("Bob", "N")])),
            "✫N✫ Bob_"
        );
    }

    // Parity with the JS original (known bug, fix after v3.0): the letter or digit of
    // a color code counts as part of the name, so "§aBob" and "§7Bob" are not replaced.
    #[test]
    fn color_code_directly_before_a_name_blocks_replacement() {
        assert_eq!(
            replace_names_in_text("§aBob §7Bob §r Bob", &nicks(&[("Bob", "N")])),
            "§aBob §7Bob §r N"
        );
    }
}
