//! User-facing text for a failed server call: the server's own message for a
//! refusal ("invalid username or password"), and plain wording for transport
//! failures, instead of `ServerError`'s developer-facing `Display`
//! ("server fn failed: …").

use server::ServerError;

pub(crate) fn friendly(e: &ServerError) -> String {
    match e {
        ServerError::Failed(message) => message.clone(),
        ServerError::Network(_) => "Can't reach the server. Check your connection and try again.".to_string(),
        ServerError::IncompatibleVersion { .. } => "This page is out of date. Reload to update it.".to_string(),
        other => format!("Something went wrong ({other})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_show_the_server_message() {
        assert_eq!(friendly(&ServerError::Failed("invalid username or password".into())), "invalid username or password");
        assert!(friendly(&ServerError::Network("dns".into())).starts_with("Can't reach the server"));
    }
}
