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
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
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
/// `..`, is a 404 without touching the filesystem), and only to a signed-in
/// user, so a leaked image link (a DM's or a private channel's) shows nothing
/// to a stranger. The browser sends the session cookie with `<img>` loads.
/// Mark a bundle file whose name carries a build hash
/// (`discord_clone_bg.7865764ab0eca34a.wasm`) as cacheable for a year; dev
/// builds use plain names and get no caching header.
async fn cache_hashed_bundle(request: Request, next: Next) -> Response {
    let hashed = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .is_some_and(|name| name.split('.').any(|part| part.len() == 16 && part.bytes().all(|b| b.is_ascii_hexdigit())));
    let mut response = next.run(request).await;
    if hashed && response.status().is_success() {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
    }
    response
}

async fn media_route(Path(name): Path<String>, headers: HeaderMap) -> Response {
    let header = |n: header::HeaderName| headers.get(n).and_then(|v| v.to_str().ok());
    if discord_server::session::identity_from_headers(header(header::AUTHORIZATION), header(header::COOKIE)).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    discord_server::media::http_response(discord_server::media::media_dir(), &name)
}

#[tokio::main]
async fn main() {
    // `server reset-link <username>` prints a one-time password reset link
    // for the account and exits (see `password_resets`). The link starts
    // with PUBLIC_URL, else localhost on PORT. Arguments are checked before
    // the database is opened, so a usage error creates nothing.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let reset_user = match args.first().map(String::as_str) {
        Some("reset-link") => match args.get(1) {
            Some(name) => Some(name.clone()),
            None => {
                eprintln!("usage: server reset-link <username>");
                std::process::exit(2);
            }
        },
        _ => None,
    };

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

    if let Some(username) = reset_user {
        use discord_server::password_resets::{now_ms, ResetService, URL_PARAM};
        match ResetService::new(db).issue(&username, now_ms()) {
            Ok(token) => {
                let base = std::env::var("PUBLIC_URL").unwrap_or_else(|_| {
                    format!("http://localhost:{}", std::env::var("PORT").unwrap_or_else(|_| "3000".into()))
                });
                println!("{}/?{URL_PARAM}={token}", base.trim_end_matches('/'));
                println!("Works once, for 24 hours. Earlier links for {username} no longer work.");
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

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
        // A release build writes `.br` / `.gz` siblings; send those to
        // browsers that accept them. Files whose names carry the build hash
        // never change, so browsers may keep them for a year.
        let pkg = Router::new()
            .fallback_service(ServeDir::new(&pkg_dir).precompressed_br().precompressed_gzip())
            .layer(axum::middleware::from_fn(cache_hashed_bundle));
        app = app.nest("/pkg", pkg).fallback_service(
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
