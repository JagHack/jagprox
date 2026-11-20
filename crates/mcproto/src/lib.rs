//! Minecraft 1.8.9 (protocol 47) codec: framing, compression, encryption and the
//! handful of packets JagProx inspects. Everything else is passed through raw.

pub mod buf;
pub mod conn;
pub mod crypto;
pub mod error;
pub mod frame;
pub mod packets;
pub mod world;

pub use error::{Error, Result};

/// Protocol version spoken on both legs of the proxy.
pub const PROTOCOL_VERSION: i32 = 47;
pub const VERSION_NAME: &str = "1.8.9";
