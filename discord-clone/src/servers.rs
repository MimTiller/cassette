//! Server and channel wire types for the UI.
//!
//! The UI reaches the backend only through `crate::backend::<verb>` (the
//! seam), so the RPCs never appear here — a different backend later
//! replaces only the implementation files under `crate::backend`, not this
//! file. Wire types still ride in from `discord_server` (a shared types
//! crate is a later decision).

pub use discord_server::server_settings::UnreadCount;
pub use discord_server::servers::{ChannelSummary, ServerSummary};
