//! Client-side direct-message front-end.
//!
//! Every DM call routes through the backend seam (`crate::backend`), so the
//! UI never reaches the server's RPC stubs itself — a different backend
//! later replaces only the implementation files under `crate::backend`.

pub use discord_server::dms::Dm;

pub use crate::backend::{
    add_group_dm_member, create_group_dm, list_dm_messages, list_dms, remove_group_dm_member,
    rename_group_dm, send_dm_message, start_dm,
};
