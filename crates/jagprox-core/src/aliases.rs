//! Port of `aliasManager.js`: chat command aliases (`/bw4` → `/play bedwars_four_four`)
//! from `aliases.json`, which the launcher's Aliases page edits. Reloaded on change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::log::log;

pub const DEFAULT_ALIASES: &str = include_str!("../../../aliases.json");

#[derive(Clone)]
pub struct Aliases {
    path: PathBuf,
    map: Arc<RwLock<HashMap<String, String>>>,
}

/// Lowercases keys and drops non-string values.
fn normalize(value: &Value) -> HashMap<String, String> {
    value
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.to_lowercase(), v.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn parse(text: &str) -> Result<HashMap<String, String>, serde_json::Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    Ok(normalize(&serde_json::from_str::<Value>(text)?))
}

impl Aliases {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("aliases.json");
        let aliases = Self {
            path,
            map: Arc::new(RwLock::new(HashMap::new())),
        };
        if !aliases.path.exists() {
            log("aliases.json not found. Creating it from the defaults.");
            let _ = crate::file_util::write_file_atomic(&aliases.path, DEFAULT_ALIASES);
        }
        aliases.reload();
        aliases
    }

    /// Fixed aliases for tests.
    pub fn from_map(map: HashMap<String, String>) -> Self {
        Self {
            path: PathBuf::new(),
            map: Arc::new(RwLock::new(map)),
        }
    }

    fn reload(&self) {
        match std::fs::read_to_string(&self.path)
            .map_err(|e| e.to_string())
            .and_then(|t| parse(&t).map_err(|e| e.to_string()))
        {
            Ok(map) => {
                log(format!(
                    "Successfully loaded {} aliases from aliases.json.",
                    map.len()
                ));
                *self.map.write().unwrap_or_else(|e| e.into_inner()) = map;
            }
            // Keep the previous aliases if the file is mid-edit or invalid.
            Err(e) => log(format!("[ERROR] Failed to load or parse aliases.json: {e}")),
        }
    }

    /// The target of an alias typed exactly (case-insensitive), if any.
    pub fn get(&self, message: &str) -> Option<String> {
        self.map
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&message.to_lowercase())
            .cloned()
    }

    /// Reloads when the launcher saves new aliases.
    pub fn watch(&self) -> tokio::task::JoinHandle<()> {
        let aliases = self.clone();
        tokio::spawn(async move {
            let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let mut last: Option<SystemTime> = modified(&aliases.path);
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let now = modified(&aliases.path);
                if now != last {
                    last = now;
                    aliases.reload();
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_lowercased_and_bom_is_ignored() {
        let map = parse("\u{feff}{\"/BW4\": \"/play bedwars_four_four\", \"/x\": 5}").unwrap();
        assert_eq!(
            map.get("/bw4").map(String::as_str),
            Some("/play bedwars_four_four")
        );
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn default_aliases_parse() {
        assert!(!parse(DEFAULT_ALIASES).unwrap().is_empty());
    }
}
