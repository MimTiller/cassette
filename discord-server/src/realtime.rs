//! Realtime fan-out and gateway authorization (server-only).
//!
//! A process-wide [`Hub`] tracks every open gateway socket per user. Anything
//! that produces an event (a channel message, a DM, a presence change) works
//! out WHO should hear it from the database at delivery time -- the members of
//! the server, the participants of the DM -- and hands the list to the hub.
//! Because the audience is recomputed per event there is no per-socket
//! subscription state to keep in sync: join a server and you start hearing it,
//! leave and you stop.
//!
//! Authorization for the gateway also lives here: a socket may only read or
//! post in a text channel of a server the (token-authenticated) user belongs
//! to; posts are size-limited and rate-limited per user.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::db::{Db, Message};
use crate::rate_limit::RateLimiter;
use crate::server_settings::WsServerMsg;

/// Identifies one open socket of a user.
pub type ConnId = u64;

/// Per-socket outbound queue depth. A client that falls this far behind drops
/// events (it can re-sync with `History`) rather than growing server memory.
const QUEUE_DEPTH: usize = 512;

/// Longest message body accepted, in characters.
pub const MAX_MESSAGE_CHARS: usize = 4000;

/// Posts allowed per user per window.
const SEND_LIMIT: usize = 20;
const SEND_WINDOW: Duration = Duration::from_secs(10);

/// The set of open gateway sockets, keyed by user id.
#[derive(Default)]
pub struct Hub {
    conns: Mutex<HashMap<i64, Vec<(ConnId, mpsc::Sender<WsServerMsg>)>>>,
    next: AtomicU64,
}

static HUB: OnceLock<Hub> = OnceLock::new();

/// The process-wide hub.
pub fn hub() -> &'static Hub {
    HUB.get_or_init(Hub::default)
}

impl Hub {
    /// Register a new socket for `user_id`; returns its id and the receiver of
    /// the events routed to it.
    pub fn register(&self, user_id: i64) -> (ConnId, mpsc::Receiver<WsServerMsg>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(QUEUE_DEPTH);
        self.conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(user_id)
            .or_default()
            .push((id, tx));
        (id, rx)
    }

    /// Remove a socket. Returns `true` when that was the user's LAST open
    /// socket (the caller then marks them offline).
    pub fn unregister(&self, user_id: i64, id: ConnId) -> bool {
        let mut map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        let Some(list) = map.get_mut(&user_id) else {
            return true;
        };
        list.retain(|(cid, _)| *cid != id);
        if list.is_empty() {
            map.remove(&user_id);
            true
        } else {
            false
        }
    }

    /// Deliver `msg` to every open socket of every user in `users` (each user
    /// once even if listed twice). Never blocks: a socket whose queue is full
    /// misses the event. Returns how many sockets it was queued on.
    pub fn publish(&self, users: &[i64], msg: &WsServerMsg) -> usize {
        let map = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        let mut seen: Vec<i64> = Vec::with_capacity(users.len());
        let mut queued = 0;
        for &u in users {
            if seen.contains(&u) {
                continue;
            }
            seen.push(u);
            if let Some(list) = map.get(&u) {
                for (_, tx) in list {
                    if tx.try_send(msg.clone()).is_ok() {
                        queued += 1;
                    }
                }
            }
        }
        queued
    }

    /// How many sockets `user_id` currently has open.
    pub fn connection_count(&self, user_id: i64) -> usize {
        self.conns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&user_id)
            .map_or(0, Vec::len)
    }

    /// Test hook: drop every open socket. The process-wide hub is shared by
    /// all e2e tests; a previous test's router tasks keep running after its
    /// `e2e_lock` guard is released and may still be holding sockets, which
    /// would make [`Hub::unregister`] stop reporting "last socket". Call this
    /// at the start of an e2e test (while holding the lock) for a clean slate.
    #[cfg(test)]
    pub fn reset(&self) {
        self.conns.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn send_limiter() -> &'static RateLimiter {
    static LIMITER: OnceLock<RateLimiter> = OnceLock::new();
    LIMITER.get_or_init(|| RateLimiter::new(SEND_LIMIT, SEND_WINDOW))
}

/// The user ids that belong to `server_id`.
pub fn server_audience(db: &Db, server_id: i64) -> Vec<i64> {
    db.server_members(server_id)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

/// Everyone who should hear about `user_id`'s presence: the user themself and
/// every member of every server they belong to.
pub fn presence_audience(db: &Db, user_id: i64) -> Vec<i64> {
    let mut out = vec![user_id];
    for (server_id, _, _) in db.servers_for_user(user_id) {
        out.extend(server_audience(db, server_id));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Tell the people who care that `user_id`'s presence became `status`.
pub fn announce_presence(db: &Db, user_id: i64, status: &str) {
    hub().publish(
        &presence_audience(db, user_id),
        &WsServerMsg::Presence {
            user_id,
            status: status.to_string(),
        },
    );
}

/// Check that `user_id` may use text channel `channel_id`: the channel must
/// exist, be a text channel, and belong to a server the user is a member of.
/// Returns the channel's server id.
pub fn authorize_text_channel(
    db: &Db,
    user_id: i64,
    channel_id: i64,
) -> Result<i64, &'static str> {
    let (_id, server_id, _name, kind, _pos, _parent) =
        db.channel_full(channel_id).ok_or("channel not found")?;
    if db.membership_role(server_id, user_id).is_none() {
        return Err("not a member of this server");
    }
    if kind != "text" {
        return Err("not a text channel");
    }
    Ok(server_id)
}

/// Validate, persist and fan out one channel message. The audience is every
/// member of the channel's server (including the sender's own sockets, which is
/// how the sender receives its echo).
pub fn post_channel_message(
    db: &Db,
    user_id: i64,
    channel_id: i64,
    content: &str,
) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("empty message".into());
    }
    if content.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!("message too long (max {MAX_MESSAGE_CHARS} characters)"));
    }
    if let Err(retry) = send_limiter().check(&user_id.to_string()) {
        return Err(format!("rate limited: retry in {}s", retry.as_secs() + 1));
    }
    let server_id = authorize_text_channel(db, user_id, channel_id).map_err(String::from)?;
    let created_at = now_millis();
    let message_id = db
        .insert_message(channel_id, user_id, content, created_at)
        .map_err(|e| e.to_string())?;
    hub().publish(
        &server_audience(db, server_id),
        &WsServerMsg::Message {
            message_id,
            channel_id,
            author_id: user_id,
            content: content.to_string(),
            created_at,
            edited_at: None,
            new: true,
        },
    );
    Ok(())
}

/// The recent history of a channel, if `user_id` may read it.
pub fn history_for(
    db: &Db,
    user_id: i64,
    channel_id: i64,
    limit: i64,
) -> Result<Vec<Message>, &'static str> {
    authorize_text_channel(db, user_id, channel_id)?;
    Ok(db.recent_messages(channel_id, limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Users 1..=4. Server 1: owner 1 + member 2, text channel 1, voice
    /// channel 2. Server 2: owner 3, text channel 3. User 4 is in no server.
    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = Db::migrate(&dir.path().join("t.db"), &migrations).unwrap();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES
                    (1,'alice','a@x',1),(2,'bob','b@x',1),(3,'carol','c@x',1),(4,'dave','d@x',1);
                 INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES
                    (1,'S1',1,NULL,1),(2,'S2',3,NULL,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                    (1,1,'owner',1),(1,2,'member',1),(2,3,'owner',1);
                 INSERT INTO channels (id, server_id, name, kind, position, parent_channel_id, created_at) VALUES
                    (1,1,'general','text',0,NULL,1),
                    (2,1,'lounge','voice',1,NULL,1),
                    (3,2,'other','text',0,NULL,1);",
            )
            .unwrap();
        (dir, db)
    }

    // ── pure hub behaviour ────────────────────────────────────────────────

    #[tokio::test]
    async fn hub_delivers_to_listed_users_only_and_reports_last_close() {
        let h = Hub::default();
        let (a1, mut ra1) = h.register(1);
        let (_a2, mut ra2) = h.register(1);
        let (_b, mut rb) = h.register(2);
        let (_c, mut rc) = h.register(3);
        let msg = WsServerMsg::Error { reason: "x".into() };
        // Users 1 and 2 (1 listed twice): 2 sockets for user 1 + 1 for user 2.
        assert_eq!(h.publish(&[1, 2, 1], &msg), 3);
        assert!(ra1.try_recv().is_ok() && ra2.try_recv().is_ok() && rb.try_recv().is_ok());
        assert!(rc.try_recv().is_err(), "user 3 was not in the audience");
        assert_eq!(h.connection_count(1), 2);
        assert!(!h.unregister(1, a1), "user 1 still has a second socket");
        assert!(h.unregister(2, _b), "user 2's only socket closed");
        assert_eq!(h.publish(&[2], &msg), 0);
    }

    #[tokio::test]
    async fn hub_drops_events_for_a_full_queue_instead_of_blocking() {
        let h = Hub::default();
        let (_id, mut rx) = h.register(1);
        let msg = WsServerMsg::Error { reason: "x".into() };
        for _ in 0..QUEUE_DEPTH {
            assert_eq!(h.publish(&[1], &msg), 1);
        }
        assert_eq!(h.publish(&[1], &msg), 0, "queue full: event dropped, not blocked");
        assert!(rx.try_recv().is_ok());
    }

    // ── authorization helpers ─────────────────────────────────────────────

    #[test]
    fn channel_authorization_rules() {
        let (_d, db) = setup();
        assert_eq!(authorize_text_channel(&db, 1, 1), Ok(1));
        assert_eq!(authorize_text_channel(&db, 2, 1), Ok(1));
        assert_eq!(authorize_text_channel(&db, 4, 1), Err("not a member of this server"));
        assert_eq!(authorize_text_channel(&db, 1, 3), Err("not a member of this server"));
        assert_eq!(authorize_text_channel(&db, 1, 2), Err("not a text channel"));
        assert_eq!(authorize_text_channel(&db, 1, 99), Err("channel not found"));
        assert!(history_for(&db, 4, 1, 50).is_err());
        assert!(history_for(&db, 1, 1, 50).is_ok());
    }

    #[test]
    fn posting_validates_size_emptiness_and_membership() {
        let (_d, db) = setup();
        assert_eq!(post_channel_message(&db, 1, 1, "   ").unwrap_err(), "empty message");
        let long = "x".repeat(MAX_MESSAGE_CHARS + 1);
        assert!(post_channel_message(&db, 1, 1, &long).unwrap_err().contains("too long"));
        assert!(post_channel_message(&db, 4, 1, "hi").unwrap_err().contains("not a member"));
        assert!(post_channel_message(&db, 1, 2, "hi").unwrap_err().contains("not a text"));
        assert!(post_channel_message(&db, 1, 1, "ok").is_ok());
        assert_eq!(db.recent_messages(1, 10).len(), 1, "only the valid post persisted");
    }

    // ---- PASTE INSIDE `mod tests` in realtime.rs (before its final closing brace) ----
    use crate::server_settings::WsClientMsg;
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::tungstenite::Message as WsMsg;

    type Ws = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn boot() -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, server::router()).await;
        });
        addr
    }

    fn hex(s: &str) -> String {
        s.bytes().map(|b| format!("{b:02x}")).collect()
    }

    /// Connect a gateway socket authenticated with `token` (the wire arg).
    async fn connect(addr: std::net::SocketAddr, token: &str) -> Result<Ws, String> {
        let args = hex(&serde_json::to_string(&[token]).unwrap());
        let url = format!("ws://{addr}/_srv/_ws/message_gateway?args={args}");
        tokio_tungstenite::connect_async(url)
            .await
            .map(|(ws, _)| ws)
            .map_err(|e| e.to_string())
    }

    async fn send(ws: &mut Ws, msg: &WsClientMsg) {
        ws.send(WsMsg::Text(serde_json::to_string(msg).unwrap()))
            .await
            .unwrap();
    }

    /// Read frames until `pred` matches one (or 3s pass) and return it.
    async fn next_matching(
        ws: &mut Ws,
        pred: impl Fn(&WsServerMsg) -> bool,
    ) -> Option<WsServerMsg> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, ws.next()).await {
                Ok(Some(Ok(WsMsg::Text(t)))) => {
                    let m: WsServerMsg = serde_json::from_str(&t).unwrap();
                    if pred(&m) {
                        return Some(m);
                    }
                }
                Ok(Some(Ok(_))) => continue,
                _ => return None,
            }
        }
    }

    /// True if NO frame satisfying `pred` arrives within 400ms.
    async fn hears_nothing(ws: &mut Ws, pred: impl Fn(&WsServerMsg) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, ws.next()).await {
                Ok(Some(Ok(WsMsg::Text(t)))) => {
                    if pred(&serde_json::from_str(&t).unwrap()) {
                        return false;
                    }
                }
                Ok(Some(Ok(_))) => continue,
                _ => return true,
            }
        }
    }

    /// POST a JSON args-tuple to `/_srv/<path>`, optionally with an
    /// `Authorization: Bearer` token and/or a legacy `x-user-id`.
    async fn http(
        addr: std::net::SocketAddr,
        path: &str,
        bearer: Option<&str>,
        legacy: Option<&str>,
        body: &str,
    ) -> serde_json::Value {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut r, mut w) = stream.into_split();
        let mut hdrs = String::new();
        if let Some(t) = bearer {
            hdrs.push_str(&format!("authorization: Bearer {t}\r\n"));
        }
        if let Some(u) = legacy {
            hdrs.push_str(&format!("x-user-id: {u}\r\n"));
        }
        let req = format!(
            "POST /_srv/{path} HTTP/1.1\r\nHost: {addr}\r\n{hdrs}\
             content-type: application/json\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{body}",
            body.len()
        );
        w.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).await.unwrap();
        let s = String::from_utf8(buf).unwrap();
        let (_, b) = s.split_once("\r\n\r\n").unwrap_or(("", ""));
        serde_json::from_str(b).unwrap_or(serde_json::Value::Null)
    }

    fn is_msg(content: &'static str) -> impl Fn(&WsServerMsg) -> bool {
        move |m| matches!(m, WsServerMsg::Message { content: c, new: true, .. } if c == content)
    }

    // ── end to end through the real router ───────────────────────────────

    #[tokio::test]
    async fn gateway_authenticates_authorizes_and_fans_out() {
        let (_d, db) = setup();
        let _guard = crate::e2e_lock::guard();
        crate::realtime::hub().reset();
        server::install_state(db.clone());
        crate::session::set_legacy_header_for_tests(Some(false));
        let addr = boot().await;

        let (t1, t2, t3, t4) = (
            crate::session::issue_token(1),
            crate::session::issue_token(2),
            crate::session::issue_token(3),
            crate::session::issue_token(4),
        );

        // 1. Bad / forged / missing tokens never get a working socket.
        for bad in ["", "garbage", "v1.1.9999999999.deadbeef"] {
            match connect(addr, bad).await {
                Err(_) => {}
                Ok(mut ws) => {
                    let r = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
                    assert!(
                        !matches!(r, Ok(Some(Ok(WsMsg::Text(_))))),
                        "token {bad:?} must not get frames"
                    );
                }
            }
        }
        assert_eq!(hub().connection_count(1), 0);

        // 2. Alice + Bob (server 1), Carol (server 2) and Dave (no server).
        let mut alice = connect(addr, &t1).await.expect("alice connects");
        let mut bob = connect(addr, &t2).await.expect("bob connects");
        let mut carol = connect(addr, &t3).await.expect("carol connects");
        let mut dave = connect(addr, &t4).await.expect("dave connects");

        // Bob hears Alice's presence... she was online before he connected, so
        // instead: Bob's own arrival is announced to Alice (a shared server).
        assert!(
            next_matching(&mut alice, |m| matches!(
                m, WsServerMsg::Presence { user_id: 2, status } if status == "online"
            ))
            .await
            .is_some(),
            "alice is told bob came online (shared server)"
        );

        // 3. Fan-out: Alice posts; Alice (echo) and Bob receive it.
        send(&mut alice, &WsClientMsg::Send { channel_id: 1, content: "hello bob".into() }).await;
        assert!(next_matching(&mut alice, is_msg("hello bob")).await.is_some(), "echo to sender");
        assert!(next_matching(&mut bob, is_msg("hello bob")).await.is_some(), "bob receives");
        // ...but users outside server 1 hear nothing.
        assert!(hears_nothing(&mut carol, is_msg("hello bob")).await, "carol is in another server");
        assert!(hears_nothing(&mut dave, is_msg("hello bob")).await, "dave is in no server");

        // 4. Authorization: a non-member cannot post or read; voice is not text.
        send(&mut dave, &WsClientMsg::Send { channel_id: 1, content: "sneaky".into() }).await;
        let err = next_matching(&mut dave, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason }) if reason.contains("not a member")));
        send(&mut dave, &WsClientMsg::History { channel_id: 1 }).await;
        let err = next_matching(&mut dave, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason }) if reason.contains("not a member")));
        send(&mut alice, &WsClientMsg::Send { channel_id: 2, content: "into voice".into() }).await;
        let err = next_matching(&mut alice, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason }) if reason.contains("not a text")));
        assert!(hears_nothing(&mut bob, is_msg("sneaky")).await);
        assert_eq!(db.recent_messages(1, 10).len(), 1, "only alice's valid post persisted");

        // 5. Members can read history; it replays the persisted message.
        send(&mut bob, &WsClientMsg::History { channel_id: 1 }).await;
        let h = next_matching(&mut bob, |m| matches!(m, WsServerMsg::Message { new: false, .. })).await;
        assert!(matches!(h, Some(WsServerMsg::Message { ref content, .. }) if content == "hello bob"));

        // 6. Presence: when Bob's only socket closes, Alice is told he is offline.
        drop(bob);
        assert!(
            next_matching(&mut alice, |m| matches!(
                m, WsServerMsg::Presence { user_id: 2, status } if status == "offline"
            ))
            .await
            .is_some(),
            "alice is told bob went offline"
        );
        assert!(hears_nothing(&mut carol, |m| matches!(m, WsServerMsg::Presence { user_id: 2, .. })).await);

        crate::session::set_legacy_header_for_tests(None);
    }

    #[tokio::test]
    async fn http_endpoints_require_a_valid_token_and_dms_are_pushed() {
        let (_d, db) = setup();
        let _guard = crate::e2e_lock::guard();
        crate::realtime::hub().reset();
        server::install_state(db.clone());
        crate::session::set_legacy_header_for_tests(Some(false));
        let addr = boot().await;
        let (t1, t2) = (crate::session::issue_token(1), crate::session::issue_token(2));

        // The old stand-in header is rejected; a forged token is rejected; a
        // real token works.
        let v = http(addr, "list_servers", None, Some("1"), "[null]").await;
        assert!(v.get("Err").is_some(), "legacy x-user-id must not authenticate: {v:?}");
        let v = http(addr, "list_servers", Some("v1.1.9999999999.deadbeef"), Some("1"), "[null]").await;
        assert!(v.get("Err").is_some(), "forged token must not authenticate: {v:?}");
        let v = http(addr, "list_servers", Some(&t1), None, "[null]").await;
        assert!(v.get("Ok").is_some(), "valid token authenticates: {v:?}");
        assert_eq!(v["Ok"].as_array().map(Vec::len), Some(1), "alice is in one server");
        // The token, not the body, decides who is acting: bob sees his own list.
        let v = http(addr, "list_servers", Some(&t2), None, "[null]").await;
        assert_eq!(v["Ok"].as_array().map(Vec::len), Some(1));

        // DM push: bob has a socket open; alice DMs him over plain HTTP.
        let mut bob = connect(addr, &t2).await.expect("bob connects");
        let dm = http(addr, "start_dm", Some(&t1), None, r#"[{"user_id":2}]"#).await;
        let dm_id = dm["Ok"]["channel_id"].as_i64().unwrap_or_else(|| panic!("start_dm failed: {dm:?}"));
        let body = format!(r#"[{{"channel_id":{dm_id},"content":"psst"}}]"#);
        let sent = http(addr, "send_dm_message", Some(&t1), None, &body).await;
        assert!(sent.get("Ok").is_some(), "send_dm_message: {sent:?}");
        let pushed = next_matching(&mut bob, |m| matches!(m, WsServerMsg::DmMessage { .. })).await;
        match pushed {
            Some(WsServerMsg::DmMessage { dm_id: d, author_id, content, .. }) => {
                assert_eq!((d, author_id, content.as_str()), (dm_id, 1, "psst"));
            }
            other => panic!("bob should be pushed the DM, got {other:?}"),
        }

        crate::session::set_legacy_header_for_tests(None);
    }
}
