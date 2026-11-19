//! `jagprox launch launcher`: starts the launcher window as its own process.

use std::path::PathBuf;
use std::process::{Command, Stdio};

const LAUNCHER: &str = if cfg!(windows) {
    "jagprox-launcher.exe"
} else {
    "jagprox-launcher"
};

/// Next to this binary (installers put both in one folder; macOS: inside the app
/// bundle), then anywhere on PATH.
fn find_launcher() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let beside = exe.parent()?.join(LAUNCHER);
    if beside.is_file() {
        return Some(beside);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(LAUNCHER))
            .find(|p| p.is_file())
    })
}

pub fn launch_launcher() -> Result<(), String> {
    let path = find_launcher().ok_or_else(|| {
        "the JagProx launcher isn't installed next to this command. Install the full JagProx package, \
         or use `jagprox launch proxy` to run the proxy in this terminal."
            .to_string()
    })?;
    let mut command = Command::new(&path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Detach, so closing the terminal doesn't close the launcher.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", path.display()))?;
    Ok(())
}
