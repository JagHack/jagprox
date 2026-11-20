//! The JagProx broadcasting rendezvous protocol: newline-delimited JSON control
//! messages, each tagged by a `t` field. After both sides are told to `start_relay`
//! with a token, they open fresh data connections carrying only `relay` and the
//! server pipes raw bytes between the matching pair.
//!
//! This file is kept byte-identical in `jagprox-core`'s broadcasting client so both
//! ends agree on the wire format.

use serde::{Deserialize, Serialize};

/// A message from a JagProx proxy to the rendezvous server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// A broadcaster announces itself and keeps this control connection open.
    Register { name: String, uuid: String },
    /// A spectator asks to watch `target`. Sent on its own control connection,
    /// which then waits for `Accepted`/`Denied`.
    Join { target: String, from: String },
    /// The broadcaster allows a pending spectator.
    Accept { from: String },
    /// The broadcaster rejects a pending spectator.
    Deny { from: String },
    /// A fresh data connection to be paired with its twin and piped.
    Relay { token: String },
    /// Keep-alive.
    Ping,
}

/// A message from the rendezvous server to a JagProx proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    /// The broadcaster is registered.
    Registered,
    /// A spectator wants to watch; shown to the broadcaster for approval.
    JoinRequest { from: String },
    /// Both sides open a data connection with this token. `host` is true for the
    /// broadcaster (acts as the Minecraft server) and false for the spectator.
    StartRelay {
        token: String,
        host: bool,
        peer: String,
    },
    /// The spectator's join was refused (denied, offline, or an error).
    Denied { reason: String },
    /// A protocol-level error.
    Error { msg: String },
    /// Keep-alive reply.
    Pong,
}
