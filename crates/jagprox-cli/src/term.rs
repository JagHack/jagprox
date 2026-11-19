//! Terminal output: Minecraft `§` codes to ANSI colors, color detection and a spinner.

use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use jagprox_core::constants::MC_COLORS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    None,
    Ansi256,
    TrueColor,
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// Colors for stdout: off when piped or with `NO_COLOR`, forced with `CLICOLOR_FORCE`
/// / `FORCE_COLOR`; truecolor when the terminal says it supports it.
pub fn detect() -> ColorMode {
    let forced = env_set("CLICOLOR_FORCE") || env_set("FORCE_COLOR");
    if env_set("NO_COLOR") || (!forced && !std::io::stdout().is_terminal()) {
        return ColorMode::None;
    }
    #[cfg(windows)]
    if enable_ansi_support::enable_ansi_support().is_err() {
        return ColorMode::None;
    }
    let colorterm = std::env::var("COLORTERM")
        .unwrap_or_default()
        .to_lowercase();
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    if colorterm.contains("truecolor")
        || colorterm.contains("24bit")
        || env_set("WT_SESSION")
        || matches!(
            term_program.as_str(),
            "iTerm.app" | "vscode" | "WezTerm" | "ghostty"
        )
    {
        ColorMode::TrueColor
    } else {
        ColorMode::Ansi256
    }
}

/// Nearest xterm-256 color cube/grey index for an RGB value.
fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    let level = |c: u8| -> u8 {
        if c < 48 {
            0
        } else if c < 115 {
            1
        } else {
            (c - 35) / 40
        }
    };
    let (lr, lg, lb) = (level(r), level(g), level(b));
    let cube = 16 + 36 * lr + 6 * lg + lb;
    let cube_rgb = |l: u8| if l == 0 { 0 } else { 55 + 40 * l };
    let cube_dist = dist((r, g, b), (cube_rgb(lr), cube_rgb(lg), cube_rgb(lb)));

    let avg = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
    let grey_index = if avg > 238 {
        23
    } else {
        (avg.saturating_sub(3) / 10) as u8
    };
    let grey = 8 + 10 * grey_index;
    if dist((r, g, b), (grey, grey, grey)) < cube_dist {
        232 + grey_index
    } else {
        cube
    }
}

fn dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).pow(2) as u32;
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

/// SGR foreground sequence for an RGB color in this mode.
pub fn fg(mode: ColorMode, [r, g, b]: [u8; 3]) -> String {
    match mode {
        ColorMode::None => String::new(),
        ColorMode::Ansi256 => format!("\x1b[38;5;{}m", ansi256(r, g, b)),
        ColorMode::TrueColor => format!("\x1b[38;2;{r};{g};{b}m"),
    }
}

pub fn reset(mode: ColorMode) -> &'static str {
    if mode == ColorMode::None {
        ""
    } else {
        "\x1b[0m"
    }
}

/// Renders legacy `§` text. A color code resets formatting, as in Minecraft; the
/// default color is the chat's white (§f).
pub fn render(text: &str, mode: ColorMode) -> String {
    if mode == ColorMode::None {
        return jagprox_core::formatter::strip_colors(text);
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut chars = text.chars().peekable();
    out.push_str(&fg(mode, [255, 255, 255]));
    while let Some(c) = chars.next() {
        if c != '§' {
            out.push(c);
            continue;
        }
        let Some(code) = chars.next() else {
            out.push(c);
            break;
        };
        match code.to_ascii_lowercase() {
            hex @ ('0'..='9' | 'a'..='f') => {
                let index = hex.to_digit(16).unwrap_or(15) as usize;
                out.push_str("\x1b[0m");
                out.push_str(&fg(mode, MC_COLORS[index].1));
            }
            'l' => out.push_str("\x1b[1m"),
            'm' => out.push_str("\x1b[9m"),
            'n' => out.push_str("\x1b[4m"),
            'o' => out.push_str("\x1b[3m"),
            'r' => {
                out.push_str("\x1b[0m");
                out.push_str(&fg(mode, [255, 255, 255]));
            }
            _ => {} // §k (obfuscated) and unknown codes are dropped
        }
    }
    out.push_str("\x1b[0m");
    out
}

pub fn print_lines(lines: &[String], mode: ColorMode) {
    let mut stdout = std::io::stdout().lock();
    for line in lines {
        let _ = writeln!(stdout, "{}", render(line, mode));
    }
}

/// A braille spinner on stderr, only when stderr is a terminal.
pub struct Spinner {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    pub fn start(message: impl Into<String>) -> Self {
        let message = message.into();
        let running = Arc::new(AtomicBool::new(std::io::stderr().is_terminal()));
        let handle = running.load(Ordering::Relaxed).then(|| {
            let running = running.clone();
            std::thread::spawn(move || {
                const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let mut i = 0;
                while running.load(Ordering::Relaxed) {
                    eprint!("\r{} {message}", FRAMES[i % FRAMES.len()]);
                    let _ = std::io::stderr().flush();
                    i += 1;
                    std::thread::sleep(Duration::from_millis(80));
                }
                eprint!("\r\x1b[2K");
                let _ = std::io::stderr().flush();
            })
        });
        Self { running, handle }
    }

    pub fn stop(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_mode_strips_codes() {
        assert_eq!(render("§aHi §lthere", ColorMode::None), "Hi there");
    }

    #[test]
    fn truecolor_uses_minecraft_rgb() {
        let out = render("§cX", ColorMode::TrueColor);
        assert!(out.contains("\x1b[38;2;255;85;85mX"), "{out:?}");
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn ansi256_maps_primary_colors() {
        assert_eq!(ansi256(0, 0, 0), 16);
        assert_eq!(ansi256(255, 255, 255), 231);
        assert_eq!(ansi256(255, 0, 0), 196);
        assert_eq!(ansi256(128, 128, 128), 244);
    }
}
