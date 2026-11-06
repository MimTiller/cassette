//! Invite-code helpers for the invite flow.
//!
//! Codes are minted and redeemed on the real backend through the seam
//! (`crate::backend::create_invite` / `crate::backend::redeem_invite`); this
//! file only parses what users paste.

/// The invite code in what someone pasted: a bare code, or a link ending in
/// one (`https://…/qcyftbyg`, `discord.gg/qcyftbyg/`, with or without a query).
pub fn code_from_input(input: &str) -> String {
    let s = input.trim();
    let s = s.split(['?', '#']).next().unwrap_or(s).trim_end_matches('/');
    s.rsplit('/').next().unwrap_or(s).trim().to_string()
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
