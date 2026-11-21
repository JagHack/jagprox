//! `jagprox update`, `jagprox version <x.y.z>` and `jagprox install-cli`.

use std::io::{IsTerminal, Write};

use jagprox_core::install::{self, Install};
use jagprox_core::update::{self, Plan, Source, Version};

use crate::term::Spinner;

/// Installs the latest release, or `wanted` (which may be older: a downgrade).
pub async fn update(wanted: Option<&str>) -> Result<(), String> {
    let wanted = match wanted {
        Some(text) => Some(Version::parse(text).ok_or_else(|| {
            format!("`{text}` isn't a version number (for example: jagprox version 3.0.0)")
        })?),
        None => None,
    };
    if let Some(v) = &wanted {
        update::check_installable(v)?;
    }
    let current = Version::current();
    let install = install::detect();

    let spinner = Spinner::start(match &wanted {
        Some(v) => format!("Looking up v{v}..."),
        None => "Checking for updates...".to_string(),
    });
    let source = Source::official();
    let release = match &wanted {
        Some(v) => update::fetch(&source, Some(v)).await,
        None => update::fetch_newest(&source, &current).await,
    };
    spinner.stop();
    let release = release?;

    match &wanted {
        None if release.version <= current => {
            println!("You're on the latest version (v{current}).");
            return Ok(());
        }
        Some(v) if *v == current => {
            println!("v{current} is already installed.");
            return Ok(());
        }
        _ => {}
    }
    let downgrade = release.version < current;
    let plan = update::plan(&install, &release);
    let asset = match &plan {
        Plan::Manual(text) => {
            println!("JagProx v{} is available. {text}", release.version);
            return Ok(());
        }
        Plan::ReplaceBinary { asset, .. }
        | Plan::ReplaceAppImage { asset, .. }
        | Plan::Package { asset, .. }
        | Plan::WindowsInstaller { asset }
        | Plan::ReplaceBundle { asset, .. } => asset.clone(),
    };
    println!(
        "{} v{current} -> v{} ({})",
        if downgrade { "Downgrading" } else { "Updating" },
        release.version,
        install.describe()
    );

    let show_progress = std::io::stderr().is_terminal();
    let file = update::download(&release, &asset, &update::download_dir(), |done, total| {
        if show_progress {
            let percent = (done * 100).checked_div(total).unwrap_or(0);
            eprint!(
                "\rDownloading {}  {percent:>3}%  {:.1}/{:.1} MB",
                asset.name,
                done as f64 / 1e6,
                total as f64 / 1e6
            );
            let _ = std::io::stderr().flush();
        }
    })
    .await;
    if show_progress {
        eprintln!();
    }
    let file = file?;

    match plan {
        Plan::Manual(_) => unreachable!("handled above"),
        #[cfg(unix)]
        Plan::ReplaceBinary { exe, .. } => {
            update::replace_binary(&file, &exe)?;
            let _ = std::fs::remove_file(&file);
            println!("Done: jagprox is now v{}.", release.version);
        }
        #[cfg(unix)]
        Plan::ReplaceAppImage { path, .. } => {
            update::replace_appimage(&file, &path)?;
            println!(
                "Done: {} is now v{}. Restart the launcher if it's open.",
                path.display(),
                release.version
            );
        }
        #[cfg(unix)]
        Plan::ReplaceBundle { bundle, .. } => {
            update::replace_bundle(&file, &bundle)?;
            let _ = std::fs::remove_file(&file);
            println!(
                "Done: {} is now v{}. Restart the launcher if it's open.",
                bundle.display(),
                release.version
            );
        }
        #[cfg(not(unix))]
        Plan::ReplaceBinary { .. } | Plan::ReplaceAppImage { .. } | Plan::ReplaceBundle { .. } => {
            return Err("this kind of update isn't possible on this system".into());
        }
        Plan::Package { manager, .. } => {
            let command = manager.command(&file, downgrade);
            println!("Running: {}", command.join(" "));
            let status = std::process::Command::new(&command[0])
                .args(&command[1..])
                .status()
                .map_err(|e| format!("could not run {}: {e}", command[0]))?;
            if !status.success() {
                return Err(format!(
                    "the package manager didn't finish; the package is at {}",
                    file.display()
                ));
            }
            let _ = std::fs::remove_file(&file);
            println!("Done: JagProx is now v{}.", release.version);
        }
        Plan::WindowsInstaller { .. } => {
            // Printed first: the installer closes every running jagprox, this one too.
            println!("Starting the installer; it closes the launcher and any running jagprox.");
            update::run_windows_installer(&file, false)?;
        }
    }
    Ok(())
}

fn ask(question: &str) -> bool {
    if !std::io::stdin().is_terminal() {
        return false;
    }
    print!("{question} [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    !matches!(answer.trim().to_ascii_lowercase().as_str(), "n" | "no")
}

/// Puts `jagprox` on PATH for the AppImage and the macOS app (packages and the Windows
/// installer already do).
pub fn install_cli() -> Result<(), String> {
    let install = install::detect();
    let target = match install::cli_target(&install) {
        Ok(target) => target,
        Err(message) if matches!(install, Install::Unknown | Install::Dev) => {
            return Err(message);
        }
        Err(message) => {
            println!("Nothing to do: {message}");
            return Ok(());
        }
    };
    let link = install::link_path().ok_or("could not find your home folder")?;
    link_cli(&target, &link)?;

    if !cfg!(target_os = "macos")
        && let Some(dir) = link.parent()
        && !install::on_path(dir)
    {
        let Some(fix) = install::current_path_fix() else {
            return Ok(());
        };
        println!("{} isn't on your PATH yet.", dir.display());
        if ask(&format!("Add it in {}?", fix.file.display())) {
            if install::apply_path_fix(&fix)? {
                println!("Added. Open a new terminal, then run: jagprox --help");
            } else {
                println!(
                    "It's already in {}; open a new terminal.",
                    fix.file.display()
                );
            }
        } else {
            println!(
                "Add this line to your shell's startup file:\n  {}",
                fix.line
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
fn link_cli(target: &std::path::Path, link: &std::path::Path) -> Result<(), String> {
    match install::link_cli(target, link) {
        Ok(install::Linked::Created(link)) => {
            println!("Installed: {} -> {}", link.display(), target.display());
            Ok(())
        }
        Ok(install::Linked::AlreadyThere(link)) => {
            println!("jagprox is already installed at {}.", link.display());
            Ok(())
        }
        #[cfg(target_os = "macos")]
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            println!("Creating {} needs your password.", link.display());
            install::link_cli_as_admin(target, link, false)?;
            println!("Installed: {} -> {}", link.display(), target.display());
            Ok(())
        }
        Err(e) => Err(format!("could not create {}: {e}", link.display())),
    }
}

#[cfg(not(unix))]
fn link_cli(_target: &std::path::Path, _link: &std::path::Path) -> Result<(), String> {
    Err("the installer puts jagprox on PATH on this system.".into())
}
