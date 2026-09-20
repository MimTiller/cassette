//! Full-stack API + static-server binary for the Discord clone.
//!
//! Compiled ONLY with the `server` cargo feature (`required-features`), so
//! the wasm client build never touches it. Mirrors the `server-fn-demo`
//! bin: it installs the shared app state (the SQLite `Db`) into the
//! `server` runtime's process-wide state store, then serves the
//! `#[server]`-registered API at `/_srv/*`.
//!
//! Port defaults to 3000 (override with `PORT`); the DB file defaults to
//! `./discord.db` (override with `DISCORD_DB`). Migrations run on startup.

// Force-link the app lib: the `#[server]` fns live in the lib and register
// their routes via `inventory::submit!`. Without a reference into the lib,
// the linker dead-strips those statics and every `/_srv/<fn>` request 404s
// with no build error (see `server::runtime`'s zero-route warning).
pub use discord_server::server_settings;

use std::net::SocketAddr;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    // Open (or create + migrate) the SQLite database.
    let db_path = std::env::var("DISCORD_DB").unwrap_or_else(|_| "discord.db".to_string());
    let db_path = PathBuf::from(db_path);
    let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");

    let db = discord_server::db::Db::migrate(&db_path, &migrations)
        .unwrap_or_else(|e| panic!("failed to open/migrate database at {db_path:?}: {e}"));

    // Install the shared state the `#[server]` fns read via `use_state::<Db>()`.
    server::install_state(db);

    // Bind + serve the API.
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("invalid port");

    println!("discord-server listening on http://{addr} (db: {db_path:?})");
    server::serve(addr)
        .await
        .unwrap_or_else(|e| panic!("server failed: {e}"));
}
