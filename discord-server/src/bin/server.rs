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

use axum::extract::{ConnectInfo, Path, Request};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get;

/// Stamp the TCP peer's IP on every request as `session::PEER_IP_HEADER`,
/// replacing any value the client sent, so the API's per-IP throttles key on
/// an address the client can't choose. (Behind a reverse proxy this would be
/// the proxy's address; this server is reached directly.)
async fn tag_peer_ip(ConnectInfo(peer): ConnectInfo<SocketAddr>, mut req: Request, next: Next) -> Response {
    if let Ok(v) = HeaderValue::from_str(&peer.ip().to_string()) {
        req.headers_mut().insert(discord_server::session::PEER_IP_HEADER, v);
    }
    next.run(req).await
}

/// GET /media/{name}: serve ONLY content-addressed names (see
/// [`discord_server::media::http_response`] — anything else, including
/// `..`, is a 404 without touching the filesystem).
async fn media_route(Path(name): Path<String>) -> Response {
    discord_server::media::http_response(discord_server::media::media_dir(), &name)
}

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

    // Session signing secret: DISCORD_SESSION_SECRET, else a 0600 file next to
    // the database (so tokens survive a restart).
    let mut secret_path = db_path.clone().into_os_string();
    secret_path.push(".session-secret");
    discord_server::session::init_persistent_secret(std::path::Path::new(&secret_path))
        .unwrap_or_else(|e| panic!("failed to load/create the session secret at {secret_path:?}: {e}"));

    // One-time, idempotent legacy pass: convert inline `data:` values in
    // users.avatar_url, servers.icon_url and message_attachments.url into
    // media files + paths (decoded bytes AS-IS, no validation; a base64
    // body that fails to decode is left in place and counted).
    let (converted, unchanged) = discord_server::media::migrate_existing(&db);
    if converted > 0 || unchanged > 0 {
        println!(
            "media legacy pass: converted {converted} inline image values to media files, left {unchanged} unchanged"
        );
    }

    // Session tokens carry the account's token_version (see `session`).
    let versions_db = db.clone();
    discord_server::session::init_token_versions(move |user_id| versions_db.token_version(user_id));

    // Install the shared state the `#[server]` fns read via `use_state::<Db>()`.
    server::install_state(db);

    // Compose the router: the API, plus the built UI when `WEB_DIST` is set.
    let mut app: Router = server::router();
    // Content-addressed image bytes live on disk; serve them under /media/*.
    app = app.route("/media/*name", get(media_route));
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
    let app = app.layer(axum::middleware::from_fn(tag_peer_ip));
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap_or_else(|e| panic!("server failed: {e}"));
}
