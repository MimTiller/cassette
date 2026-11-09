//! Today's event stream: the gateway socket.

use super::*;

impl EventStreamApi for CassetteBackend {
    fn connect(token: String) -> Gateway {
        Gateway {
            handle: discord_server::server_settings::message_gateway(token),
        }
    }
}
