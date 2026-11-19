//! Port of `utils/fileUtil.js`: atomic writes and keeping the user's config.yml in
//! sync with the bundled default without ever losing their settings.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// Writes to a temp file in the same directory, then renames it over the target, so a
/// crash mid-write never leaves a half-written file behind.
pub fn write_file_atomic(path: &Path, content: &str) -> io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Copies `path` to `<path>.broken-<timestamp>.bak` and returns the backup path.
pub fn backup_file(path: &Path) -> io::Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S-%3fZ");
    let backup = PathBuf::from(format!("{}.broken-{stamp}.bak", path.display()));
    fs::copy(path, &backup)?;
    Ok(backup)
}

/// Converts parsed YAML to JSON, stringifying non-string keys (a numeric player name
/// key such as `1234:` must still be a name).
fn yaml_to_json(value: serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Y;
    match value {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number)
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(items) => Value::Array(items.into_iter().map(yaml_to_json).collect()),
        Y::Mapping(mapping) => Value::Object(
            mapping
                .into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        Y::String(s) => s,
                        Y::Null => "null".into(),
                        other => serde_yaml_ng::to_string(&other)
                            .unwrap_or_default()
                            .trim_end()
                            .to_string(),
                    };
                    (key, yaml_to_json(v))
                })
                .collect(),
        ),
        Y::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

/// Parses YAML text into JSON values. An empty document is `Null`.
pub fn parse_yaml(text: &str) -> Result<Value, serde_yaml_ng::Error> {
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text).map(yaml_to_json)
}

pub fn to_yaml(value: &Value) -> Result<String, serde_yaml_ng::Error> {
    serde_yaml_ng::to_string(value)
}

fn is_plain_object(value: &Value) -> bool {
    value.is_object()
}

/// Adds keys that exist in `source` but are missing from `target` (recursively for
/// objects). Never overwrites values the user already has. Returns true if anything
/// was added.
pub fn merge_missing_keys(target: &mut Map<String, Value>, source: &Map<String, Value>) -> bool {
    let mut updated = false;
    for (key, value) in source {
        if let Value::Object(source_child) = value {
            if !target.get(key).is_some_and(is_plain_object) {
                target.insert(key.clone(), Value::Object(Map::new()));
                updated = true;
            }
            if let Some(Value::Object(target_child)) = target.get_mut(key)
                && merge_missing_keys(target_child, source_child)
            {
                updated = true;
            }
        } else if !target.contains_key(key) {
            target.insert(key.clone(), value.clone());
            updated = true;
        }
    }
    updated
}

#[derive(Debug)]
pub enum SyncStatus {
    Created,
    Updated,
    Unchanged,
    /// The user's file could not be parsed; it was backed up and left untouched.
    Unparseable {
        backup: PathBuf,
        error: String,
    },
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Yaml(serde_yaml_ng::Error),
    NotAMapping,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "{e}"),
            ConfigError::Yaml(e) => write!(f, "{e}"),
            ConfigError::NotAMapping => write!(f, "config.yml is not a mapping"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<serde_yaml_ng::Error> for ConfigError {
    fn from(e: serde_yaml_ng::Error) -> Self {
        ConfigError::Yaml(e)
    }
}

/// Brings the user's config.yml up to date with keys added to the bundled default.
/// A user config that fails to parse is backed up and left untouched, never replaced.
pub fn sync_config_file(user_path: &Path, default_yaml: &str) -> Result<SyncStatus, ConfigError> {
    let default = match parse_yaml(default_yaml)? {
        Value::Object(map) => map,
        _ => Map::new(),
    };

    if !user_path.exists() {
        write_file_atomic(user_path, &to_yaml(&Value::Object(default))?)?;
        return Ok(SyncStatus::Created);
    }

    let text = fs::read_to_string(user_path)?;
    let mut user = match parse_yaml(&text) {
        Ok(Value::Object(map)) => map,
        Ok(Value::Null) => Map::new(),
        Ok(_) => {
            let backup = backup_file(user_path)?;
            return Ok(SyncStatus::Unparseable {
                backup,
                error: ConfigError::NotAMapping.to_string(),
            });
        }
        Err(e) => {
            let backup = backup_file(user_path)?;
            return Ok(SyncStatus::Unparseable {
                backup,
                error: e.to_string(),
            });
        }
    };

    if merge_missing_keys(&mut user, &default) {
        write_file_atomic(user_path, &to_yaml(&Value::Object(user))?)?;
        return Ok(SyncStatus::Updated);
    }
    Ok(SyncStatus::Unchanged)
}

/// Read-modify-write of config.yml. Fails (and writes nothing) if the file can't be
/// parsed.
pub fn update_config_file(
    path: &Path,
    mutate: impl FnOnce(&mut Map<String, Value>),
) -> Result<Map<String, Value>, ConfigError> {
    let mut config = match parse_yaml(&fs::read_to_string(path)?)? {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        _ => return Err(ConfigError::NotAMapping),
    };
    mutate(&mut config);
    write_file_atomic(path, &to_yaml(&Value::Object(config.clone()))?)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jagprox-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn merge_missing_keys_adds_without_overwriting() {
        let mut target = obj(json!({ "a": 1, "nested": { "x": 1 }, "arr": [1] }));
        let source = obj(json!({ "a": 2, "b": 3, "nested": { "x": 2, "y": 3 }, "arr": [9, 9] }));
        assert!(merge_missing_keys(&mut target, &source));
        assert_eq!(
            Value::Object(target.clone()),
            json!({ "a": 1, "nested": { "x": 1, "y": 3 }, "arr": [1], "b": 3 })
        );
        assert!(!merge_missing_keys(&mut target, &source));
    }

    #[test]
    fn merge_missing_keys_replaces_non_objects_with_objects() {
        let mut target = obj(json!({ "auto_gg": true }));
        assert!(merge_missing_keys(
            &mut target,
            &obj(json!({ "auto_gg": { "enabled": false } }))
        ));
        assert_eq!(target["auto_gg"], json!({ "enabled": false }));
    }

    #[test]
    fn sync_creates_updates_and_leaves_alone() {
        let dir = temp_dir("sync");
        let path = dir.join("config.yml");
        let default = "port: 2107\nauto_gg:\n  enabled: false\n";

        assert!(matches!(
            sync_config_file(&path, default).unwrap(),
            SyncStatus::Created
        ));
        fs::write(&path, "port: 1234\n").unwrap();
        assert!(matches!(
            sync_config_file(&path, default).unwrap(),
            SyncStatus::Updated
        ));
        let config = parse_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            config,
            json!({ "port": 1234, "auto_gg": { "enabled": false } })
        );
        assert!(matches!(
            sync_config_file(&path, default).unwrap(),
            SyncStatus::Unchanged
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sync_backs_up_an_unparseable_config() {
        let dir = temp_dir("broken");
        let path = dir.join("config.yml");
        fs::write(&path, "port: [unclosed\n").unwrap();
        match sync_config_file(&path, "port: 2107\n").unwrap() {
            SyncStatus::Unparseable { backup, .. } => {
                assert!(backup.exists());
                assert_eq!(
                    fs::read_to_string(&path).unwrap(),
                    "port: [unclosed\n",
                    "original left untouched"
                );
            }
            other => panic!("expected Unparseable, got {other:?}"),
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn numeric_keys_stay_names() {
        let config = parse_yaml("nicknames:\n  1234: Nick\n  Bob: B\n").unwrap();
        assert_eq!(config["nicknames"], json!({ "1234": "Nick", "Bob": "B" }));
    }

    #[test]
    fn the_shipped_default_config_parses() {
        let config = parse_yaml(include_str!("../../../config.yml")).unwrap();
        assert_eq!(config["port"], json!(2107));
        assert_eq!(config["queue_stats"]["bw"], json!(true));
    }
}
