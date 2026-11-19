//! `jagprox __windows-path add|remove <dir>`: the Windows installer's hook for putting
//! its folder on the user's PATH (`HKCU\Environment\Path`). The value stays
//! REG_EXPAND_SZ so entries like `%USERPROFILE%\bin` keep working.

use std::ptr::{null, null_mut};

use jagprox_core::install::{windows_path_add, windows_path_remove};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ, RegCloseKey,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn open_environment() -> Result<Key, String> {
    let mut key: HKEY = null_mut();
    let name = wide("Environment");
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            name.as_ptr(),
            0,
            KEY_READ | KEY_WRITE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not open HKCU\\Environment (error {status})"));
    }
    Ok(Key(key))
}

/// The user's PATH and its registry type (missing: empty, REG_EXPAND_SZ).
fn read_path(key: &Key) -> Result<(String, u32), String> {
    let name = wide("Path");
    let mut kind = 0u32;
    let mut size = 0u32;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            null_mut(),
            &mut size,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok((String::new(), REG_EXPAND_SZ));
    }
    if status != ERROR_SUCCESS {
        return Err(format!("could not read the user PATH (error {status})"));
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2) + 1];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not read the user PATH (error {status})"));
    }
    buffer.truncate(size as usize / 2);
    let text = String::from_utf16_lossy(&buffer);
    Ok((text.trim_end_matches('\0').to_string(), kind))
}

fn write_path(key: &Key, value: &str, kind: u32) -> Result<(), String> {
    let kind = if kind == REG_SZ {
        REG_SZ
    } else {
        REG_EXPAND_SZ
    };
    let name = wide("Path");
    let data = wide(value);
    let status = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            kind,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not write the user PATH (error {status})"));
    }
    // Tell Explorer, so new terminals see the change without signing out.
    let environment = wide("Environment");
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            environment.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        )
    };
    Ok(())
}

pub fn run(add: bool, dir: &str) -> Result<(), String> {
    let key = open_environment()?;
    let (current, kind) = read_path(&key)?;
    let changed = if add {
        windows_path_add(&current, dir)
    } else {
        windows_path_remove(&current, dir)
    };
    if let Some(value) = changed {
        write_path(&key, &value, kind)?;
    }
    Ok(())
}
