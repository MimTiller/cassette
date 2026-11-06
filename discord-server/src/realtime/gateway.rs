//! End-to-end gateway tests: the message flow through the real router and a
//! live WebSocket socket.

#[cfg(test)]
mod gateway_tests {
    use std::time::Duration;

    use tokio::net::TcpStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::mpsc;
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    use futures_util::{SinkExt, StreamExt};
    use crate::realtime::test_fixtures::{setup};
    use crate::realtime::{hub, post_channel_message};
    use crate::server_settings::{WsServerMsg, WsClientMsg};
    use server::{server, ServerError};

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
                m, WsServerMsg::Presence { user_id: 2, status, .. } if status == "online"
            ))
            .await
            .is_some(),
            "alice is told bob came online (shared server)"
        );

        // 3. Fan-out: Alice posts; Alice (echo) and Bob receive it.
        send(&mut alice, &WsClientMsg::Send { channel_id: 1, content: "hello bob".into(), reply_to_message_id: None, attachments: Vec::new() }).await;
        assert!(next_matching(&mut alice, is_msg("hello bob")).await.is_some(), "echo to sender");
        assert!(next_matching(&mut bob, is_msg("hello bob")).await.is_some(), "bob receives");
        // ...but users outside server 1 hear nothing.
        assert!(hears_nothing(&mut carol, is_msg("hello bob")).await, "carol is in another server");
        assert!(hears_nothing(&mut dave, is_msg("hello bob")).await, "dave is in no server");

        // 4. Authorization: a non-member cannot post or read; voice is not text.
        send(&mut dave, &WsClientMsg::Send { channel_id: 1, content: "sneaky".into(), reply_to_message_id: None, attachments: Vec::new() }).await;
        let err = next_matching(&mut dave, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason, .. }) if reason.contains("not a member")));
        send(&mut dave, &WsClientMsg::History { channel_id: 1, before_id: None }).await;
        let err = next_matching(&mut dave, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason, .. }) if reason.contains("not a member")));
        send(&mut alice, &WsClientMsg::Send { channel_id: 2, content: "into voice".into(), reply_to_message_id: None, attachments: Vec::new() }).await;
        let err = next_matching(&mut alice, |m| matches!(m, WsServerMsg::Error { .. })).await;
        assert!(matches!(err, Some(WsServerMsg::Error { ref reason, .. }) if reason.contains("not a text")));
        assert!(hears_nothing(&mut bob, is_msg("sneaky")).await);
        assert_eq!(db.recent_messages(1, 10).len(), 1, "only alice's valid post persisted");

        // 5. Members can read history; it replays the persisted message.
        send(&mut bob, &WsClientMsg::History { channel_id: 1, before_id: None }).await;
        let h = next_matching(&mut bob, |m| matches!(m, WsServerMsg::Message { new: false, .. })).await;
        assert!(matches!(h, Some(WsServerMsg::Message { ref content, .. }) if content == "hello bob"));

        // 6. Presence: when Bob's only socket closes, Alice is told he is offline.
        drop(bob);
        assert!(
            next_matching(&mut alice, |m| matches!(
                m, WsServerMsg::Presence { user_id: 2, status, .. } if status == "offline"
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
