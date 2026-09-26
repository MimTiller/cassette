//! Rate limiting for the auth endpoints (signup + login).
//!
//! Two complementary throttles, both tuned to slow down brute-force and
//! credential-stuffing:
//!
//!   * **Per-IP sliding window** — the primary throttle. Every IP gets at most
//!     `max_requests` attempts in any rolling `window` of time. This caps the
//!     rate at which *any* account can be probed from a single source, which
//!     is exactly the credential-stuffing pattern (many accounts, one IP).
//!   * **Per-account login-failure floor** — enforced in the data layer
//!     (`db::Db::login_failures` / `record_login_failure`). Even if an attacker
//!     rotates IPs, a single account can't be hammered forever: after
//!     `max_failures` wrong passwords within `window`, logins for that account
//!     are rejected until the window slides past the oldest recorded failure.
//!
//! Both are pure policy; the *delivery* of a 429 (the HTTP status +
//! `Retry-After` header) is the job of the dispatch hook in
//! [`crate::server_settings`], which short-circuits with
//! `TransportError::Server { status: 429, .. }` before the handler body runs.
//!
//! The limiter is feature-gated to `server` (like the rest of the data layer);
//! the wasm client never compiles it.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One limiter instance holds a fixed policy and a mutable per-key attempt
/// log. It is `Send + Sync` and cheap to clone (shared state behind `Arc`).
#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<Inner>>,
}

struct Inner {
    /// Per-key log of attempt instants, oldest first, pruned to the window.
    buckets: std::collections::HashMap<String, VecDeque<Instant>>,
    max_requests: usize,
    window: Duration,
    /// Injectable clock: `Some` in tests (advance explicitly), `None` in
    /// production (wall clock). Lets tests trip the sliding window without
    /// sleeping.
    now_override: Option<Instant>,
    base: Instant,
}

impl Inner {
    fn now(&self) -> Instant {
        // Production: wall clock. The limiter is built with `base = now`, so
        // `now - base` is well-formed. Tests drive `now_override` from `base`.
        self.now_override.unwrap_or_else(Instant::now)
    }
}

impl RateLimiter {
    /// A limiter allowing at most `max_requests` attempts per key within a
    /// rolling `window`.
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(Inner {
                buckets: std::collections::HashMap::new(),
                max_requests,
                window,
                now_override: None,
                base: Instant::now(),
            })),
        }
    }

    /// Test-only: override the current time (relative to construction) so a
    /// sliding window can be advanced without sleeping. `t` is the offset from
    /// the limiter's creation instant.
    #[cfg(test)]
    pub fn set_now(&self, t: Duration) {
        let mut s = self.lock();
        s.now_override = Some(s.base + t);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Returns `Ok(())` if the attempt is allowed, or `Err(retry_after)` if the
    /// caller is over the limit — `retry_after` is how long to wait before the
    /// next attempt is permitted (i.e. until the oldest in-window attempt ages
    /// out). The allowed attempt is *recorded* as a side effect, so an allowed
    /// call consumes one unit of budget.
    pub fn check(&self, key: &str) -> Result<(), Duration> {
        let mut s = self.lock();
        let now = s.now();
        // Copy the policy fields into locals BEFORE taking the mutable `entry`
        // borrow of `s.buckets` below — `Duration`/`usize` are `Copy`, and the
        // borrow checker won't let us read `s.window` / `s.max_requests` while
        // that borrow is live.
        let window = s.window;
        let max_requests = s.max_requests;
        let cutoff = now - window;
        let b = s.buckets.entry(key.to_string()).or_default();
        // Prune attempts that have slid out of the window.
        while let Some(front) = b.front() {
            if *front <= cutoff {
                b.pop_front();
            } else {
                break;
            }
        }
        if b.len() >= max_requests {
            // Over budget. The next slot frees when the oldest in-window
            // attempt exits the window: `oldest + window - now`.
            let oldest = b.front().copied().unwrap();
            let retry = (oldest + window - now).max(Duration::from_secs(1));
            return Err(retry);
        }
        b.push_back(now);
        Ok(())
    }

    /// Clear a key's recorded attempts (e.g. after a successful login).
    pub fn reset(&self, key: &str) {
        let mut s = self.lock();
        s.buckets.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_then_rejects() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        for _ in 0..3 {
            assert!(rl.check("1.2.3.4").is_ok(), "under budget should pass");
        }
        let err = rl.check("1.2.3.4").expect_err("4th attempt must be rejected");
        assert!(err >= Duration::from_secs(1), "retry-after should be >= 1s");
    }

    #[test]
    fn independent_keys_do_not_share_budget() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.check("a").is_ok());
        assert!(rl.check("a").is_ok());
        assert!(rl.check("a").is_err(), "a is exhausted");
        assert!(rl.check("b").is_ok(), "b has its own budget");
    }

    #[test]
    fn sliding_window_releases_after_time_passes() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.check("x").is_ok());
        assert!(rl.check("x").is_ok());
        assert!(rl.check("x").is_err(), "exhausted at t=0");
        // Advance past the 60s window: both attempts age out.
        rl.set_now(Duration::from_secs(61));
        assert!(rl.check("x").is_ok(), "window has slid past the old attempts");
    }

    #[test]
    fn reset_clears_budget() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        assert!(rl.check("y").is_ok());
        assert!(rl.check("y").is_err());
        rl.reset("y");
        assert!(rl.check("y").is_ok(), "reset restored budget");
    }
}
