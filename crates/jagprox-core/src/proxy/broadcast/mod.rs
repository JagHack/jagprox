//! Live game broadcasting: let a friend spectate your Hypixel game through your
//! proxy, peer-to-peer, with Hypixel only ever seeing you.
//!
//! The proxy tracks your whole world as Hypixel sends it ([`state::GameState`]);
//! a spectator's own proxy connects to yours over an [`iroh`] tunnel brokered by
//! the JagProx rendezvous server, is given a snapshot of that world and then the
//! live stream, and swaps its Minecraft client over from Hypixel to that feed.

pub mod state;

pub use state::GameState;
