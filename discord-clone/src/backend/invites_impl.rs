//! Today's invite operations: the `discord-server` RPC stubs.

use super::*;

impl InvitesApi for CassetteBackend {
    async fn create_invite(
        server_id: i64,
        expires_at: Option<i64>,
        max_uses: Option<i64>,
    ) -> Result<Invite, ServerError> {
        discord_server::invites::create_invite(
            discord_server::invites::CreateInviteInput {
                server_id,
                expires_at,
                max_uses,
            },
        ).await
    }
    async fn redeem_invite(code: String) -> Result<RedeemResult, ServerError> {
        discord_server::invites::redeem_invite(
            discord_server::invites::RedeemInviteInput { code },
        ).await
    }
}
