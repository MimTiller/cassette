//! Invite-code helpers for the invite flow (mock/stub: no backend calls yet).
//!
//! A code is 8 characters from an unambiguous alphabet (no 0/O/1/I). Codes are
//! deterministic per server id so the UI can show a stable mock invite link.

const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Length of every invite code.
pub const INVITE_CODE_LEN: usize = 8;

/// Deterministic mock invite code for a server id.
pub fn generate_invite_code(server_id: u32) -> String {
    let mut x = (server_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    let mut out = String::with_capacity(INVITE_CODE_LEN);
    for _ in 0..INVITE_CODE_LEN {
        x ^= x >> 33;
        x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        x ^= x >> 29;
        out.push(ALPHABET[(x % ALPHABET.len() as u64) as usize] as char);
    }
    out
}

/// The shareable link for a code.
pub fn invite_link(code: &str) -> String {
    format!("https://discord.gg/{code}")
}

/// Accepts a bare code or a full invite link; returns the normalized
/// (upper-case) code, or `None` if it is not a well-formed invite code.
pub fn parse_invite_code(input: &str) -> Option<String> {
    let t = input.trim();
    let code = t.rsplit('/').next().unwrap_or(t).to_ascii_uppercase();
    if code.len() == INVITE_CODE_LEN && code.bytes().all(|b| ALPHABET.contains(&b)) {
        Some(code)
    } else {
        None
    }
}

/// Create a real invite for `server_id` with the given `expires_at` (UTC epoch
/// seconds, `None` = never expires) and `max_uses` (redemption cap, `None` =
/// unlimited). Thin front-end for the backend's permission-checked
/// `#[server]` `create_invite` fn (moderator-or-above may mint an invite), the
/// same shape as `moderation.rs`: this crate depends on `discord-server` with
/// default features, so this compiles to the codegen'd RPC stub. Every call
/// authenticates as the signed-in user via the session token the `server` SDK
/// attaches. The backend's rejection message comes back as the `Err` string.
pub async fn create_invite(
    server_id: i64,
    expires_at: Option<i64>,
    max_uses: Option<i64>,
) -> Result<discord_server::invites::Invite, String> {
    let input = discord_server::invites::CreateInviteInput {
        server_id,
        expires_at,
        max_uses,
    };
    discord_server::invites::create_invite(input)
        .await
        .map_err(|e| e.to_string())
}

/// Join a server by redeeming an invite `code` (the code alone names the
/// server, like a real Discord invite). Same thin-RPC-stub shape as
/// `create_invite`: the backend rejects an unknown/expired/fully-spent code
/// and its message comes back as the `Err` string. On success the acting user
/// is a plain member of the named server.
pub async fn redeem_invite(
    code: &str,
) -> Result<discord_server::invites::RedeemResult, String> {
    let input = discord_server::invites::RedeemInviteInput {
        code: code.to_string(),
    };
    discord_server::invites::redeem_invite(input)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_codes_are_valid_and_stable() {
        let a = generate_invite_code(1);
        assert_eq!(a.len(), INVITE_CODE_LEN);
        assert_eq!(a, generate_invite_code(1));
        assert_ne!(a, generate_invite_code(2));
        assert_eq!(parse_invite_code(&a), Some(a));
    }

    #[test]
    fn parses_links_and_normalizes_case() {
        let code = generate_invite_code(7);
        let link = invite_link(&code);
        assert_eq!(parse_invite_code(&link), Some(code.clone()));
        assert_eq!(parse_invite_code(&format!("  {}  ", code.to_lowercase())), Some(code));
    }

    #[test]
    fn rejects_malformed_codes() {
        assert_eq!(parse_invite_code(""), None);
        assert_eq!(parse_invite_code("short"), None);
        assert_eq!(parse_invite_code("ABCDEFG0"), None); // '0' is not in the alphabet
        assert_eq!(parse_invite_code("https://discord.gg/"), None);
    }
}
