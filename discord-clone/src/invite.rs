//! Invite-code helpers for the invite flow.
//!
//! Codes are minted on the real backend via `create_invite`; this file only
//! parses what users paste and names the RPC stubs.

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
        .map_err(|e| crate::errors::friendly(&e))
}

/// The invite code in what someone pasted: a bare code, or a link ending in
/// one (`https://…/qcyftbyg`, `discord.gg/qcyftbyg/`, with or without a query).
pub fn code_from_input(input: &str) -> String {
    let s = input.trim();
    let s = s.split(['?', '#']).next().unwrap_or(s).trim_end_matches('/');
    s.rsplit('/').next().unwrap_or(s).trim().to_string()
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
        .map_err(|e| crate::errors::friendly(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_links_give_their_code() {
        assert_eq!(code_from_input("  qcyftbyg "), "qcyftbyg");
        assert_eq!(code_from_input("discord.gg/qcyftbyg"), "qcyftbyg");
        assert_eq!(code_from_input("https://chat.example.com/invite/qcyftbyg/?ref=x"), "qcyftbyg");
        assert_eq!(code_from_input(""), "");
    }
}
