//! Small helpers that reproduce JavaScript behaviour the original code relies on,
//! so output stays byte-identical: `x || 0` on JSON values, `toFixed` and
//! `toLocaleString` (en-US).

use serde_json::Value;

/// JS `value || 0` for a number: missing, null, non-numeric, 0 and NaN all become 0.
pub fn num(value: Option<&Value>) -> f64 {
    match value.and_then(Value::as_f64) {
        Some(n) if !n.is_nan() => n,
        _ => 0.0,
    }
}

/// `obj[key] || 0` where `obj` may be missing or not an object.
pub fn field(obj: Option<&Value>, key: &str) -> f64 {
    num(obj.and_then(|o| o.get(key)))
}

/// JS truthiness of a JSON value.
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// JS `Number.prototype.toFixed(digits)`: ties round away from zero, unlike Rust's
/// formatter, which rounds ties to even (`0.125` must print `0.13`, not `0.12`).
pub fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    // Rust prints the exact binary value when given enough digits, so an exact tie
    // shows up as a single '5' followed only by zeros.
    let exact = format!("{:.*}", digits + 40, value.abs());
    let cut = exact.len() - 40;
    let is_tie = exact[cut..].starts_with('5') && exact[cut + 1..].bytes().all(|b| b == b'0');
    let magnitude = if is_tie {
        let step = 10f64.powi(-(digits as i32));
        // Truncate, then step up one unit in the last place.
        let truncated = &exact[..cut];
        let base: f64 = truncated.trim_end_matches('.').parse().unwrap_or(0.0);
        format!("{:.*}", digits, base + step)
    } else {
        format!("{:.*}", digits, value.abs())
    };
    if value < 0.0 && magnitude.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        format!("-{magnitude}")
    } else {
        magnitude
    }
}

/// JS `Number.prototype.toLocaleString()` in en-US: thousands separators and at most
/// three fraction digits, trailing zeros dropped.
pub fn to_locale(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "∞".into()
        } else {
            "-∞".into()
        };
    }
    let fixed = to_fixed(value.abs(), 3);
    let (int_part, frac_part) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let frac = frac_part.trim_end_matches('0');

    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let sign = if value < 0.0 && (int_part != "0" || !frac.is_empty()) {
        "-"
    } else {
        ""
    };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac}")
    }
}

/// JS `String.prototype.padEnd` counting characters (JS counts UTF-16 units; every
/// string we pad is BMP-only, so chars are equivalent).
pub fn pad_end(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - len))
    }
}

/// JS `String.prototype.padStart`, see [`pad_end`].
pub fn pad_start(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{}{s}", " ".repeat(width - len))
    }
}

/// JS `str.substring(0, n)` by characters, so `✫`/`§`/`█` are never split.
pub fn take_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn to_fixed_rounds_ties_up_like_js() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(0.375, 2), "0.38");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(3.0, 2), "3.00");
        assert_eq!(to_fixed(1.005, 2), "1.00"); // 1.005 is really 1.00499999…, as in JS
        assert_eq!(to_fixed(10.0 / 3.0, 2), "3.33");
        assert_eq!(to_fixed(0.0, 2), "0.00");
    }

    #[test]
    fn to_locale_groups_thousands() {
        assert_eq!(to_locale(0.0), "0");
        assert_eq!(to_locale(999.0), "999");
        assert_eq!(to_locale(1000.0), "1,000");
        assert_eq!(to_locale(1234567.0), "1,234,567");
        assert_eq!(to_locale(1234.5678), "1,234.568");
        assert_eq!(to_locale(-1500.0), "-1,500");
    }

    #[test]
    fn num_mimics_or_zero() {
        let v = json!({ "a": 5, "b": null, "c": "x", "d": 2.5 });
        assert_eq!(field(Some(&v), "a"), 5.0);
        assert_eq!(field(Some(&v), "b"), 0.0);
        assert_eq!(field(Some(&v), "c"), 0.0);
        assert_eq!(field(Some(&v), "d"), 2.5);
        assert_eq!(field(Some(&v), "missing"), 0.0);
        assert_eq!(field(None, "a"), 0.0);
    }

    #[test]
    fn take_chars_never_splits_multibyte() {
        assert_eq!(take_chars("§a[123✫] ", 7), "§a[123✫");
        assert_eq!(take_chars("ab", 16), "ab");
    }
}
