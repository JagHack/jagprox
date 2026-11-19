//! `jagprox` command-line entry point.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};

/// Folder name shared with the Electron launcher, so existing settings carry over.
const DATA_DIR_NAME: &str = "jagprox-launcher";
const DEFAULT_CONFIG: &str = include_str!("../../../config.yml");

#[derive(Parser)]
#[command(name = "jagprox", version, about = "JagProx - a Hypixel proxy")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Launch the launcher window or the proxy on its own
    Launch {
        #[command(subcommand)]
        what: LaunchTarget,
    },
    /// Open config.yml in your editor
    Config,
    /// Install a specific JagProx version (e.g. 3.0.0)
    Version { version: String },
    /// Update to the latest JagProx version
    Update,
}

#[derive(Subcommand)]
enum LaunchTarget {
    /// Open the launcher window (default when no command is given)
    Launcher,
    /// Run the proxy headless in this terminal (Ctrl+C to stop)
    Proxy,
}

fn data_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|d| d.join(DATA_DIR_NAME))
        .ok_or_else(|| "could not find your user config folder".into())
}

fn open_config() -> Result<(), String> {
    let dir = data_dir()?;
    let path = dir.join("config.yml");
    if !path.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, DEFAULT_CONFIG).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("Created default config at {}", path.display());
    }

    // Terminal users with $EDITOR set get it in place; everyone else gets the OS default app.
    if let Some(editor) = std::env::var_os("EDITOR").filter(|e| !e.is_empty()) {
        let status = Command::new(&editor)
            .arg(&path)
            .status()
            .map_err(|e| format!("could not start {}: {e}", editor.to_string_lossy()))?;
        if !status.success() {
            return Err(format!("{} exited with {status}", editor.to_string_lossy()));
        }
    } else {
        open::that_detached(&path)
            .map_err(|e| format!("could not open {}: {e}", path.display()))?;
        println!("Opened {}", path.display());
    }
    Ok(())
}

fn not_yet(what: &str) -> Result<(), String> {
    Err(format!(
        "`jagprox {what}` is not available yet in the Rust build"
    ))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command.unwrap_or(Cmd::Launch {
        what: LaunchTarget::Launcher,
    }) {
        Cmd::Launch {
            what: LaunchTarget::Launcher,
        } => not_yet("launch launcher"),
        Cmd::Launch {
            what: LaunchTarget::Proxy,
        } => not_yet("launch proxy"),
        Cmd::Config => open_config(),
        Cmd::Version { .. } => not_yet("version"),
        Cmd::Update => not_yet("update"),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("jagprox: {e}");
            ExitCode::FAILURE
        }
    }
}
