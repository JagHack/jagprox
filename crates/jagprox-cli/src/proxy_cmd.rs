//! `jagprox launch proxy` (headless proxy in the terminal) and `jagprox account`.

use jagprox_core::config::Config;
use jagprox_core::msauth::Accounts;
use jagprox_core::proxy::{Proxy, ProxyEvent, ProxyOptions};

use crate::commands::CmdResult;
use crate::term::{self, ColorMode};
use jagprox_core::account as session;

pub async fn run_proxy(color: ColorMode, show_chat: bool) -> CmdResult {
    let data_dir = session::data_dir()?;
    jagprox_core::log::set_sink(move |line| {
        println!("{}", term::render(&format!("§7{line}"), color))
    });
    let config = Config::load(&data_dir).map_err(|e| format!("could not load config.yml: {e}"))?;
    config.watch();

    let jwt = session::load().map(|(token, _)| token);
    if jwt.is_none() {
        println!(
            "{}",
            term::render(
                "§eNot logged in to JagProx: stat commands won't work. Run `jagprox login` first.",
                color
            )
        );
    }
    let accounts = Accounts::new(&data_dir).names();
    if accounts.is_empty() {
        println!(
            "{}",
            term::render(
                "§eNo Microsoft account is signed in yet. You'll get a sign-in code when you connect, or run `jagprox account add`.",
                color
            )
        );
    }

    let (events, mut rx) = tokio::sync::broadcast::channel(256);
    let proxy = Proxy::new(ProxyOptions {
        data_dir,
        config: config.clone(),
        jwt,
        events,
    })
    .map_err(|e| e.to_string())?;
    let mut server = tokio::spawn(proxy.run());

    loop {
        tokio::select! {
            result = &mut server => {
                return match result {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(format!("the proxy stopped: {e}")),
                };
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nStopping JagProx.");
                return Ok(());
            }
            event = rx.recv() => match event {
                Ok(ProxyEvent::Started { host, port }) => println!(
                    "{}",
                    term::render(&format!("§dJagProx is running. In Minecraft 1.8.9, connect to §f{host}:{port}§d. Ctrl+C stops it."), color)
                ),
                Ok(ProxyEvent::SignIn { name, code, link }) => {
                    println!(
                        "{}",
                        term::render(&format!("§d§lSign in to Microsoft as §f§l{name}§d§l: open §b{link}§d§l and enter §f§l{code}"), color)
                    );
                    let _ = open::that_detached(&link);
                }
                Ok(ProxyEvent::SignedIn(name)) => {
                    println!("{}", term::render(&format!("§aSigned in as {name}. Reconnect in Minecraft."), color));
                }
                Ok(ProxyEvent::Chat(text)) if show_chat => println!("{}", term::render(&text, color)),
                _ => {}
            }
        }
    }
}

pub async fn account_add(color: ColorMode) -> CmdResult {
    let accounts = Accounts::new(&session::data_dir()?);
    let session = accounts
        .sign_in(|code| {
            println!(
                "{}",
                term::render(
                    &format!(
                        "§dOpen §b{}§d and enter the code §f§l{}",
                        code.verification_uri, code.user_code
                    ),
                    color
                )
            );
            println!("Waiting for you to sign in...");
            let _ = open::that_detached(code.link());
        })
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Signed in to Microsoft as {}. JagProx will use it when {} connects.",
        session.name, session.name
    );
    Ok(())
}

pub async fn account_add_cookie(file: &std::path::Path) -> CmdResult {
    let contents = std::fs::read_to_string(file)
        .map_err(|e| format!("could not read {}: {e}", file.display()))?;
    let session = Accounts::new(&session::data_dir()?)
        .cookie_sign_in(&contents)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Imported {} from cookies. This session is short-lived (no refresh token) — \
         re-import when it expires. JagProx will use it when {} connects.",
        session.name, session.name
    );
    Ok(())
}

pub fn account_list() -> CmdResult {
    let names = Accounts::new(&session::data_dir()?).names();
    if names.is_empty() {
        println!("No Microsoft accounts are signed in. Add one with `jagprox account add`.");
    } else {
        for name in names {
            println!("{name}");
        }
    }
    Ok(())
}

pub fn account_remove(name: &str) -> CmdResult {
    if Accounts::new(&session::data_dir()?)
        .remove(name)
        .map_err(|e| e.to_string())?
    {
        println!("Removed {name}.");
        Ok(())
    } else {
        Err(format!("no signed-in account named {name}."))
    }
}
