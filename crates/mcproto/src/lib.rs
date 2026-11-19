//! Minecraft 1.8.9 (protocol 47) codec: framing, compression, encryption and the
//! handful of packets JagProx inspects. Everything else is passed through raw.

/// Protocol version spoken on both legs of the proxy.
pub const PROTOCOL_VERSION: i32 = 47;
