//! Full-stack API + static-server binary for the Discord clone.
//!
//! Compiled ONLY with the `server` cargo feature (`required-features`), so
//! the wasm client build never touches it. Mirrors the `server-fn-demo`
//! bin: it installs the shared app state (the SQLite `Db`) into the
//! `server` runtime's process-wide state store, then serves the
//! `#[server]`-registered API at `/_srv/*`.
//!
//! When `WEB_DIST` is set (exported by `idealyst dev --web` / `idealyst run
//! server`) the built wasm UI is served from that directory at `/` (and its
//! `pkg/` at `/pkg`), so the browser loads the app and calls the API from
//! the same origin.
//!
//! Env: `PORT` (default 3000), `HOST` (default 127.0.0.1 -- set to 0.0.0.0
//! to expose on the LAN), `DISCORD_DB` (default `./discord.db`), `WEB_DIST`.
//! Migrations run on startup.

// Force-link the app lib: the `#[server]` fns live in the lib and register
// their routes via `inventory::submit!`. Without a reference into the lib,
// the linker dead-strips those statics and every `/_srv/<fn>` request 404s
// with no build error (see `server::runtime`'s zero-route warning).
pub use discord_server::server_settings;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use axum::Router;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    // Open (or create + migrate) the SQLite database.
    let db_path = std::env::var("DISCORD_DB").unwrap_or_else(|_| "discord.db".to_string());
    let db_path = PathBuf::from(db_path);
    // This file is also built as the bin of the `discord-server-run` wrapper
    // crate, where CARGO_MANIFEST_DIR is the wrapper dir -- so fall back to the
    // sibling `discord-server/migrations` (or set DISCORD_MIGRATIONS).
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let migrations = std::env::var_os("DISCORD_MIGRATIONS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let own = manifest_dir.join("migrations");
            if own.is_dir() { own } else { manifest_dir.join("../discord-server/migrations") }
        });

    let db = discord_server::db::Db::migrate(&db_path, &migrations)
        .unwrap_or_else(|e| panic!("failed to open/migrate database at {db_path:?}: {e}"));

    // Install the shared state the `#[server]` fns read via `use_state::<Db>()`.
    server::install_state(db);

    // Compose the router: the API, plus the built UI when `WEB_DIST` is set.
    let mut app: Router = server::router();
    if let Some(dist) = std::env::var_os("WEB_DIST").map(PathBuf::from) {
        let pkg_dir = dist.join("pkg");
        if !pkg_dir.exists() {
            eprintln!(
                "warning: {} doesn't exist yet -- run `idealyst build --web` (or `idealyst dev --web`) to produce the wasm bundle",
                pkg_dir.display()
            );
        }
        app = app.nest_service("/pkg", ServeDir::new(&pkg_dir)).fallback_service(
            ServeDir::new(&dist)
                .not_found_service(ServeDir::new(&dist).append_index_html_on_directories(true)),
        );
    }

    // Bind + serve.
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let host: IpAddr = std::env::var("HOST")
        .ok()
        .and_then(|h| h.parse().ok())
        .unwrap_or_else(|| [127, 0, 0, 1].into());
    let addr = SocketAddr::new(host, port);

    println!("discord-server listening on http://{addr} (db: {db_path:?})");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("server failed: {e}"));
}
