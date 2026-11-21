//! config.yml, shared by the proxy, the launcher and the CLI. Kept as a JSON map (like
//! the JS object it replaces) so unknown keys survive, and reloaded when the file
//! changes on disk.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde_json::{Map, Value};

use crate::file_util::{self, ConfigError};
use crate::nicknames::Nicknames;

/// The default config shipped with JagProx.
pub const DEFAULT_CONFIG: &str = include_str!("../../../config.yml");

#[derive(Clone)]
pub struct Config {
    path: PathBuf,
    values: Arc<RwLock<Map<String, Value>>>,
}

impl Config {
    /// Loads `<data_dir>/config.yml`, creating it from the default first if needed and
    /// adding keys that newer versions introduced.
    pub fn load(data_dir: &Path) -> Result<Self, ConfigError> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join("config.yml");
        if let file_util::SyncStatus::Unparseable { backup, error } =
            file_util::sync_config_file(&path, DEFAULT_CONFIG)?
        {
            crate::log::log(format!(
                "[WARN] config.yml could not be read ({error}); a backup was saved to {}",
                backup.display()
            ));
        }
        let values = read(&path)?;
        crate::log::set_debug(
            values
                .get("debug_mode")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        );
        Ok(Self {
            path,
            values: Arc::new(RwLock::new(values)),
        })
    }

    /// A config that only lives in memory (tests).
    pub fn in_memory(values: Value) -> Self {
        let values = values.as_object().cloned().unwrap_or_default();
        Self {
            path: PathBuf::new(),
            values: Arc::new(RwLock::new(values)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.values
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    fn bool_at(&self, path: &[&str]) -> Option<bool> {
        let values = self.values.read().unwrap_or_else(|e| e.into_inner());
        let mut v = values.get(path[0])?;
        for key in &path[1..] {
            v = v.get(key)?;
        }
        v.as_bool()
    }

    pub fn port(&self) -> u16 {
        self.get("port")
            .and_then(|v| v.as_u64())
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p != 0)
            .unwrap_or(crate::DEFAULT_PROXY_PORT)
    }

    pub fn host(&self) -> String {
        self.get("host")
            .and_then(|v| v.as_str().map(str::to_string))
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "127.0.0.1".into())
    }

    /// Hidden, for development and tests: the server to proxy to.
    pub fn upstream(&self) -> (String, u16) {
        let host = self
            .get("upstream_host")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "mc.hypixel.net".into());
        let port = self
            .get("upstream_port")
            .and_then(|v| v.as_u64())
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(25565);
        (host, port)
    }

    /// Hidden, for development and tests: skip Mojang authentication on both sides.
    pub fn offline_mode(&self) -> bool {
        self.get("offline_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn nicknames(&self) -> Nicknames {
        self.get("nicknames")
            .and_then(|v| {
                v.as_object().map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    pub fn discord_rpc_enabled(&self) -> bool {
        self.bool_at(&["discord_rpc", "enabled"]).unwrap_or(false)
    }

    pub fn stats_telemetry(&self) -> bool {
        self.get("stats_telemetry").and_then(|v| v.as_bool()) == Some(true)
    }

    /// Start broadcasting automatically on login (off by default). While it's off,
    /// `/bc on` and `/bc off` show a tip that it can be enabled.
    pub fn auto_broadcast(&self) -> bool {
        self.get("auto_broadcast").and_then(|v| v.as_bool()) == Some(true)
    }

    /// Changes the in-memory config and writes the given keys back to the file
    /// (read-modify-write, so keys the launcher changed meanwhile are kept).
    pub fn update(
        &self,
        keys: &[&str],
        change: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<(), ConfigError> {
        let snapshot = {
            let mut values = self.values.write().unwrap_or_else(|e| e.into_inner());
            change(&mut values);
            keys.iter()
                .map(|k| (k.to_string(), values.get(*k).cloned()))
                .collect::<Vec<_>>()
        };
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        file_util::update_config_file(&self.path, |file| {
            for (key, value) in snapshot {
                match value {
                    Some(v) => {
                        file.insert(key, v);
                    }
                    None => {
                        file.remove(&key);
                    }
                }
            }
        })?;
        Ok(())
    }

    /// Re-reads the file every 2 s while it keeps changing (port/host need a restart).
    pub fn watch(&self) -> tokio::task::JoinHandle<()> {
        let config = self.clone();
        file_util::watch_file(self.path.clone(), move || match read(&config.path) {
            Ok(fresh) => {
                crate::log::set_debug(
                    fresh
                        .get("debug_mode")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                );
                *config.values.write().unwrap_or_else(|e| e.into_inner()) = fresh;
                crate::log::log("config.yml changed on disk, settings reloaded.");
            }
            Err(e) => crate::log::log(format!(
                "[ERROR] config.yml changed but could not be reloaded (keeping old settings): {e}"
            )),
        })
    }
}

fn read(path: &Path) -> Result<Map<String, Value>, ConfigError> {
    match file_util::parse_yaml(&std::fs::read_to_string(path)?)? {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        _ => Err(ConfigError::NotAMapping),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_and_values() {
        let config =
            Config::in_memory(json!({ "port": 25570, "nicknames": { "Bob": "B", "Bad": 5 } }));
        assert_eq!(config.port(), 25570);
        assert_eq!(config.host(), "127.0.0.1");
        assert_eq!(config.upstream(), ("mc.hypixel.net".into(), 25565));
        assert_eq!(config.nicknames().len(), 1);
        assert!(!config.discord_rpc_enabled());
    }

    #[test]
    fn load_creates_the_default_and_update_writes_back() {
        let dir = std::env::temp_dir().join(format!("jagprox-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = Config::load(&dir).unwrap();
        assert_eq!(config.port(), 2107);
        config
            .update(&["tab_alerts"], |c| {
                c.insert("tab_alerts".into(), json!(["Notch"]));
            })
            .unwrap();
        let reloaded = Config::load(&dir).unwrap();
        assert_eq!(reloaded.get("tab_alerts"), Some(json!(["Notch"])));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
