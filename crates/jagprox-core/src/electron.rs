//! Moving over from JagProx 2 (Electron), which shared this data folder: its saved
//! login is carried over, then its leftover Chromium files are deleted.
//!
//! The Electron launcher kept the login token in Chromium's localStorage, a LevelDB
//! database in `Local Storage/leveldb`. The small reader below handles the two file
//! kinds in there: the write-ahead `.log` and the (snappy-compressed) `.ldb` tables.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::account;

/// Chromium caches Electron left in the data folder: rebuilt if JagProx 2 runs again,
/// so they're always deleted. JagProx 3 creates none of these (its web view keeps its
/// data elsewhere).
const ELECTRON_CACHES: &[&str] = &[
    "Cache",
    "Code Cache",
    "Crashpad",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "GPUCache",
    "GPUPersistentCache",
    "GraphiteDawnCache",
    "GrShaderCache",
    "ShaderCache",
    "VideoDecodeStats",
];

/// JagProx 2's own state (its login, cookies, settings): only deleted once JagProx 2
/// is uninstalled, so running both never logs the old one out.
const ELECTRON_STATE: &[&str] = &[
    "blob_storage",
    "Cookies",
    "Cookies-journal",
    "databases",
    "declarative_performance_observer.db",
    "declarative_performance_observer.db-journal",
    "Dictionaries",
    "DIPS",
    "DIPS-journal",
    "IndexedDB",
    "Local State",
    "Local Storage",
    "Network",
    "Network Persistent State",
    "Partitions",
    "Preferences",
    "QuotaManager",
    "QuotaManager-journal",
    "Service Worker",
    "Session Storage",
    "Shared Dictionary",
    "SharedStorage",
    "TransportSecurity",
    "Trust Tokens",
    "Trust Tokens-journal",
    "WebStorage",
    ".updaterId",
];

/// electron-builder's uninstall key for JagProx 2 (UUID v5 of com.jaghack.jagprox).
#[cfg(windows)]
const ELECTRON_UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\59aab638-ecaa-5aab-9b5b-9a5e6fed9139";

/// Whether JagProx 2 may still be installed. Windows can tell from its uninstall
/// entry (the JagProx 3 installer removes it); elsewhere JagProx 2 was an AppImage
/// that could be anywhere, so assume it might be.
fn electron_maybe_installed() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("reg")
            .args(["query", ELECTRON_UNINSTALL_KEY])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_or(true, |s| s.success())
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// The localStorage key of the token: origin `file://`, then the key as Latin-1.
const TOKEN_KEY: &[u8] = b"_file://\x00\x01jwt_token";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Migration {
    /// The JagProx 2 login was saved for JagProx 3.
    pub login_imported: bool,
    /// How many Electron files and folders were deleted.
    pub removed: usize,
}

/// Runs once after the move from JagProx 2; does nothing when there's nothing left of
/// it. Never replaces a login JagProx 3 already has.
pub fn migrate(data_dir: &Path) -> Migration {
    migrate_with(
        data_dir,
        dirs::cache_dir().as_deref(),
        electron_maybe_installed,
        || account::load().is_some(),
        |token| account::save(token).is_ok(),
    )
}

/// `migrate` with the keychain and cache folder passed in (for tests).
fn migrate_with(
    data_dir: &Path,
    cache_dir: Option<&Path>,
    electron_installed: impl FnOnce() -> bool,
    has_login: impl FnOnce() -> bool,
    save_login: impl FnOnce(&str) -> bool,
) -> Migration {
    let mut result = Migration::default();
    if let Some(token) = saved_login(&data_dir.join("Local Storage").join("leveldb"))
        && !has_login()
    {
        result.login_imported = save_login(&token);
    }
    // electron-updater's download folder (it kept the last installer). During the
    // move this may still hold the running installer; it goes on a later start.
    let updater = cache_dir
        .map(|cache| cache.join(format!("{}-updater", account::DATA_DIR_NAME)))
        .filter(|dir| dir.is_dir());
    // Only look for the uninstall entry when there's something of JagProx 2's to clear.
    let state_left = ELECTRON_STATE.iter().any(|n| data_dir.join(n).exists());
    let clear_state = (state_left || updater.is_some()) && !electron_installed();
    let names = ELECTRON_CACHES
        .iter()
        .chain(ELECTRON_STATE.iter().filter(|_| clear_state));
    for name in names {
        let path = data_dir.join(name);
        let removed = match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => fs::remove_dir_all(&path).is_ok(),
            Ok(_) => fs::remove_file(&path).is_ok(),
            Err(_) => false,
        };
        result.removed += usize::from(removed);
    }
    if clear_state
        && let Some(updater) = updater
        && fs::remove_dir_all(&updater).is_ok()
    {
        result.removed += 1;
    }
    result
}

/// The token JagProx 2 had saved, if it was still logged in.
pub fn saved_login(leveldb: &Path) -> Option<String> {
    let entries = read_leveldb(leveldb);
    let (_, value) = entries.get(TOKEN_KEY)?;
    let value = value.as_ref()?;
    let token = match value.split_first()? {
        // Latin-1
        (1, bytes) => bytes.iter().map(|&b| b as char).collect::<String>(),
        // UTF-16LE
        (0, bytes) => String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
        .ok()?,
        _ => return None,
    };
    account::looks_like_jwt(&token).then_some(token)
}

/// Every key's newest state: (sequence number, value or `None` if deleted).
type Entries = HashMap<Vec<u8>, (u64, Option<Vec<u8>>)>;

fn read_leveldb(dir: &Path) -> Entries {
    let mut entries = Entries::new();
    let Ok(listing) = fs::read_dir(dir) else {
        return entries;
    };
    for file in listing.flatten() {
        let path = file.path();
        let Ok(data) = fs::read(&path) else {
            continue;
        };
        match path.extension().and_then(|e| e.to_str()) {
            Some("log") => read_log(&data, &mut entries),
            Some("ldb" | "sst") => {
                let _ = read_table(&data, &mut entries);
            }
            _ => {}
        }
    }
    entries
}

fn keep(entries: &mut Entries, key: &[u8], seq: u64, value: Option<Vec<u8>>) {
    match entries.get(key) {
        Some((known, _)) if *known >= seq => {}
        _ => {
            entries.insert(key.to_vec(), (seq, value));
        }
    }
}

fn varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn bytes<'a>(data: &'a [u8], pos: &mut usize, len: usize) -> Option<&'a [u8]> {
    let slice = data.get(*pos..pos.checked_add(len)?)?;
    *pos += len;
    Some(slice)
}

fn length_prefixed<'a>(data: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    let len = usize::try_from(varint(data, pos)?).ok()?;
    bytes(data, pos, len)
}

/// The write-ahead log: 32 KiB blocks of records, which join into write batches.
fn read_log(data: &[u8], entries: &mut Entries) {
    const BLOCK: usize = 32 * 1024;
    let mut batch = Vec::new();
    let mut block_start = 0;
    while block_start < data.len() {
        let block = &data[block_start..data.len().min(block_start + BLOCK)];
        let mut pos = 0;
        while pos + 7 <= block.len() {
            let len = usize::from(u16::from_le_bytes([block[pos + 4], block[pos + 5]]));
            let kind = block[pos + 6];
            let Some(payload) = block.get(pos + 7..pos + 7 + len) else {
                break;
            };
            pos += 7 + len;
            match kind {
                // full
                1 => read_batch(payload, entries),
                // first
                2 => batch = payload.to_vec(),
                // middle
                3 => batch.extend_from_slice(payload),
                // last
                4 => {
                    batch.extend_from_slice(payload);
                    read_batch(&batch, entries);
                    batch.clear();
                }
                _ => break,
            }
        }
        block_start += BLOCK;
    }
}

fn read_batch(batch: &[u8], entries: &mut Entries) {
    if batch.len() < 12 {
        return;
    }
    let mut seq = u64::from_le_bytes(batch[..8].try_into().expect("8 bytes"));
    let mut pos = 12;
    while pos < batch.len() {
        let tag = batch[pos];
        pos += 1;
        let Some(key) = length_prefixed(batch, &mut pos) else {
            return;
        };
        let value = match tag {
            1 => match length_prefixed(batch, &mut pos) {
                Some(v) => Some(v.to_vec()),
                None => return,
            },
            0 => None,
            _ => return,
        };
        keep(entries, key, seq, value);
        seq += 1;
    }
}

/// A sorted table: footer → index block → data blocks.
fn read_table(data: &[u8], entries: &mut Entries) -> Option<()> {
    let footer = data.get(data.len().checked_sub(48)?..)?;
    let mut pos = 0;
    varint(footer, &mut pos)?; // metaindex offset
    varint(footer, &mut pos)?; // metaindex size
    let index_offset = usize::try_from(varint(footer, &mut pos)?).ok()?;
    let index_size = usize::try_from(varint(footer, &mut pos)?).ok()?;
    let index = table_block(data, index_offset, index_size)?;
    for (_, handle) in block_entries(&index)? {
        let mut pos = 0;
        let offset = usize::try_from(varint(&handle, &mut pos)?).ok()?;
        let size = usize::try_from(varint(&handle, &mut pos)?).ok()?;
        let block = table_block(data, offset, size)?;
        for (internal_key, value) in block_entries(&block)? {
            let split = internal_key.len().checked_sub(8)?;
            let (key, trailer) = internal_key.split_at(split);
            let trailer = u64::from_le_bytes(trailer.try_into().ok()?);
            let value = (trailer & 0xff == 1).then_some(value);
            keep(entries, key, trailer >> 8, value);
        }
    }
    Some(())
}

/// A block's bytes, decompressed if its trailer says snappy.
fn table_block(data: &[u8], offset: usize, size: usize) -> Option<Vec<u8>> {
    let raw = data.get(offset..offset.checked_add(size)?)?;
    match *data.get(offset + size)? {
        0 => Some(raw.to_vec()),
        1 => snap::raw::Decoder::new().decompress_vec(raw).ok(),
        _ => None,
    }
}

/// The (key, value) pairs of a block, undoing the shared-prefix key compression.
fn block_entries(block: &[u8]) -> Option<Vec<(Vec<u8>, Vec<u8>)>> {
    let restarts = u32::from_le_bytes(block.get(block.len().checked_sub(4)?..)?.try_into().ok()?);
    let end = block
        .len()
        .checked_sub(4 + 4 * usize::try_from(restarts).ok()?)?;
    let mut result = Vec::new();
    let mut key: Vec<u8> = Vec::new();
    let mut pos = 0;
    while pos < end {
        let shared = usize::try_from(varint(block, &mut pos)?).ok()?;
        let unshared = usize::try_from(varint(block, &mut pos)?).ok()?;
        let value_len = usize::try_from(varint(block, &mut pos)?).ok()?;
        key.truncate(shared);
        key.extend_from_slice(bytes(block, &mut pos, unshared)?);
        let value = bytes(block, &mut pos, value_len)?.to_vec();
        result.push((key.clone(), value));
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/electron")
            .join(name)
            .join("Local Storage/leveldb")
    }

    const FIRST: &str = "eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyIjoiRmlyc3QIn0.c2lnbmF0dXJlRmlyc3Q";

    #[test]
    fn reads_the_saved_login_from_the_log() {
        assert_eq!(saved_login(&fixture("login")).as_deref(), Some(FIRST));
    }

    #[test]
    fn the_newest_login_wins() {
        assert_eq!(
            saved_login(&fixture("relogin")).as_deref(),
            Some("eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyIjoiU2Vjb25kIn0.c2lnbmF0dXJlU2Vjb25k")
        );
    }

    #[test]
    fn a_logged_out_profile_has_no_login() {
        assert_eq!(saved_login(&fixture("logout")), None);
    }

    #[test]
    fn reads_compressed_tables() {
        // 8 MB of other keys pushed everything into a snappy-compressed .ldb table.
        assert_eq!(
            saved_login(&fixture("big")).as_deref(),
            Some("eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyIjoiQmlnZ2VyIn0.c2lnbmF0dXJlQmlnZ2Vy")
        );
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// A data folder as JagProx 2 leaves it, plus what JagProx 3 keeps there.
    fn electron_data_dir(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("jagprox-electron-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("jagprox-launcher");
        copy_dir(&fixture("login"), &data.join("Local Storage/leveldb"));
        for dir in ["Cache/Cache_Data", "GPUCache", "Code Cache/js", "logs"] {
            fs::create_dir_all(data.join(dir)).unwrap();
        }
        for file in [
            "Local State",
            "Preferences",
            ".updaterId",
            "config.yml",
            "aliases.json",
            "logs/main.log",
        ] {
            fs::write(data.join(file), "x").unwrap();
        }
        fs::create_dir_all(root.join("cache/jagprox-launcher-updater/pending")).unwrap();
        root
    }

    #[test]
    fn migration_imports_the_login_and_clears_electron_files() {
        let root = electron_data_dir("fresh");
        let data = root.join("jagprox-launcher");
        let mut saved = None;
        let result = migrate_with(
            &data,
            Some(&root.join("cache")),
            || false,
            || false,
            |t| {
                saved = Some(t.to_string());
                true
            },
        );
        assert!(result.login_imported);
        assert_eq!(saved.as_deref(), Some(FIRST));
        // Local Storage, Cache, GPUCache, Code Cache, Local State, Preferences,
        // .updaterId and the updater folder.
        assert_eq!(result.removed, 8);
        let mut left: Vec<String> = fs::read_dir(&data)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["aliases.json", "config.yml", "logs"]);
        assert!(data.join("logs/main.log").exists());
        assert!(!root.join("cache/jagprox-launcher-updater").exists());

        // Running again finds nothing.
        let again = migrate_with(
            &data,
            Some(&root.join("cache")),
            || false,
            || false,
            |_| panic!("no login left"),
        );
        assert_eq!(again, Migration::default());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migration_keeps_an_existing_jagprox_3_login() {
        let root = electron_data_dir("loggedin");
        let data = root.join("jagprox-launcher");
        let result = migrate_with(
            &data,
            None,
            || false,
            || true,
            |_| panic!("must not replace the login"),
        );
        assert!(!result.login_imported);
        assert!(!data.join("Local Storage").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn while_jagprox_2_is_installed_only_caches_go() {
        let root = electron_data_dir("installed");
        let data = root.join("jagprox-launcher");
        let mut saved = None;
        let result = migrate_with(
            &data,
            Some(&root.join("cache")),
            || true,
            || false,
            |t| {
                saved = Some(t.to_string());
                true
            },
        );
        // The login is copied (the old app keeps it too), only caches are deleted.
        assert!(result.login_imported);
        assert_eq!(saved.as_deref(), Some(FIRST));
        assert_eq!(result.removed, 3); // Cache, Code Cache, GPUCache
        for kept in ["Local Storage", "Local State", "Preferences", ".updaterId"] {
            assert!(data.join(kept).exists(), "{kept} was deleted");
        }
        assert!(root.join("cache/jagprox-launcher-updater").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_updater_folder_is_retried_on_its_own() {
        // The installer ran the migration while it was itself in the updater folder,
        // so only that folder is left.
        let root =
            std::env::temp_dir().join(format!("jagprox-electron-updater-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("jagprox-launcher");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(root.join("cache/jagprox-launcher-updater/pending")).unwrap();
        let installed = migrate_with(
            &data,
            Some(&root.join("cache")),
            || true,
            || false,
            |_| false,
        );
        assert_eq!(installed.removed, 0);
        let gone = migrate_with(
            &data,
            Some(&root.join("cache")),
            || false,
            || false,
            |_| false,
        );
        assert_eq!(gone.removed, 1);
        assert!(!root.join("cache/jagprox-launcher-updater").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_electron_folder_means_nothing() {
        assert_eq!(saved_login(Path::new("/nonexistent")), None);
    }
}
