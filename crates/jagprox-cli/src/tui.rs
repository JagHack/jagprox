//! `jagprox` (no command) and `jagprox launch tui`: a terminal launcher.
//!
//! A ratatui dashboard that starts and stops the proxy, shows its live status
//! and Microsoft sign-in prompts, streams the log, and lists your aliases — the
//! launcher's core, without any GUI libraries, so it runs anywhere the static
//! `jagprox` binary does.

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, cursor};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Padding, Paragraph, Tabs, Wrap};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use jagprox_core::account as session;
use jagprox_core::config::Config;
use jagprox_core::msauth::Accounts;
use jagprox_core::proxy::{Proxy, ProxyEvent, ProxyOptions};

use crate::commands::CmdResult;

const ACCENT: Color = Color::Rgb(150, 86, 236);
const MUTED: Color = Color::Rgb(150, 146, 160);
const GOOD: Color = Color::Rgb(85, 200, 120);
const MAX_LOGS: usize = 1000;

const TABS: [&str; 3] = ["Status", "Logs", "Aliases"];

struct Tui {
    data_dir: PathBuf,
    config: Config,
    logs: Arc<Mutex<Vec<String>>>,
    aliases: Vec<(String, String)>,
    logged_in: bool,
    accounts: Vec<String>,
    tab: usize,
    scroll: u16,
    running: bool,
    connect: Option<String>,
    signin: Option<(String, String, String)>, // name, link, code
    handle: Option<JoinHandle<Result<(), String>>>,
    events: Option<broadcast::Receiver<ProxyEvent>>,
}

impl Tui {
    fn push(&self, line: impl Into<String>) {
        push_log(&self.logs, line.into());
    }

    fn start_proxy(&mut self) {
        if self.running {
            return;
        }
        let jwt = session::load().map(|(token, _)| token);
        let (events, rx) = broadcast::channel(256);
        match Proxy::new(ProxyOptions {
            data_dir: self.data_dir.clone(),
            config: self.config.clone(),
            jwt,
            events,
        }) {
            Ok(proxy) => {
                self.handle = Some(tokio::spawn(proxy.run()));
                self.events = Some(rx);
                self.running = true;
                self.push("§7Starting JagProx…");
            }
            Err(e) => self.push(format!("§c[ERROR] Could not start the proxy: {e}")),
        }
    }

    fn stop_proxy(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
        self.events = None;
        self.running = false;
        self.connect = None;
        self.signin = None;
        self.push("§7JagProx stopped.");
    }

    fn on_event(&mut self, event: ProxyEvent) {
        match event {
            ProxyEvent::Started { host, port } => {
                self.connect = Some(format!("{host}:{port}"));
                self.push(format!(
                    "§dJagProx is running — connect to §f{host}:{port}§d in MC 1.8.9."
                ));
            }
            ProxyEvent::SignIn { name, code, link } => {
                self.signin = Some((name.clone(), link.clone(), code.clone()));
                self.push(format!(
                    "§dSign in as §f{name}§d: open §b{link}§d and enter §f{code}"
                ));
                let _ = open::that_detached(&link);
            }
            ProxyEvent::SignedIn(name) => {
                self.signin = None;
                self.push(format!("§aSigned in as {name}. Reconnect in Minecraft."));
            }
            ProxyEvent::Chat(_) | ProxyEvent::Playing(_) | ProxyEvent::DiscordRpc(_) => {}
        }
    }
}

fn push_log(logs: &Arc<Mutex<Vec<String>>>, line: String) {
    let mut logs = logs.lock().unwrap_or_else(|e| e.into_inner());
    logs.push(line);
    let len = logs.len();
    if len > MAX_LOGS {
        logs.drain(0..len - MAX_LOGS);
    }
}

pub async fn run_tui() -> CmdResult {
    let data_dir = session::data_dir()?;
    let config = Config::load(&data_dir).map_err(|e| format!("could not load config.yml: {e}"))?;
    config.watch();

    let logs = Arc::new(Mutex::new(Vec::new()));
    let sink = logs.clone();
    jagprox_core::log::set_sink(move |line| push_log(&sink, format!("§7{line}")));

    let mut app = Tui {
        aliases: jagprox_core::aliases::list(&data_dir),
        logged_in: session::load().is_some(),
        accounts: Accounts::new(&data_dir).names(),
        data_dir,
        config,
        logs,
        tab: 0,
        scroll: 0,
        running: false,
        connect: None,
        signin: None,
        handle: None,
        events: None,
    };
    app.push("§7Welcome to JagProx. Press §fs§7 to start the proxy, §f?§7 for keys.");

    let mut terminal = setup_terminal().map_err(|e| format!("terminal setup failed: {e}"))?;
    let result = event_loop(&mut terminal, &mut app).await;
    restore_terminal(&mut terminal);
    if app.running {
        app.stop_proxy();
    }
    result.map_err(|e| format!("tui error: {e}"))
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut Tui,
) -> io::Result<()> {
    // Blocking terminal input on its own thread, delivered over a channel.
    let (tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        loop {
            match event::poll(Duration::from_millis(250)) {
                Ok(true) => {
                    if let Ok(ev) = event::read()
                        && tx.send(ev).is_err()
                    {
                        break;
                    }
                }
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });

    loop {
        terminal.draw(|f| draw(f, app))?;
        tokio::select! {
            maybe = input_rx.recv() => {
                let Some(event) = maybe else { break };
                if let Event::Key(key) = event
                    && key.kind != KeyEventKind::Release
                    && handle_key(app, key.code)
                {
                    break;
                }
            }
            event = recv_proxy(&mut app.events) => {
                match event {
                    Some(Ok(event)) => app.on_event(event),
                    Some(Err(broadcast::error::RecvError::Lagged(_))) => {}
                    Some(Err(broadcast::error::RecvError::Closed)) | None => {
                        // The proxy task ended on its own (e.g. a bind error).
                        if app.running {
                            app.running = false;
                            app.events = None;
                            app.connect = None;
                            app.push("§7The proxy stopped.");
                        }
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(300)) => {}
        }
    }
    Ok(())
}

/// Waits on the proxy event stream only while it exists.
async fn recv_proxy(
    rx: &mut Option<broadcast::Receiver<ProxyEvent>>,
) -> Option<Result<ProxyEvent, broadcast::error::RecvError>> {
    match rx {
        Some(rx) => Some(rx.recv().await),
        None => std::future::pending().await,
    }
}

/// Returns true to quit.
fn handle_key(app: &mut Tui, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Char('s') => {
            if app.running {
                app.stop_proxy();
            } else {
                app.start_proxy();
            }
        }
        KeyCode::Char('r') => app.aliases = jagprox_core::aliases::list(&app.data_dir),
        KeyCode::Tab | KeyCode::Right => app.tab = (app.tab + 1) % TABS.len(),
        KeyCode::BackTab | KeyCode::Left => app.tab = (app.tab + TABS.len() - 1) % TABS.len(),
        KeyCode::Char(c @ '1'..='3') => app.tab = (c as usize - '1' as usize).min(TABS.len() - 1),
        // `scroll` counts lines up from the tail; draw_logs caps it at the top.
        KeyCode::Up => app.scroll = app.scroll.saturating_add(1),
        KeyCode::Down => app.scroll = app.scroll.saturating_sub(1),
        _ => {}
    }
    false
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    out.execute(EnterAlternateScreen)?;
    out.execute(cursor::Hide)?;
    // Leave the terminal usable if a panic escapes the loop. Only for the UI thread:
    // the proxy catches panics on its own tasks and the TUI keeps running.
    let ui_thread = std::thread::current().id();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == ui_thread {
            let mut out = io::stdout();
            let _ = out.execute(LeaveAlternateScreen);
            let _ = out.execute(cursor::Show);
            let _ = disable_raw_mode();
        }
        hook(info);
    }));
    Terminal::new(CrosstermBackend::new(out))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) {
    let _ = disable_raw_mode();
    let _ = terminal.backend_mut().execute(LeaveAlternateScreen);
    let _ = terminal.backend_mut().execute(cursor::Show);
    let _ = terminal.show_cursor();
}

fn draw(f: &mut Frame, app: &mut Tui) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Min(1),    // body
        Constraint::Length(1), // footer
    ])
    .split(f.area());

    let title = Line::from(vec![
        Span::styled("⚡ JagProx ", Style::new().fg(ACCENT).bold()),
        Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::new().fg(MUTED),
        ),
    ]);
    let tabs = Tabs::new(TABS.iter().map(|t| Span::raw(*t)))
        .select(app.tab)
        .highlight_style(Style::new().fg(ACCENT).bold())
        .divider(Span::styled("│", Style::new().fg(MUTED)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(ACCENT))
                .title(title),
        );
    f.render_widget(tabs, chunks[0]);

    match app.tab {
        1 => draw_logs(f, app, chunks[1]),
        2 => draw_aliases(f, app, chunks[1]),
        _ => draw_status(f, app, chunks[1]),
    }

    let mut footer: Vec<Span> = Vec::new();
    for (key, what) in [
        ("s", if app.running { "stop" } else { "start" }),
        ("Tab", "switch"),
        ("↑↓", "scroll"),
        ("r", "reload aliases"),
        ("q", "quit"),
    ] {
        footer.push(Span::styled(
            format!("  {key} "),
            Style::new().fg(ACCENT).bold(),
        ));
        footer.push(Span::styled(what, Style::new().fg(MUTED)));
    }
    f.render_widget(Paragraph::new(Line::from(footer)), chunks[2]);
}

fn draw_status(f: &mut Frame, app: &Tui, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    let (dot, state) = if app.running {
        (
            Span::styled("● ", Style::new().fg(GOOD)),
            Span::styled("running", Style::new().fg(GOOD).bold()),
        )
    } else {
        (
            Span::styled("● ", Style::new().fg(MUTED)),
            Span::styled("stopped", Style::new().fg(MUTED).bold()),
        )
    };
    lines.push(Line::from(vec![Span::raw("Proxy:   "), dot, state]));
    if let Some(addr) = &app.connect {
        lines.push(Line::from(vec![
            Span::raw("Connect: "),
            Span::styled(addr.clone(), Style::new().fg(Color::White).bold()),
            Span::styled("  (Minecraft 1.8.9)", Style::new().fg(MUTED)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw("JagProx: "),
        if app.logged_in {
            Span::styled("logged in (stat commands work)", Style::new().fg(GOOD))
        } else {
            Span::styled(
                "not logged in — run `jagprox login`",
                Style::new().fg(MUTED),
            )
        },
    ]));
    let accounts = if app.accounts.is_empty() {
        "none yet — you'll get a sign-in code on first connect".to_string()
    } else {
        app.accounts.join(", ")
    };
    lines.push(Line::from(vec![
        Span::raw("MS accts: "),
        Span::styled(accounts, Style::new().fg(MUTED)),
    ]));

    if let Some((name, link, code)) = &app.signin {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Sign in to Microsoft as {name}:"),
            Style::new().fg(ACCENT).bold(),
        ));
        lines.push(Line::from(vec![
            Span::raw("  open "),
            Span::styled(link.clone(), Style::new().fg(Color::Cyan)),
            Span::raw("  enter "),
            Span::styled(code.clone(), Style::new().fg(Color::White).bold()),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Press s to start/stop. The launcher window has the full feature set — open it with `jagprox launch launcher`.",
        Style::new().fg(MUTED),
    ));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Rgb(70, 70, 70)))
        .padding(Padding::new(2, 2, 1, 1))
        .title(" Status ");
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_logs(f: &mut Frame, app: &mut Tui, area: Rect) {
    let logs = app.logs.lock().unwrap_or_else(|e| e.into_inner());
    let lines: Vec<Line> = logs.iter().map(|l| mc_line(l)).collect();
    let total = lines.len() as u16;
    let view = area.height.saturating_sub(2);
    // Follow the tail unless the user scrolled up.
    let base = total.saturating_sub(view);
    app.scroll = app.scroll.min(base);
    let scroll = base - app.scroll;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Rgb(70, 70, 70)))
        .padding(Padding::new(1, 1, 0, 0))
        .title(" Logs ");
    f.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
}

fn draw_aliases(f: &mut Frame, app: &Tui, area: Rect) {
    let items: Vec<ListItem> = if app.aliases.is_empty() {
        vec![ListItem::new(Span::styled(
            "No aliases. Edit aliases.json or use the launcher's Aliases pane.",
            Style::new().fg(MUTED),
        ))]
    } else {
        app.aliases
            .iter()
            .map(|(from, to)| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{from:<16}"), Style::new().fg(Color::White)),
                    Span::styled("→ ", Style::new().fg(MUTED)),
                    Span::styled(to.clone(), Style::new().fg(ACCENT)),
                ]))
            })
            .collect()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Rgb(70, 70, 70)))
        .padding(Padding::new(2, 2, 1, 1))
        .title(" Aliases (read-only — press r to reload) ");
    f.render_widget(List::new(items).block(block), area);
}

/// Parses Minecraft §-colour codes into a styled line.
fn mc_line(line: &str) -> Line<'static> {
    let spans: Vec<Span<'static>> = jagprox_core::formatter::legacy_segments(line)
        .into_iter()
        .map(|(color, bold, text)| {
            let [r, g, b] = color.unwrap_or([210, 208, 214]);
            let style = Style::new().fg(Color::Rgb(r, g, b));
            Span::styled(text, if bold { style.bold() } else { style })
        })
        .collect();
    Line::from(spans)
}
