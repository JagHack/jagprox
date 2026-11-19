//! The JagProx launcher window. The pages are the old Electron launcher's HTML/CSS
//! (in `ui/`); everything the Electron main process did is in `ipc.rs`, and the
//! proxy runs inside this process.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod discord;
mod ipc;
mod updater;

use std::io::Write;
use std::time::Duration;

use jagprox_core::account;
use jagprox_core::config::Config;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::discord::Discord;
use crate::ipc::AppState;
use crate::updater::{Check, Updates};

/// WebKitGTK's DMA-BUF renderer crashes on NVIDIA under Wayland ("Error 71 (Protocol
/// error) dispatching to Wayland display"), so it is turned off there unless the user
/// already chose a value.
#[cfg(target_os = "linux")]
fn work_around_webkit_nvidia() {
    const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(VAR).is_none() && std::path::Path::new("/proc/driver/nvidia").exists() {
        // SAFETY: called first thing in main, before any other thread exists.
        unsafe { std::env::set_var(VAR, "1") };
    }
}

/// On Wayland without a desktop settings daemon (Hyprland, Sway, ...) GTK reports the
/// font DPI as -1 ("unset"). WebKitGTK 2.52 divides that by 96 and renders the page at a
/// negative zoom, so everything collapses. Use the standard 96 DPI when it is unset.
#[cfg(target_os = "linux")]
fn fix_unset_gtk_dpi() {
    use gtk::prelude::*;
    if let Some(settings) = gtk::Settings::default()
        && settings.gtk_xft_dpi() <= 0
    {
        settings.set_gtk_xft_dpi(96 * 1024);
    }
}

/// `jagprox install-cli` links `jagprox` to the AppImage itself, so the AppImage
/// starts this binary for CLI commands too; those runs go to the bundled CLI.
#[cfg(target_os = "linux")]
fn run_cli_if_called_as_jagprox() {
    use std::os::unix::process::CommandExt;
    if std::env::var_os("APPIMAGE").is_none() {
        return;
    }
    let Some(argv0) = std::env::var_os("ARGV0") else {
        return;
    };
    if std::path::Path::new(&argv0).file_name() != Some("jagprox".as_ref()) {
        return;
    }
    let Some(cli) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("jagprox")))
    else {
        return;
    };
    let error = std::process::Command::new(&cli)
        .args(std::env::args_os().skip(1))
        .exec();
    eprintln!("jagprox: could not start {}: {error}", cli.display());
    std::process::exit(1);
}

/// Opens the launcher window and closes the splash.
pub fn open_main(handle: &tauri::AppHandle) {
    if let Some(main) = handle.get_webview_window("main") {
        let _ = main.set_focus();
        return;
    }
    let main = WebviewWindowBuilder::new(handle, "main", WebviewUrl::App("index.html".into()))
        .title("JagProx")
        .inner_size(1000.0, 650.0)
        .min_inner_size(760.0, 480.0)
        .decorations(false)
        .center()
        .build();
    if let Err(e) = main {
        jagprox_core::log::log(format!("[ERROR] Could not open the launcher window: {e}"));
    }
    if let Some(splash) = handle.get_webview_window("splash") {
        let _ = splash.close();
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    run_cli_if_called_as_jagprox();
    #[cfg(target_os = "linux")]
    work_around_webkit_nvidia();

    let data_dir = match account::data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("JagProx: {e}");
            std::process::exit(1);
        }
    };
    let _ = std::fs::create_dir_all(data_dir.join("logs"));
    let config = match Config::load(&data_dir) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("JagProx: could not load config.yml: {e}");
            std::process::exit(1);
        }
    };
    // New installs start with the bundled default aliases.
    let _ = jagprox_core::aliases::Aliases::load(&data_dir);

    let discord = Discord::start();
    if config.discord_rpc_enabled() {
        discord.set_enabled(true);
    }

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch just brings the running launcher to the front.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(AppState::new(data_dir.clone(), config.clone(), discord))
        .manage(Updates::default())
        .invoke_handler(tauri::generate_handler![ipc::ipc])
        .setup(move |app| {
            #[cfg(target_os = "linux")]
            fix_unset_gtk_dpi();

            // Log lines go to the Log Monitor and to logs/main.log.
            let handle = app.handle().clone();
            let log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(data_dir.join("logs/main.log"))
                .ok();
            let log_file = std::sync::Mutex::new(log_file);
            jagprox_core::log::set_sink(move |line| {
                if let Some(file) = log_file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    let _ = writeln!(file, "{line}");
                }
                let _ = handle.emit("proxy-log", line);
            });

            // First start after JagProx 2: keep its login, drop its Chromium files.
            let moved = jagprox_core::electron::migrate(&data_dir);
            if moved.login_imported {
                jagprox_core::log::log("Kept your JagProx 2 login.");
            }
            if moved.removed > 0 {
                jagprox_core::log::log(format!(
                    "Removed {} leftover files of JagProx 2.",
                    moved.removed
                ));
            }

            let watched = config.clone();
            tauri::async_runtime::spawn(async move {
                watched.watch();
            });

            WebviewWindowBuilder::new(app, "splash", WebviewUrl::App("splash.html".into()))
                .title("JagProx")
                .inner_size(400.0, 350.0)
                .decorations(false)
                .resizable(false)
                .center()
                .build()?;

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let _ = handle.emit("update-status", "Checking for updates...");
                match updater::check(&handle).await {
                    // The prompt's answer carries on (ipc `update-response`).
                    Check::Prompted => {}
                    Check::Nothing(text) => {
                        let _ = handle.emit("update-status", text);
                        tokio::time::sleep(Duration::from_millis(1200)).await;
                        open_main(&handle);
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("JagProx: {e}");
        std::process::exit(1);
    }
}
