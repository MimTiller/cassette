//! Client-side front for pinned messages.
//!
//! Every call routes through the backend seam (`crate::backend`), so the
//! UI never reaches the server's RPC stubs itself — a different backend
//! later replaces only the implementation files under `crate::backend`.
//!
//! Permission model (enforced by the backend, not here): pinning/unpinning
//! a message is moderator+ (Discord "Manage Messages" parity), while
//! reading the pinned view / count is any member. A plain member who clicks
//! "Pin message" gets the backend's rejection as `Err` and the UI surfaces
//! it.

pub use discord_server::server_settings::PinnedMessage;
