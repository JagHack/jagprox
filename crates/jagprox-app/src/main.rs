//! The JagProx launcher window. The pages are the old Electron launcher's HTML/CSS
//! (in `ui/`); everything the Electron main process did is in `ipc.rs`, and the
//! proxy runs inside this process.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod discord;
mod ipc;

use std::io::Write;
use std::time::Duration;

use jagprox_core::account;
use jagprox_core::config::Config;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::discord::Discord;
use crate::ipc::AppState;

fn main() {
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
        .invoke_handler(tauri::generate_handler![ipc::ipc])
        .setup(move |app| {
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
                let _ = handle.emit("update-status", "Launching JagProx...");
                tokio::time::sleep(Duration::from_millis(1200)).await;
                let main = WebviewWindowBuilder::new(
                    &handle,
                    "main",
                    WebviewUrl::App("index.html".into()),
                )
                .title("JagProx")
                .inner_size(1000.0, 650.0)
                .min_inner_size(760.0, 480.0)
                .decorations(false)
                .center()
                .build();
                if let Err(e) = main {
                    jagprox_core::log::log(format!(
                        "[ERROR] Could not open the launcher window: {e}"
                    ));
                }
                if let Some(splash) = handle.get_webview_window("splash") {
                    let _ = splash.close();
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
