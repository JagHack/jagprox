//! `jagprox` command-line entry point.

mod commands;
mod launch;
mod login;
mod proxy_cmd;
mod term;
mod update_cmd;
#[cfg(windows)]
mod windows_path;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use commands::{Ctx, GametrackPeriod};

const DEFAULT_CONFIG: &str = include_str!("../../../config.yml");

#[derive(Parser)]
#[command(name = "jagprox", version, about = "JagProx - a Hypixel proxy", after_help = EXAMPLES)]
struct Cli {
    /// Print raw JSON instead of formatted text (stats, status, profile, gametrack)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Cmd>,
}

const EXAMPLES: &str = "Examples:
  jagprox stats Notch            Bed Wars stats for Notch
  jagprox stats Notch sumo       Sumo Duels stats
  jagprox status Notch           Is Notch online, and where?
  jagprox login                  Log in to your JagProx account";

#[derive(Subcommand)]
enum Cmd {
    /// Hypixel stats for a player (default game: Bed Wars)
    #[command(visible_alias = "sc")]
    Stats {
        player: String,
        /// Game mode, e.g. bw, skywars, sumo, "mega walls" (see `jagprox modes`)
        mode: Vec<String>,
    },
    /// Is a player online, and in which game?
    Status { player: String },
    /// Duels wins and losses per mode
    Spread { player: String },
    /// Duels titles per mode
    Jtitles { player: String },
    /// Players in a Duels mode right now (no mode: list the modes)
    Playercount { mode: Vec<String> },
    /// Your tracked wins and losses
    #[command(visible_alias = "gt")]
    Gametrack {
        #[command(subcommand)]
        period: Option<Period>,
    },
    /// List the game modes `stats` understands
    Modes,
    /// Log in to your JagProx account
    Login {
        /// Log in with email and password instead of the browser (for SSH / no browser)
        #[arg(long, value_name = "EMAIL", num_args = 0..=1)]
        email: Option<Option<String>>,
    },
    /// Log out and forget the saved login
    Logout,
    /// Show your JagProx account
    Profile,
    /// Show or set the Hypixel API key stored on your JagProx account
    Key {
        /// The new key to save
        api_key: Option<String>,
        /// Print the full key instead of a masked one
        #[arg(long)]
        show: bool,
    },
    /// Link your Minecraft account to JagProx
    Link,
    /// Microsoft accounts the proxy logs in to Hypixel with
    Account {
        #[command(subcommand)]
        action: AccountAction,
    },
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
    /// Put the jagprox command on PATH (for the AppImage and the macOS app)
    InstallCli,
    /// Used by the Windows installer: add or remove its folder on the user PATH
    #[command(name = "__windows-path", hide = true)]
    WindowsPath { action: PathAction, dir: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PathAction {
    Add,
    Remove,
}

#[derive(Subcommand, Clone)]
enum Period {
    /// The last N hours (default 1)
    Hour { hours: Option<u32> },
    /// Today
    Day,
    /// Your 10 most recent games
    Log,
}

#[derive(Subcommand)]
enum LaunchTarget {
    /// Open the launcher window (default when no command is given)
    Launcher,
    /// Run the proxy headless in this terminal (Ctrl+C to stop)
    Proxy {
        /// Also print the in-game chat
        #[arg(long)]
        chat: bool,
    },
}

#[derive(Subcommand)]
enum AccountAction {
    /// Sign in a Microsoft account
    Add,
    /// List signed-in accounts
    List,
    /// Sign out an account
    Remove { name: String },
}

async fn run(cmd: Cmd, ctx: &Ctx) -> Result<(), String> {
    match cmd {
        Cmd::Stats { player, mode } => commands::stats(ctx, &player, &mode).await,
        Cmd::Status { player } => commands::status(ctx, &player).await,
        Cmd::Spread { player } => commands::spread(ctx, &player).await,
        Cmd::Jtitles { player } => commands::jtitles(ctx, &player).await,
        Cmd::Playercount { mode } => commands::playercount(ctx, &mode).await,
        Cmd::Gametrack { period } => {
            let period = match period.unwrap_or(Period::Hour { hours: None }) {
                Period::Hour { hours: Some(0) } => return Err("invalid number of hours.".into()),
                Period::Hour { hours } => GametrackPeriod::Hours(hours.unwrap_or(1)),
                Period::Day => GametrackPeriod::Day,
                Period::Log => GametrackPeriod::Log,
            };
            commands::gametrack(ctx, period).await
        }
        Cmd::Modes => commands::modes(ctx),
        Cmd::Login { email } => commands::login(email).await,
        Cmd::Logout => commands::logout(),
        Cmd::Profile => commands::profile(ctx).await,
        Cmd::Key { api_key, show } => commands::key(api_key, show).await,
        Cmd::Link => commands::link(),
        Cmd::Launch {
            what: LaunchTarget::Launcher,
        } => launch::launch_launcher(),
        Cmd::Launch {
            what: LaunchTarget::Proxy { chat },
        } => proxy_cmd::run_proxy(ctx.color, chat).await,
        Cmd::Account {
            action: AccountAction::Add,
        } => proxy_cmd::account_add(ctx.color).await,
        Cmd::Account {
            action: AccountAction::List,
        } => proxy_cmd::account_list(),
        Cmd::Account {
            action: AccountAction::Remove { name },
        } => proxy_cmd::account_remove(&name),
        Cmd::Config => commands::open_config(DEFAULT_CONFIG),
        Cmd::Version { version } => update_cmd::update(Some(&version)).await,
        Cmd::Update => update_cmd::update(None).await,
        Cmd::InstallCli => update_cmd::install_cli(),
        #[cfg(windows)]
        Cmd::WindowsPath { action, dir } => {
            windows_path::run(matches!(action, PathAction::Add), &dir)
        }
        #[cfg(not(windows))]
        Cmd::WindowsPath { .. } => Err("this is only for the Windows installer".into()),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ctx = Ctx {
        color: term::detect(),
        json: cli.json,
    };
    // Library log lines are diagnostics; the CLI only shows them with JAGPROX_DEBUG.
    if std::env::var_os("JAGPROX_DEBUG").is_none() {
        jagprox_core::log::set_sink(|_| {});
    } else {
        jagprox_core::log::set_sink(|line| eprintln!("{line}"));
    }

    let command = cli.command.unwrap_or(Cmd::Launch {
        what: LaunchTarget::Launcher,
    });
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("jagprox: could not start: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(command, &ctx)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("jagprox: {e}");
            ExitCode::FAILURE
        }
    }
}
