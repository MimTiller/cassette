//! The process-wide hub of open gateway sockets.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use tokio::sync::mpsc;

use crate::server_settings::WsServerMsg;

/// Identifies one open socket of a user.
pub type ConnId = u64;

/// Per-socket outbound queue depth. A client that falls this far behind drops
/// events (it can re-sync with `History`) rather than growing server memory.
const QUEUE_DEPTH: usize = 512;

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

#[cfg(test)]
mod tests {
    use super::*;

    // ── pure hub behaviour ────────────────────────────────────────────────

    #[tokio::test]
    async fn hub_delivers_to_listed_users_only_and_reports_last_close() {
        let h = Hub::default();
        let (a1, mut ra1) = h.register(1);
        let (_a2, mut ra2) = h.register(1);
        let (_b, mut rb) = h.register(2);
        let (_c, mut rc) = h.register(3);
        let msg = WsServerMsg::Error { reason: "x".into(), seq: 1 };
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
        let msg = WsServerMsg::Error { reason: "x".into(), seq: 1 };
        for _ in 0..QUEUE_DEPTH {
            assert_eq!(h.publish(&[1], &msg), 1);
        }
        assert_eq!(h.publish(&[1], &msg), 0, "queue full: event dropped, not blocked");
        assert!(rx.try_recv().is_ok());
    }
}
