//! In-app updates: check the latest release and, for installs that can replace
//! themselves (AppImage, macOS app, Windows installer), download and install it.
//! Other installs (package managers, Nix) are told how to update instead. Progress
//! goes to the Log Monitor rather than a modal.

use std::sync::Arc;

use jagprox_core::install::{self, Install};
use jagprox_core::update::{self, Plan, Release, Source, Version};

use crate::Shared;

/// Checks for a newer release; `quiet` stays silent when already up to date
/// (used for the automatic check on startup).
pub fn check(shared: &Arc<Shared>, quiet: bool) {
    let shared = shared.clone();
    let rt = shared.rt.clone();
    rt.spawn(async move {
        let install = install::detect();
        if install == Install::Dev && std::env::var_os("JAGPROX_RELEASES_URL").is_none() {
            if !quiet {
                shared.push_log("§7[Update] Development build — update checks are off.");
            }
            return;
        }
        let current = Version::current();
        let release = match update::fetch_newest(&Source::official(), &current).await {
            Ok(r) => r,
            Err(e) => {
                if !quiet {
                    shared.push_log(format!("§c[Update] Check failed: {e}"));
                }
                return;
            }
        };
        if release.version <= current {
            if !quiet {
                shared.push_log(format!(
                    "§a[Update] You're on the latest version (v{current})."
                ));
            }
            return;
        }
        shared.push_log(format!("§e[Update] v{} is available.", release.version));
        install_plan(&shared, &release, update::plan(&install, &release)).await;
    });
}

async fn install_plan(shared: &Arc<Shared>, release: &Release, plan: Plan) {
    let asset = match &plan {
        Plan::ReplaceAppImage { asset, .. }
        | Plan::WindowsInstaller { asset }
        | Plan::ReplaceBundle { asset, .. } => asset.clone(),
        Plan::Package { .. } => {
            shared.push_log("§e[Update] Run `jagprox update` in a terminal to install it.");
            return;
        }
        Plan::ReplaceBinary { .. } => {
            shared.push_log(format!(
                "§e[Update] Download v{} from github.com/JagHack/jagprox/releases",
                release.version
            ));
            return;
        }
        Plan::Manual(text) => {
            shared.push_log(format!("§e[Update] {text}"));
            return;
        }
    };
    shared.push_log("§7[Update] Downloading…");
    let file = match update::download(release, &asset, &update::download_dir(), |_, _| {}).await {
        Ok(f) => f,
        Err(e) => {
            shared.push_log(format!("§c[Update] {e}"));
            return;
        }
    };
    shared.push_log(format!(
        "§7[Update] Installing v{} and restarting…",
        release.version
    ));
    let outcome: Result<(), String> = match plan {
        #[cfg(unix)]
        Plan::ReplaceAppImage { path, .. } => {
            update::replace_appimage(&file, &path).map(|()| relaunch(&path))
        }
        #[cfg(unix)]
        Plan::ReplaceBundle { bundle, .. } => update::replace_bundle(&file, &bundle).map(|()| {
            let _ = std::fs::remove_file(&file);
            relaunch(&bundle);
        }),
        Plan::WindowsInstaller { .. } => update::run_windows_installer(&file, true),
        _ => Err("this install can't update itself".into()),
    };
    match outcome {
        Ok(()) => {
            shared.stop_proxy();
            shared.discord.set_enabled(false);
            std::process::exit(0);
        }
        Err(e) => shared.push_log(format!("§c[Update] {e}")),
    }
}

/// Starts the updated launcher a moment after this one exits.
#[cfg(unix)]
fn relaunch(path: &std::path::Path) {
    use std::os::unix::process::CommandExt;
    let opener = if cfg!(target_os = "macos") {
        "open \"$1\""
    } else {
        "\"$1\""
    };
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg(format!("sleep 1; exec {opener}"))
        .arg("sh")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let _ = command.spawn();
}
