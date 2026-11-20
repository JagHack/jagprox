//! Port of `modules/commandHandler.js`: the JagProx chat commands. [`handle`] decides
//! synchronously whether a message is ours (and must not reach Hypixel); the work
//! itself runs as a session task.

use serde_json::{Map, Value, json};

use mcproto::packets::{Packet, clientbound as cb};

use super::ProxyEvent;
use super::broadcast;
use super::lookups;
use super::modules::party;
use super::session::Session;
use crate::constants::{self, QUICK_QUEUE};
use crate::js::{to_fixed, to_locale};
use crate::log::{debug, log};
use crate::reports::{self, DIVIDER};
use crate::stat_format;
use crate::stat_helper::{self, get_stat_value};

fn normalize_action(action: Option<&str>) -> Option<String> {
    action.map(|a| match a {
        "remove" | "rem" | "delete" | "del" => "remove".to_string(),
        other => other.to_string(),
    })
}

/// Returns true if `message` was a JagProx command (or alias) and must not be sent on.
pub fn handle(s: &Session, message: &str) -> bool {
    if let Some(target) = s.proxy().aliases().get(message) {
        if target.to_lowercase().starts_with("/play ") {
            s.state().last_play_command = Some(target.clone());
            log(format!("Captured last play command (from alias): {target}"));
        }
        s.chat(&format!("§dAlias §8» §f{target}"));
        s.send_server_chat(&target);
        return true;
    }

    let mut args: Vec<String> = message[1..]
        .split(' ')
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect();
    let command = if args.is_empty() {
        String::new()
    } else {
        args.remove(0).to_lowercase()
    };

    // config.yml can rename commands, e.g. `commands: { statcheck: sc }`.
    let renamed = s
        .proxy()
        .config()
        .get("commands")
        .and_then(|c| c.as_object().cloned())
        .and_then(|c| {
            c.iter()
                .find(|(_, alias)| alias.as_str() == Some(&command))
                .map(|(base, _)| base.clone())
        });
    let base = renamed.unwrap_or(command);

    let run = |task: std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + Send>>| {
        let session = s.clone();
        let message = message.to_string();
        s.spawn(async move {
            if let Err(e) = task.await {
                log(format!("[ERROR] Command \"{message}\" failed: {e}"));
                session.chat("§cThat command failed. Check the launcher logs for details.");
            }
        });
        true
    };
    let s2 = s.clone();
    match base.as_str() {
        "statcheck" => run(Box::pin(async move { statcheck(&s2, args).await })),
        "jtitles" => run(Box::pin(async move { jtitles(&s2, args).await })),
        "playercount" => run(Box::pin(async move { playercount(&s2, args).await })),
        "status" => {
            let Some(name) = args.first() else {
                s.chat("§cUsage: /status <username>");
                return true;
            };
            let real = reports::resolve_nickname(name, &s.proxy().config().nicknames());
            run(Box::pin(async move {
                lookups::status(&s2, &real).await;
                Ok(())
            }))
        }
        "spread" => run(Box::pin(async move { spread(&s2).await })),
        "q" => {
            quick_queue(s, &args);
            true
        }
        "goal" => run(Box::pin(async move { goal(&s2, args).await })),
        "superf" => run(Box::pin(async move { super_friend(&s2, args).await })),
        "psc" => {
            party::start(s, args.first().cloned());
            true
        }
        "alert" => {
            alert(s, args);
            true
        }
        "nickname" => {
            nickname(s, args);
            true
        }
        "rq" => {
            match s.state().last_play_command.clone() {
                Some(last) => {
                    s.chat(&format!("§dRe-queuing §8» §f{last}"));
                    s.send_server_chat(&last);
                }
                None => s.chat("§cNo last game found to re-queue for."),
            }
            true
        }
        "jagprox" => {
            help(s);
            true
        }
        "drpc" => {
            drpc(s);
            true
        }
        "bc" | "broadcast" => {
            broadcast_command(s, args);
            true
        }
        "watch" | "spectate" => {
            watch_command(s, args);
            true
        }
        "unwatch" | "stopwatching" => {
            unwatch_command(s);
            true
        }
        "link" => run(Box::pin(async move { link(&s2).await })),
        "gametrack" | "gt" => run(Box::pin(async move { gametrack(&s2, args).await })),
        "leaderboard" => run(Box::pin(async move { leaderboard(&s2, args).await })),
        _ => false,
    }
}

fn show_lines(s: &Session, lines: &[String]) {
    s.chat(&lines.join("\n"));
}

/// `/bc on|off|accept <name>|deny <name>|status`: control live broadcasting.
fn broadcast_command(s: &Session, args: Vec<String>) {
    let sub = args.first().map(|a| a.to_lowercase()).unwrap_or_default();
    match sub.as_str() {
        "" | "on" | "start" => {
            if s.broadcast().is_some() {
                s.chat("§eYou're already broadcasting. §7Use §f/bc off§7 to stop.");
                return;
            }
            let addr = s.proxy().compass_addr();
            let s2 = s.clone();
            s.spawn(async move {
                s2.chat("§7Starting broadcast…");
                match broadcast::start(&s2, &addr).await {
                    Ok(b) => {
                        s2.set_broadcast(Some(b));
                        s2.chat(&format!(
                            "§a§lBroadcasting! §r§7A friend can watch with §f/watch {}",
                            s2.username()
                        ));
                    }
                    Err(e) => s2.chat(&format!("§cCouldn't start broadcasting: {e}")),
                }
            });
        }
        "off" | "stop" => match s.broadcast() {
            Some(b) => {
                b.close();
                s.set_broadcast(None);
                s.chat("§aStopped broadcasting.");
            }
            None => s.chat("§eYou aren't broadcasting."),
        },
        "accept" | "deny" => {
            let Some(from) = args.get(1) else {
                s.chat("§cUsage: /bc accept <name>");
                return;
            };
            let Some(b) = s.broadcast() else {
                s.chat("§eYou aren't broadcasting.");
                return;
            };
            if sub == "accept" {
                b.accept(from);
                s.chat(&format!("§aLetting §f{from}§a watch."));
            } else {
                b.deny(from);
                s.chat(&format!("§7Declined §f{from}§7."));
            }
        }
        "status" => match s.broadcast() {
            Some(b) => s.chat(&format!(
                "§aBroadcasting — §f{}§a watching.",
                b.spectator_count()
            )),
            None => s.chat("§7Not broadcasting. §fUse /bc on."),
        },
        _ => s.chat("§cUsage: §f/bc on|off|accept <name>|deny <name>|status"),
    }
}

/// `/watch <player>`: reconnect into spectate mode for that broadcaster.
fn watch_command(s: &Session, args: Vec<String>) {
    let Some(target) = args.first().cloned() else {
        s.chat("§cUsage: /watch <player>");
        return;
    };
    if let Some(b) = s.broadcast() {
        b.close();
        s.set_broadcast(None);
    }
    s.proxy().set_spectate(Some(target.clone()));
    kick(
        s,
        &format!(
            "§aConnecting you to §f{target}§a's broadcast…\n\n§7Reconnect to JagProx to start watching."
        ),
    );
}

/// `/unwatch`: leave spectate mode and reconnect to Hypixel normally.
fn unwatch_command(s: &Session) {
    s.proxy().set_spectate(None);
    kick(
        s,
        "§aLeaving the broadcast.\n\n§7Reconnect to JagProx to return to Hypixel.",
    );
}

/// Disconnects the player's client with a message (they reconnect to apply a mode
/// change like entering or leaving spectate).
fn kick(s: &Session, reason: &str) {
    s.send_to_client(
        cb::Disconnect {
            reason: json!({ "text": reason }).to_string(),
        }
        .to_payload(),
    );
}

async fn statcheck(s: &Session, args: Vec<String>) -> Result<(), String> {
    let mode = args.first().cloned().unwrap_or_default();
    if mode == "?" {
        show_lines(s, &reports::statcheck_modes());
        return Ok(());
    }
    let username = args.get(1..).map(|a| a.join(" ")).unwrap_or_default();
    if mode.is_empty() || username.is_empty() {
        s.chat("§cUsage: /sc <gamemode> <username>");
        s.chat("§eUse /sc ? to see all available gamemodes.");
        return Ok(());
    }
    let result = lookups::statcheck(s, &mode, &username).await;
    if let Some(sc) = result
        && s.proxy().config().stats_telemetry()
        && let Some(backend) = s.backend()
    {
        // Opt-in only (config: stats_telemetry: true). Reuses the data /sc fetched.
        let wins = stat_format::get_mode_stats(Some(&sc.data.player), sc.game).wins;
        match backend
            .send_stat_telemetry(&sc.profile.username, sc.game.display_name, wins)
            .await
        {
            Ok(()) => debug(format!(
                "Telemetry dispatched for {} ({}): {wins} wins",
                sc.profile.username, sc.game.display_name
            )),
            Err(e) => log(format!("Telemetry dispatch failed: {e}")),
        }
    }
    Ok(())
}

async fn leaderboard(s: &Session, mut args: Vec<String>) -> Result<(), String> {
    let game = if args.is_empty() {
        None
    } else {
        Some(args.remove(0).to_lowercase())
    };
    let kind = args.join(" ").to_lowercase();
    if game.as_deref() != Some("duels") {
        s.chat("§cCurrently, leaderboards are only available for 'duels'.");
        s.chat("§cUsage: /leaderboard duels <monthly wins|weekly wins>");
        return Ok(());
    }
    let kind = match kind.as_str() {
        "monthly wins" => "Monthly Wins",
        "weekly wins" => "Weekly Wins",
        _ => {
            s.chat("§cInvalid leaderboard type. Supported types for Duels: 'monthly wins', 'weekly wins'.");
            s.chat("§cUsage: /leaderboard duels <monthly wins|weekly wins>");
            return Ok(());
        }
    };
    s.chat(&format!(
        "§dFetching §f{kind} §dleaderboard for §5Duels§d..."
    ));
    match lookups::leaderboard(s, "DUELS", kind).await {
        Err(e) => s.chat(&format!("§cError: {e}")),
        Ok((title, leaders)) => {
            let mut message = format!("{DIVIDER}\n  §5§l{title}\n \n");
            if leaders.is_empty() {
                message.push_str("    §8No leaders found for this category.");
            } else {
                for (i, name) in leaders.iter().enumerate() {
                    message.push_str(&format!("  §d{}. §f{name}\n", i + 1));
                }
            }
            message.push_str(&format!("\n{DIVIDER}"));
            s.chat(&message);
        }
    }
    Ok(())
}

async fn jtitles(s: &Session, args: Vec<String>) -> Result<(), String> {
    let (uuid, name) = match args.first() {
        Some(username) => {
            s.chat(&format!("§eFetching titles for §d{username}§e..."));
            let real = reports::resolve_nickname(username, &s.proxy().config().nicknames());
            match s.hypixel().mojang_uuid(&real).await {
                Ok(Some(p)) => (p.uuid, p.username),
                _ => {
                    s.chat("§cInvalid playername!");
                    return Ok(());
                }
            }
        }
        None => (s.uuid().to_string(), s.username().to_string()),
    };
    match reports::jtitles(s.hypixel(), &uuid, &name).await {
        Ok(lines) => lines.iter().for_each(|l| s.chat(l)),
        Err(e) => s.chat(&e.line),
    }
    Ok(())
}

async fn spread(s: &Session) -> Result<(), String> {
    match reports::spread(s.hypixel(), s.uuid(), None).await {
        Ok(lines) => lines.iter().for_each(|l| s.chat(l)),
        Err(e) => s.chat(&e.line),
    }
    Ok(())
}

async fn playercount(s: &Session, args: Vec<String>) -> Result<(), String> {
    let query = args.join(" ").to_lowercase();
    if query.is_empty() || query == "?" {
        show_lines(s, &reports::playercount_modes());
        return Ok(());
    }
    match reports::playercount(s.hypixel(), &query).await {
        Ok(lines) => lines.iter().for_each(|l| s.chat(l)),
        Err(e) if e.cause.is_none() => {
            s.chat(&e.line);
            s.chat("§eUse /playercount ? to see all modes.");
        }
        Err(e) => s.chat(&e.line),
    }
    Ok(())
}

async fn gametrack(s: &Session, args: Vec<String>) -> Result<(), String> {
    let Some(backend) = s.backend() else {
        s.chat("§c[GameTrack] Error: You are not logged in to JagProx.");
        return Ok(());
    };
    let sub = args
        .first()
        .map(|a| a.to_lowercase())
        .unwrap_or_else(|| "hour".into());
    let result = match sub.as_str() {
        "hour" => {
            let hours = match args.get(1) {
                Some(h) => js_parse_int(h),
                None => Some(1),
            };
            let Some(hours) = hours.filter(|h| *h > 0) else {
                s.chat("§cInvalid number of hours.");
                return Ok(());
            };
            backend.gametrack_hours(hours as u32).await.map(|data| {
                reports::gametrack_modes(
                    &format!("Game Stats for the Last {hours} Hour(s)"),
                    "No game data found for your account in this period.",
                    data.get(s.uuid()),
                )
            })
        }
        "day" => backend.gametrack_day().await.map(|data| {
            reports::gametrack_modes(
                "Game Stats For Today",
                "No game data found for your account today.",
                data.get(s.uuid()),
            )
        }),
        "log" => backend
            .gametrack_log()
            .await
            .map(|data| reports::gametrack_log(&data)),
        _ => {
            s.chat("§cInvalid /gametrack command. Use: hour [hours], day, or log.");
            return Ok(());
        }
    };
    match result {
        Ok(lines) => lines.iter().for_each(|l| s.chat(l)),
        Err(e) => s.chat(&format!("§c[GameTrack] Error: {e}")),
    }
    Ok(())
}

/// JS `parseInt(text, 10)`: leading digits, `None` for NaN.
fn js_parse_int(text: &str) -> Option<i64> {
    let t = text.trim_start();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<i64>().ok().map(|n| n * sign)
}

fn quick_queue(s: &Session, args: &[String]) {
    let Some(mode) = args.first().map(|m| m.to_lowercase()) else {
        s.chat("§cUsage: /q <mode>");
        s.chat("§eUse /q ? to see all available modes.");
        return;
    };
    if mode == "?" {
        let mut message = format!("{DIVIDER}\n§r  §5§lAvailable Quick Queue Commands (/q)\n \n");
        for (alias, name, _) in QUICK_QUEUE {
            message.push_str(&format!("§r  §d{alias} §8- §f{name}\n"));
        }
        message.push_str(&format!("\n{DIVIDER}"));
        s.chat(&message);
        return;
    }
    match QUICK_QUEUE.iter().find(|(alias, _, _)| *alias == mode) {
        Some((_, name, command)) => {
            s.chat(&format!("§eJoining {name}..."));
            s.send_server_chat(command);
            s.state().last_play_command = Some(command.to_string());
        }
        None => s.chat(&format!(
            "§cUnknown mode '{mode}'. Use /q ? to see available modes."
        )),
    }
}

async fn goal(s: &Session, mut args: Vec<String>) -> Result<(), String> {
    let action = if args.is_empty() {
        None
    } else {
        Some(args.remove(0).to_lowercase())
    };
    let path = s.proxy().data_dir().join("goal.json");
    let read_goal = || -> Option<Value> {
        let text = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                log(format!("goal.json is unreadable: {e}"));
                None
            }
        }
    };

    match action.as_deref() {
        Some("set") => {
            let mut next = || {
                if args.is_empty() {
                    None
                } else {
                    Some(args.remove(0))
                }
            };
            let game = next().map(|g| g.to_lowercase());
            let stat = next().map(|g| g.to_lowercase());
            let target = next().and_then(|t| js_parse_float(&t));
            let (Some(game), Some(stat), Some(target)) = (game, stat, target) else {
                s.chat("§cUsage: /goal set <game> <stat> <target>");
                s.chat("§eExample: /goal set bedwars fkdr 5");
                return Ok(());
            };
            let Some(stats) = stat_helper::goal_stats(&game) else {
                s.chat(&format!(
                    "§cUnknown game '{game}'. Available: §e{}",
                    stat_helper::goal_games().collect::<Vec<_>>().join(", ")
                ));
                return Ok(());
            };
            let stats: Vec<&str> = stats.collect();
            if !stats.contains(&stat.as_str()) {
                s.chat(&format!(
                    "§cInvalid stat. Available for {game}: §e{}",
                    stats.join(", ")
                ));
                return Ok(());
            }
            s.chat("§eFetching your current stats to set the goal...");
            let Ok(Some(data)) = s.hypixel().stats(s.uuid()).await else {
                s.chat("§cCould not fetch your Hypixel stats.");
                return Ok(());
            };
            let Some(current) = get_stat_value(&data.player, &game, &stat) else {
                s.chat("§cAn error occurred while retrieving the specific stat.");
                return Ok(());
            };
            if target <= current.value {
                s.chat(&format!(
                    "§cYour target of {} is not higher than your current {} of {}!",
                    to_locale(target),
                    current.name,
                    to_locale(current.value)
                ));
                return Ok(());
            }
            let goal = json!({
                "gamemode": game, "statAlias": stat, "target": target, "initial": current.value,
                "name": current.name, "setAt": chrono::Utc::now().timestamp_millis(),
            });
            crate::file_util::write_file_atomic(
                &path,
                &serde_json::to_string_pretty(&goal).unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?;
            s.chat(&format!(
                "§aGoal set! Track your progress for §e{} in {game}§a to reach §6{}§a.",
                current.name,
                to_locale(target)
            ));
        }
        Some("view") => {
            let Some(goal) = read_goal() else {
                s.chat("§eYou do not have an active goal. Use /goal set <game> <stat> <target>.");
                return Ok(());
            };
            s.chat("§eChecking your goal progress...");
            let Ok(Some(data)) = s.hypixel().stats(s.uuid()).await else {
                s.chat("§cCould not fetch your Hypixel stats.");
                return Ok(());
            };
            let text = |k: &str| {
                goal.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let num = |k: &str| crate::js::num(goal.get(k));
            let Some(current) = get_stat_value(&data.player, &text("gamemode"), &text("statAlias"))
            else {
                s.chat("§cYour saved goal uses a stat that no longer exists. Use /goal cancel and set it again.");
                return Ok(());
            };
            let (initial, target) = (num("initial"), num("target"));
            let percentage =
                ((current.value - initial) / (target - initial) * 100.0).clamp(0.0, 100.0);
            let percentage = if percentage.is_nan() { 0.0 } else { percentage };
            let remaining = (target - current.value).max(0.0);
            let filled = (20.0 * percentage / 100.0).round() as usize;
            let bar = format!("§d{}§8{}", "█".repeat(filled), "█".repeat(20 - filled));
            s.chat(DIVIDER);
            s.chat(&format!(
                "  §5§lGoal: {} in {}",
                text("name"),
                text("gamemode")
            ));
            s.chat(&format!(
                "  §7{} §8» §f{}",
                to_locale(initial),
                to_locale(target)
            ));
            s.chat(" ");
            s.chat(&format!(
                "  §fProgress: {bar} §d{}%",
                to_fixed(percentage, 2)
            ));
            s.chat(&format!(
                "  §dCurrent: §f{} §8(Remaining: §f{}§8)",
                to_locale(current.value),
                to_locale(remaining)
            ));
            s.chat(DIVIDER);
        }
        Some("cancel") => {
            if read_goal().is_none() {
                s.chat("§eYou do not have an active goal to cancel.");
                return Ok(());
            }
            let _ = std::fs::remove_file(&path);
            s.chat("§aYour active goal has been cancelled.");
        }
        _ => s.chat("§cInvalid subcommand. Use /goal <set|view|cancel>."),
    }
    Ok(())
}

/// JS `parseFloat`: the longest leading number, `None` for NaN.
fn js_parse_float(text: &str) -> Option<f64> {
    let t = text.trim_start();
    let end = (1..=t.len())
        .rev()
        .find(|&i| t.is_char_boundary(i) && t[..i].parse::<f64>().is_ok())?;
    t[..end].parse().ok().filter(|v: &f64| v.is_finite())
}

fn help(s: &Session) {
    let commands = s.proxy().config().get("commands").unwrap_or_default();
    let sc = commands
        .get("statcheck")
        .and_then(Value::as_str)
        .unwrap_or("sc")
        .to_string();
    let status = commands
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("status")
        .to_string();
    let mut list = vec![
        (
            format!("/{sc} <game> <player>"),
            "Checks Hypixel stats for a player.",
        ),
        (
            format!("/{status} <player>"),
            "Shows a player's online status.",
        ),
        (
            "/goal <set|view|cancel> [args]".into(),
            "Manages your personal stat goals.",
        ),
        (
            "/q <mode>".into(),
            "Quickly joins a game mode. Use /q ? for a list.",
        ),
        (
            "/psc [game]".into(),
            "Runs a stat check for all party members.",
        ),
        ("/rq".into(), "Re-queues your last played game."),
        (
            "/alert <add|remove|list> [player]".into(),
            "Manages in-game alerts for players.",
        ),
        (
            "/nickname <add|remove|list> [args]".into(),
            "Sets local nicknames for players.",
        ),
        (
            "/superf <add|remove|list> [player] [games...]".into(),
            "Notifies you when a friend joins one of the given games (or \"any\").",
        ),
        (
            "/gametrack <hour [n]|day|log>".into(),
            "Shows your tracked wins and losses (alias: /gt).",
        ),
        ("/drpc".into(), "Toggles the Discord Rich Presence."),
        (
            "/playercount <mode>".into(),
            "Checks how many players are queuing in a mode. Use /playercount ? for list.",
        ),
        (
            "/spread".into(),
            "Shows your Duels wins and losses spread for all modes.",
        ),
        (
            "/jtitles [player]".into(),
            "Shows Duels mode-specific titles for you or another player.",
        ),
        (
            "/leaderboard <game> <type>".into(),
            "Displays top players for a game leaderboard (e.g., duels monthly wins|weekly wins).",
        ),
        (
            "/link".into(),
            "Links your Minecraft account to your JagProx account.",
        ),
        (
            "/bc <on|off|accept|deny|status>".into(),
            "Broadcasts your game so a friend can spectate it live.",
        ),
        (
            "/watch <player>".into(),
            "Spectates a friend's live broadcast (reconnect to apply; /unwatch to leave).",
        ),
        ("/jagprox".into(), "Displays this help message."),
    ];
    // JS localeCompare: case-insensitive first.
    list.sort_by(|a, b| {
        a.0.to_lowercase()
            .cmp(&b.0.to_lowercase())
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut message = format!("{DIVIDER}\n§r  §5§lJagProx §8- §7Available Commands\n \n");
    for (syntax, desc) in &list {
        let (cmd, args) = syntax.split_once(' ').unwrap_or((syntax, ""));
        message.push_str(&format!("§r  §5{cmd} §d{args}\n§r    §8- §7{desc}\n \n"));
    }
    let mut message = message.trim_end().to_string();
    message.push_str(&format!("\n§r\n{DIVIDER}"));
    s.chat(&message);
}

/// The launcher owns the Discord connection; it's told through an event.
fn drpc(s: &Session) {
    let enabled = !s.proxy().config().discord_rpc_enabled();
    let _ = s.proxy().config().update(&["discord_rpc"], |c| {
        let entry = c.entry("discord_rpc").or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        entry["enabled"] = json!(enabled);
    });
    let _ = s.proxy().events().send(ProxyEvent::DiscordRpc(enabled));
    s.chat(if enabled {
        "§aDiscord RPC has been enabled."
    } else {
        "§cDiscord RPC has been disabled."
    });
}

/// Saves one config key, telling the player if it fails (JS `saveConfig`).
fn save(s: &Session, key: &str, change: impl FnOnce(&mut Map<String, Value>)) {
    if let Err(e) = s.proxy().config().update(&[key], change) {
        s.chat("§cError saving configuration to file.");
        log(format!("Config save error: {e}"));
    }
}

fn nickname(s: &Session, mut args: Vec<String>) {
    let first = if args.is_empty() {
        None
    } else {
        Some(args.remove(0).to_lowercase())
    };
    let action = normalize_action(first.as_deref());
    let Some(action) = action else {
        s.chat("§cUsage: /nickname <add|remove|list>");
        s.chat("§cAdd: /nickname add <real_name> <nickname>");
        s.chat("§cRemove: /nickname remove <real_name_or_nickname>");
        return;
    };
    let current: Map<String, Value> = s
        .proxy()
        .config()
        .get("nicknames")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    match action.as_str() {
        "add" => {
            let real = if args.is_empty() {
                None
            } else {
                Some(args.remove(0))
            };
            let nick = args.join(" ");
            let Some(real) = real.filter(|_| !nick.is_empty()) else {
                s.chat("§cUsage: /nickname add <real_name> <nickname>");
                return;
            };
            let (r, n) = (real.clone(), nick.clone());
            save(s, "nicknames", move |c| {
                let mut map = c
                    .get("nicknames")
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default();
                map.insert(r, json!(n));
                c.insert("nicknames".into(), Value::Object(map));
            });
            s.chat(&format!("§aSet nickname for '{real}' to '{nick}'."));
        }
        "remove" => {
            let target = args.join(" ");
            if target.is_empty() {
                s.chat("§cUsage: /nickname remove <real_name_or_nickname>");
                return;
            }
            let lower = target.to_lowercase();
            let found = current.iter().find(|(real, nick)| {
                real.to_lowercase() == lower
                    || nick.as_str().is_some_and(|n| n.to_lowercase() == lower)
            });
            match found.map(|(real, _)| real.clone()) {
                Some(real) => {
                    s.chat(&format!("§aRemoved nickname for '{real}'."));
                    save(s, "nicknames", move |c| {
                        if let Some(map) = c.get_mut("nicknames").and_then(Value::as_object_mut) {
                            map.shift_remove(&real);
                        }
                    });
                }
                None => s.chat(&format!("§cNickname or player '{target}' not found.")),
            }
        }
        "list" => {
            if current.is_empty() {
                s.chat("§dYou have no nicknames set.");
            } else {
                s.chat("§5Your nicknames:");
                for (real, nick) in &current {
                    s.chat(&format!(
                        "§8- §d{real} §8» §f{}",
                        nick.as_str().unwrap_or("")
                    ));
                }
            }
        }
        _ => s.chat("§cInvalid action. Use 'add', 'remove', or 'list'."),
    }
}

fn alert(s: &Session, mut args: Vec<String>) {
    let first = if args.is_empty() {
        None
    } else {
        Some(args.remove(0).to_lowercase())
    };
    let action = normalize_action(first.as_deref());
    let list: Vec<String> = s
        .proxy()
        .config()
        .get("tab_alerts")
        .and_then(|v| v.as_array().cloned())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if action.as_deref() == Some("list") {
        if list.is_empty() {
            s.chat("§dYour alert list is empty.");
        } else {
            s.chat("§5Players on your alert list:");
            for name in &list {
                s.chat(&format!("§8- §d{name}"));
            }
        }
        return;
    }
    let username = if args.is_empty() {
        None
    } else {
        Some(args.remove(0))
    };
    let (Some(action), Some(username)) = (action, username) else {
        s.chat("§cUsage: /alert <add|remove|list> [username]");
        return;
    };
    let index = list
        .iter()
        .position(|p| p.to_lowercase() == username.to_lowercase());
    match action.as_str() {
        "add" => {
            if index.is_some() {
                s.chat(&format!(
                    "§cPlayer '{username}' is already on the alert list."
                ));
                return;
            }
            let mut new = list.clone();
            new.push(username.clone());
            save(s, "tab_alerts", move |c| {
                c.insert("tab_alerts".into(), json!(new));
            });
            s.chat(&format!("§aAdded '{username}' to the alert list."));
        }
        "remove" => {
            let Some(index) = index else {
                s.chat(&format!("§cPlayer '{username}' is not on the alert list."));
                return;
            };
            let mut new = list.clone();
            let removed = new.remove(index);
            save(s, "tab_alerts", move |c| {
                c.insert("tab_alerts".into(), json!(new));
            });
            s.chat(&format!("§aRemoved '{removed}' from the alert list."));
        }
        _ => s.chat("§cInvalid action. Use 'add', 'remove', or 'list'."),
    }
}

async fn super_friend(s: &Session, mut args: Vec<String>) -> Result<(), String> {
    let first = if args.is_empty() {
        None
    } else {
        Some(args.remove(0).to_lowercase())
    };
    let action = normalize_action(first.as_deref());
    let friends: Map<String, Value> = s
        .proxy()
        .config()
        .get("super_friends")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    if action.as_deref() == Some("list") {
        if friends.is_empty() {
            s.chat("§dYou have no super friends set.");
        } else {
            s.chat("§5Your super friends:");
            for (name, modes) in &friends {
                let modes: Vec<&str> = modes
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                s.chat(&format!("§8- §d{name} §7({})", modes.join(", ")));
            }
        }
        return Ok(());
    }
    let username = if args.is_empty() {
        None
    } else {
        Some(args.remove(0))
    };
    let (Some(action), Some(username)) = (action, username) else {
        s.chat("§cUsage: /superf <add|remove|list> <username> [gamemodes...]");
        return Ok(());
    };
    match action.as_str() {
        "add" => {
            let modes: Vec<String> = args.iter().map(|m| m.to_lowercase()).collect();
            if modes.is_empty() {
                s.chat("§cYou must specify at least one gamemode (e.g., bedwars), or 'any'.");
                return Ok(());
            }
            let unknown: Vec<&str> = modes
                .iter()
                .filter(|m| *m != "any" && *m != "all" && constants::game_mode(m).is_none())
                .map(String::as_str)
                .collect();
            if !unknown.is_empty() {
                s.chat(&format!(
                    "§eNote: unknown gamemode(s) {} will be matched against Hypixel's raw game type.",
                    unknown.join(", ")
                ));
            }
            let Ok(Some(profile)) = s.hypixel().mojang_uuid(&username).await else {
                s.chat(&format!("§cPlayer '{username}' not found."));
                return Ok(());
            };
            let (name, list) = (profile.username.clone(), modes.clone());
            save(s, "super_friends", move |c| {
                let mut map = c
                    .get("super_friends")
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default();
                map.insert(name, json!(list));
                c.insert("super_friends".into(), Value::Object(map));
            });
            s.chat(&format!(
                "§aNow tracking {} for: §e{}§a.",
                profile.username,
                modes.join(", ")
            ));
        }
        "remove" => match friends
            .keys()
            .find(|k| k.to_lowercase() == username.to_lowercase())
            .cloned()
        {
            Some(key) => {
                let k = key.clone();
                save(s, "super_friends", move |c| {
                    if let Some(map) = c.get_mut("super_friends").and_then(Value::as_object_mut) {
                        map.shift_remove(&k);
                    }
                });
                s.chat(&format!("§aRemoved {key} from tracked friends."));
            }
            None => s.chat(&format!(
                "§cPlayer '{username}' is not currently being tracked."
            )),
        },
        _ => s.chat("§cInvalid action. Use 'add', 'remove', or 'list'."),
    }
    Ok(())
}

/// Links this Minecraft account to the JagProx account: we "join" a one-time server id
/// with our Minecraft session, and the backend asks Mojang (hasJoined) who did that.
async fn link(s: &Session) -> Result<(), String> {
    let (Some(mc), Some(backend)) = (s.mc_session(), s.backend()) else {
        s.chat("§cYou need to be connected to Hypixel through JagProx to link your account.");
        return Ok(());
    };
    s.chat("§dLinking your Minecraft account...");
    let challenge = match backend.link_challenge().await {
        Ok(c) => c,
        Err(e) => {
            s.chat(&format!("§cError starting link: {e}"));
            return Ok(());
        }
    };
    let Some(server_id) = challenge.get("server_id").and_then(Value::as_str) else {
        s.chat("§cError starting link: the server sent no challenge");
        return Ok(());
    };
    if let Err(e) = crate::msauth::session_join(mc, server_id).await {
        log(format!("Mojang session join failed: {e}"));
        s.chat("§cMojang rejected the ownership check. Try reconnecting and run /link again.");
        return Ok(());
    }
    match backend.link_verify(s.username(), server_id).await {
        Ok(result) => {
            let message = result
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Account linked.");
            s.chat(&format!("§a{message}"));
        }
        Err(e) => s.chat(&format!("§cError linking account: {e}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_number_parsing() {
        assert_eq!(js_parse_int("12abc"), Some(12));
        assert_eq!(js_parse_int("abc"), None);
        assert_eq!(js_parse_int("-3"), Some(-3));
        assert_eq!(js_parse_float("5"), Some(5.0));
        assert_eq!(js_parse_float("2.5x"), Some(2.5));
        assert_eq!(js_parse_float("x"), None);
    }

    #[test]
    fn remove_aliases() {
        assert_eq!(normalize_action(Some("del")).as_deref(), Some("remove"));
        assert_eq!(normalize_action(Some("add")).as_deref(), Some("add"));
        assert_eq!(normalize_action(None), None);
    }
}
