//! Client-side front for message reactions.
//!
//! Every call routes through the backend seam (`crate::backend`), so the
//! UI never reaches the server's RPC stubs itself — a different backend
//! later replaces only the implementation files under `crate::backend`.
//!
//! Permission model (enforced by the backend, not here): reacting is
//! allowed for ANY member of the message's channel's server (Discord
//! parity), unlike pinning (moderator+). A guest (no session token) gets
//! `Err("unauthenticated")`.

pub use crate::backend::{add_reaction, remove_reaction};
