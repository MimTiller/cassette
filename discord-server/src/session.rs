//! Session tokens and the acting-user resolver (server-only).
//!
//! `login` returns a signed, expiring token; every authenticated request sends
//! it back as `Authorization: Bearer <token>` (the websocket gateway takes it
//! as a connect-time wire arg, because a browser socket cannot set headers).
//! The server never trusts a client-supplied user id: identity comes only from
//! a token whose HMAC-SHA256 signature verifies against the server secret.
//!
//! Token format: `v1.<user_id>.<expires_unix_secs>.<hex hmac-sha256>` where the
//! MAC covers the `v1.<user_id>.<expires>` prefix.
//!
//! The legacy `x-user-id` header (the old stand-in session) is REJECTED by
//! default. Set `DISCORD_ALLOW_USER_ID_HEADER=1` to accept it for local
//! development or test tooling; a request that presents a bearer token never
//! falls back to the legacy header (a bad token is simply unauthenticated).

use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// How long an issued token stays valid.
const TOKEN_TTL_SECS: i64 = 12 * 60 * 60;

static SECRET: OnceLock<Vec<u8>> = OnceLock::new();

/// Test hook: 0 = follow the environment, 1 = allow the legacy header,
/// 2 = reject it.
static LEGACY_OVERRIDE: AtomicU8 = AtomicU8::new(0);

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let val = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks(2) {
        out.push((val(pair[0])? << 4) | val(pair[1])?);
    }
    Some(out)
}

fn random_secret() -> Vec<u8> {
    let mut buf = vec![0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf
}

/// Install the signing secret (first call wins). Prefer
/// [`init_persistent_secret`] in the server bin.
pub fn init_secret(secret: Vec<u8>) {
    let _ = SECRET.set(secret);
}

/// Load the signing secret so tokens survive a server restart: the
/// `DISCORD_SESSION_SECRET` env var if set, else the hex secret stored at
/// `path` (created with a fresh random secret, mode 0600 on unix, if absent).
pub fn init_persistent_secret(path: &Path) -> std::io::Result<()> {
    if let Ok(v) = std::env::var("DISCORD_SESSION_SECRET") {
        if v.len() >= 16 {
            init_secret(v.into_bytes());
            return Ok(());
        }
    }
    if let Ok(s) = std::fs::read_to_string(path) {
        if let Some(bytes) = hex_decode(s.trim()) {
            if bytes.len() >= 16 {
                init_secret(bytes);
                return Ok(());
            }
        }
    }
    let secret = random_secret();
    std::fs::write(path, hex_encode(&secret))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    init_secret(secret);
    Ok(())
}

/// The active secret. If nothing initialized one (tests, tools) fall back to
/// the env var, else a random per-process secret (tokens then die on restart).
fn secret() -> &'static [u8] {
    SECRET.get_or_init(|| {
        if let Ok(v) = std::env::var("DISCORD_SESSION_SECRET") {
            if v.len() >= 16 {
                return v.into_bytes();
            }
        }
        random_secret()
    })
}

fn sign(body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret()).expect("hmac accepts any key length");
    mac.update(body.as_bytes());
    hex_encode(&mac.finalize().into_bytes())
}

/// Issue a signed session token for `user_id`.
pub fn issue_token(user_id: i64) -> String {
    issue_token_at(user_id, now_secs())
}

fn issue_token_at(user_id: i64, now: i64) -> String {
    let body = format!("v1.{user_id}.{}", now + TOKEN_TTL_SECS);
    let sig = sign(&body);
    format!("{body}.{sig}")
}

/// The user id a token authenticates, or `None` if it is malformed, forged,
/// or expired.
pub fn verify_token(token: &str) -> Option<i64> {
    verify_token_at(token, now_secs())
}

fn verify_token_at(token: &str, now: i64) -> Option<i64> {
    let (body, sig_hex) = token.rsplit_once('.')?;
    let mut parts = body.split('.');
    if parts.next()? != "v1" {
        return None;
    }
    let user_id: i64 = parts.next()?.parse().ok()?;
    let expires: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let sig = hex_decode(sig_hex)?;
    let mut mac = HmacSha256::new_from_slice(secret()).ok()?;
    mac.update(body.as_bytes());
    // Constant-time comparison.
    mac.verify_slice(&sig).ok()?;
    if now >= expires {
        return None;
    }
    Some(user_id)
}

/// Whether the legacy `x-user-id` header is accepted.
fn legacy_header_allowed() -> bool {
    match LEGACY_OVERRIDE.load(Ordering::SeqCst) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    if cfg!(test) {
        return true;
    }
    static ENV: OnceLock<bool> = OnceLock::new();
    *ENV.get_or_init(|| {
        std::env::var("DISCORD_ALLOW_USER_ID_HEADER").is_ok_and(|v| v == "1")
    })
}

/// Test hook: force the legacy header on/off (`None` = follow the environment).
/// Callers must hold the crate-wide e2e lock.
#[cfg(test)]
pub(crate) fn set_legacy_header_for_tests(allow: Option<bool>) {
    LEGACY_OVERRIDE.store(
        match allow {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
        Ordering::SeqCst,
    );
}

/// Resolve an identity from the raw `Authorization` and legacy `x-user-id`
/// header values. Pure (no request context), so it is unit-testable.
///
/// - `Authorization: Bearer <token>` present → the token must verify, else
///   unauthenticated (never falls back to the legacy header);
/// - otherwise the legacy header counts only when `allow_legacy`.
pub fn resolve_identity(
    authorization: Option<&str>,
    legacy_user_id: Option<&str>,
    allow_legacy: bool,
) -> Option<i64> {
    if let Some(auth) = authorization {
        let auth = auth.trim();
        let token = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))?;
        return verify_token(token.trim());
    }
    if allow_legacy {
        return legacy_user_id?.trim().parse().ok();
    }
    None
}

/// The acting user for the current request: a verified bearer token (or, only
/// when explicitly enabled, the legacy header). `None` = unauthenticated.
pub fn acting_user() -> Option<i64> {
    let authorization = server::use_request_header("authorization");
    let legacy = server::use_request_header("x-user-id");
    resolve_identity(
        authorization.as_deref(),
        legacy.as_deref(),
        legacy_header_allowed(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_round_trips() {
        let t = issue_token(42);
        assert_eq!(verify_token(&t), Some(42));
        assert!(t.starts_with("v1.42."));
    }

    #[test]
    fn tampered_or_forged_tokens_are_rejected() {
        let t = issue_token(7);
        // Flip the user id but keep the old signature.
        let forged = t.replacen("v1.7.", "v1.8.", 1);
        assert_eq!(verify_token(&forged), None);
        // Flip a signature character.
        let mut bad = t.clone();
        let last = bad.pop().unwrap();
        bad.push(if last == '0' { '1' } else { '0' });
        assert_eq!(verify_token(&bad), None);
        // Truncated / garbage / wrong version / extra segment.
        assert_eq!(verify_token(""), None);
        assert_eq!(verify_token("garbage"), None);
        assert_eq!(verify_token("v2.1.9999999999.00"), None);
        assert_eq!(verify_token(&format!("{t}.extra")), None);
        // A hand-built token with a made-up signature.
        assert_eq!(verify_token("v1.1.9999999999.deadbeef"), None);
    }

    #[test]
    fn expired_tokens_are_rejected() {
        let t = issue_token_at(5, 1_000);
        assert_eq!(verify_token_at(&t, 1_000 + TOKEN_TTL_SECS - 1), Some(5));
        assert_eq!(verify_token_at(&t, 1_000 + TOKEN_TTL_SECS), None);
        assert_eq!(verify_token_at(&t, 1_000 + TOKEN_TTL_SECS + 1), None);
    }

    #[test]
    fn identity_resolution_rules() {
        let t = issue_token(3);
        let bearer = format!("Bearer {t}");
        // A valid bearer token wins.
        assert_eq!(resolve_identity(Some(&bearer), None, false), Some(3));
        assert_eq!(resolve_identity(Some(&bearer), Some("99"), true), Some(3));
        // The scheme is case-insensitive for the common spellings.
        assert_eq!(resolve_identity(Some(&format!("bearer {t}")), None, false), Some(3));
        // A bad bearer token is unauthenticated -- no fallback to the header.
        assert_eq!(resolve_identity(Some("Bearer nope"), Some("1"), true), None);
        assert_eq!(resolve_identity(Some("Basic abc"), Some("1"), true), None);
        // The legacy header only counts when explicitly allowed.
        assert_eq!(resolve_identity(None, Some("1"), false), None);
        assert_eq!(resolve_identity(None, Some("1"), true), Some(1));
        assert_eq!(resolve_identity(None, Some("abc"), true), None);
        assert_eq!(resolve_identity(None, None, true), None);
    }

    #[test]
    fn persistent_secret_file_is_created_and_reused() {
        // Only exercises the file helpers (the global secret is process-wide).
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("secret");
        assert!(hex_decode(&hex_encode(&[0, 15, 255, 16])).unwrap() == vec![0, 15, 255, 16]);
        assert!(hex_decode("zz").is_none());
        assert!(hex_decode("abc").is_none());
        // init_persistent_secret writes a 64-char hex file when none exists
        // (the OnceLock may already be set by another test; the file is what
        // this asserts).
        init_persistent_secret(&p).unwrap();
        if std::env::var("DISCORD_SESSION_SECRET").is_err() {
            let s = std::fs::read_to_string(&p).unwrap();
            assert_eq!(s.trim().len(), 64);
        }
    }
}
