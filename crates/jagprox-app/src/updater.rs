//! Launcher updates, as in the Electron launcher: the splash checks for a new release,
//! an "Update Available" window asks, and the update downloads with the splash's
//! progress bar, installs and restarts. The Settings button runs the same check.

use std::sync::Mutex;
use std::time::Duration;

use jagprox_core::install::{self, Install};
use jagprox_core::log::log;
use jagprox_core::update::{self, Plan, Release, Source, Version};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// The update the prompt is asking about.
#[derive(Default)]
pub struct Updates {
    pending: Mutex<Option<(Release, Plan)>>,
}

pub enum Check {
    /// Nothing to install; the text says why.
    Nothing(String),
    /// Something can be installed from here: the prompt is open.
    Prompted,
}

/// What the launcher can do itself. A .deb/.rpm needs sudo, so it's left to
/// `jagprox update` in a terminal.
fn launcher_plan(install: &Install, release: &Release) -> Plan {
    match update::plan(install, release) {
        plan @ (Plan::ReplaceAppImage { .. }
        | Plan::WindowsInstaller { .. }
        | Plan::ReplaceBundle { .. }) => plan,
        Plan::Package { .. } => Plan::Manual(
            "Run `jagprox update` in a terminal to install it with your package manager.".into(),
        ),
        Plan::ReplaceBinary { .. } => Plan::Manual(format!(
            "Download it from https://github.com/JagHack/jagprox/releases/tag/v{}",
            release.version
        )),
        Plan::Manual(text) => Plan::Manual(text),
    }
}

/// Looks for a newer release and opens the prompt if this install can take it.
pub async fn check(app: &AppHandle) -> Check {
    let install = install::detect();
    if install == Install::Dev && std::env::var_os("JAGPROX_RELEASES_URL").is_none() {
        return Check::Nothing("Development build: update checks are off.".into());
    }
    let current = Version::current();
    let release = match tokio::time::timeout(
        Duration::from_secs(8),
        update::fetch_newest(&Source::official(), &current),
    )
    .await
    {
        Ok(Ok(release)) => release,
        Ok(Err(e)) => {
            log(format!("[Updater] Update check failed: {e}"));
            return Check::Nothing("Update check failed. See logs for details.".into());
        }
        Err(_) => {
            log("[Updater] Update check timed out.");
            return Check::Nothing("Update check failed. See logs for details.".into());
        }
    };
    if release.version <= current {
        return Check::Nothing(format!("You're on the latest version (v{current})."));
    }
    log(format!(
        "[Updater] v{} is available ({}).",
        release.version,
        install.describe()
    ));
    match launcher_plan(&install, &release) {
        Plan::Manual(text) => {
            Check::Nothing(format!("Update v{} available! {text}", release.version))
        }
        plan => {
            let version = release.version.to_string();
            *app.state::<Updates>()
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some((release, plan));
            open_prompt(app, &version);
            Check::Prompted
        }
    }
}

fn open_prompt(app: &AppHandle, version: &str) {
    if let Some(window) = app.get_webview_window("update") {
        let _ = window.set_focus();
        return;
    }
    let parent = app
        .get_webview_window("splash")
        .or_else(|| app.get_webview_window("main"));
    let url = WebviewUrl::App(format!("update.html?v={version}").into());
    let mut builder = WebviewWindowBuilder::new(app, "update", url)
        .title("Update Available")
        .inner_size(450.0, 220.0)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .center();
    if let Some(parent) = &parent {
        builder = match builder.parent(parent) {
            Ok(b) => b,
            Err(e) => {
                log(format!("[Updater] {e}"));
                return;
            }
        };
    }
    if let Err(e) = builder.build() {
        log(format!("[Updater] Could not open the update window: {e}"));
    }
}

fn status(app: &AppHandle, text: impl Into<String>) {
    let _ = app.emit("update-status", text.into());
}

/// The prompt's answer. Returns false when the launcher should carry on as normal.
pub async fn respond(app: &AppHandle, install_now: bool) -> bool {
    if let Some(window) = app.get_webview_window("update") {
        let _ = window.close();
    }
    let pending = app
        .state::<Updates>()
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    let Some((release, plan)) = pending else {
        return false;
    };
    if !install_now {
        status(app, format!("Update v{} skipped.", release.version));
        return false;
    }
    match install(app, &release, plan).await {
        Ok(()) => true,
        Err(e) => {
            log(format!("[Updater] Update failed: {e}"));
            status(app, "Update failed. See logs for details.");
            false
        }
    }
}

async fn install(app: &AppHandle, release: &Release, plan: Plan) -> Result<(), String> {
    let asset = match &plan {
        Plan::ReplaceAppImage { asset, .. }
        | Plan::WindowsInstaller { asset }
        | Plan::ReplaceBundle { asset, .. } => asset.clone(),
        _ => return Err("this install can't update itself".into()),
    };
    status(app, "Downloading update...");
    let mut last = u64::MAX;
    let file = update::download(release, &asset, &update::download_dir(), |done, total| {
        let percent = (done * 100).checked_div(total).unwrap_or(0);
        if percent != last {
            last = percent;
            status(app, format!("Downloading update ({percent}%)"));
            let _ = app.emit("update-progress", percent);
        }
    })
    .await?;
    status(app, "Restarting...");
    log(format!("[Updater] Installing v{}.", release.version));
    match plan {
        #[cfg(unix)]
        Plan::ReplaceAppImage { path, .. } => {
            update::replace_appimage(&file, &path)?;
            relaunch(&path);
        }
        #[cfg(unix)]
        Plan::ReplaceBundle { bundle, .. } => {
            update::replace_bundle(&file, &bundle)?;
            let _ = std::fs::remove_file(&file);
            relaunch(&bundle);
        }
        // The installer closes this launcher and starts the new one (/R).
        Plan::WindowsInstaller { .. } => update::run_windows_installer(&file, true)?,
        _ => return Err("this install can't update itself".into()),
    }
    Ok(())
}

/// Starts the updated launcher a moment after this one has exited (otherwise the
/// single-instance check would hand the new one back to us).
#[cfg(unix)]
fn relaunch(path: &std::path::Path) {
    let opener = if cfg!(target_os = "macos") {
        "open \"$1\""
    } else {
        "\"$1\""
    };
    let script = format!("sleep 1; exec {opener}");
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    if let Err(e) = command.spawn() {
        log(format!("[Updater] Could not restart: {e}"));
    }
}
