//! Client-side direct-message front-end.
//!
//! Every DM call routes through the backend seam (`crate::backend`), so the
//! UI never reaches the server's RPC stubs itself — a different backend
//! later replaces only the implementation files under `crate::backend`.

pub use discord_server::dms::Dm;
