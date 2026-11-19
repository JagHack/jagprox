//! The proxy: a 1.8.9 server for the player's client on one side, a Microsoft-
//! authenticated connection to Hypixel on the other, and the packet relay between.

mod commands;
mod lookups;
pub mod modules;
mod server;
mod session;

pub use server::{Proxy, ProxyOptions};
pub use session::{Out, Session};

/// What the proxy tells the launcher / CLI about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyEvent {
    /// The proxy is listening.
    Started { host: String, port: u16 },
    /// A player connected (`true`) or left (`false`); drives Discord RPC.
    Playing(bool),
    /// A server chat line (positions 0 and 1) as legacy `§` text, for the chat view.
    Chat(String),
    /// A Microsoft sign-in is needed: show the code.
    SignIn {
        name: String,
        code: String,
        link: String,
    },
    /// A Microsoft account finished signing in.
    SignedIn(String),
    /// `/drpc` toggled Discord Rich Presence.
    DiscordRpc(bool),
}

/// `§8[§5jag§dprox§8] §r` + message, as every JagProx chat line starts.
pub const CHAT_PREFIX: &str = "§8[§5jag§dprox§8] §r";
