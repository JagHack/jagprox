//! Checks the number formatting against values produced by Node itself
//! (`node scripts/gen-js-fixtures.js`).

use jagprox_core::js::{to_fixed, to_locale};
use serde_json::Value;

#[test]
fn number_formatting_matches_node() {
    let fixtures: Value = serde_json::from_str(include_str!("fixtures/js_numbers.json")).unwrap();
    let mut failures = Vec::new();
    for case in fixtures["toFixed"].as_array().unwrap() {
        let (value, expected) = (case[0].as_f64().unwrap(), case[1].as_str().unwrap());
        if to_fixed(value, 2) != expected {
            failures.push(format!(
                "toFixed({value}) = {} want {expected}",
                to_fixed(value, 2)
            ));
        }
    }
    for case in fixtures["toLocale"].as_array().unwrap() {
        let (value, expected) = (case[0].as_f64().unwrap(), case[1].as_str().unwrap());
        if to_locale(value) != expected {
            failures.push(format!(
                "toLocaleString({value}) = {} want {expected}",
                to_locale(value)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
