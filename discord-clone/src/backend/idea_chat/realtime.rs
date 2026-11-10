//! The second backend's event stream: the `EventStreamApi` surface, opening
//! the idea-chat live connection instead of the discord-server gateway socket.

use crate::backend::idea_chat::live;

impl EventStreamApi for IdeaChatBackend {
    fn connect(token: String) -> Gateway {
        Gateway {
            handle: live.connect(token),
        }
    }
}
