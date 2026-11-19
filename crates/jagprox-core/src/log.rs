//! Port of `formatter.log` / `formatter.debug`: timestamped log lines. The sink is
//! swappable so the launcher can show them in its log view and the CLI can stay quiet.

use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

type Sink = Box<dyn Fn(&str) + Send + Sync>;

static SINK: RwLock<Option<Sink>> = RwLock::new(None);
static DEBUG: AtomicBool = AtomicBool::new(false);

/// Routes log lines to `sink` instead of stdout.
pub fn set_sink(sink: impl Fn(&str) + Send + Sync + 'static) {
    *SINK.write().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(sink));
}

pub fn set_debug(enabled: bool) {
    DEBUG.store(enabled, Ordering::Relaxed);
}

pub fn debug_enabled() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

/// `[HH:MM:SS] message`, like the JS `formatter.log`.
pub fn log(message: impl AsRef<str>) {
    let line = format!(
        "[{}] {}",
        chrono::Local::now().format("%H:%M:%S"),
        message.as_ref()
    );
    match SINK.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(sink) => sink(&line),
        None => println!("{line}"),
    }
}

pub fn debug(message: impl AsRef<str>) {
    if debug_enabled() {
        log(format!("DEBUG: {}", message.as_ref()));
    }
}
