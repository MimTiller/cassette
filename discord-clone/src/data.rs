//! Shared client data model: the presence status vocabulary and the wall-clock
//! timestamp helper. Server/channel/member data all comes from the backend.
use crate::theme;

/// The status the user picks from the user panel's menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Online,
    Idle,
    Dnd,
    Invisible,
}

impl Status {
    pub const ALL: [Status; 4] = [Status::Online, Status::Idle, Status::Dnd, Status::Invisible];

    /// Profile-strip name, Discord's casing: "Online", "Idle", …
    pub fn name(self) -> &'static str {
        match self {
            Status::Online => "Online",
            Status::Idle => "Idle",
            Status::Dnd => "Do Not Disturb",
            Status::Invisible => "Invisible",
        }
    }

    /// The menu's line under the name, where Discord has one.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Status::Dnd => Some("You will not receive notifications"),
            Status::Invisible => Some("You will appear offline"),
            _ => None,
        }
    }

    /// The status dot color (Discord's palette).
    pub fn color(self) -> &'static str {
        match self {
            Status::Online => theme::SUCCESS,
            Status::Idle => theme::WARNING,
            Status::Dnd => theme::DANGER,
            Status::Invisible => theme::TEXT_FAINT,
        }
    }

    /// The gateway's name for it (`WsClientMsg::SetStatus`).
    pub fn wire(self) -> &'static str {
        match self {
            Status::Online => "online",
            Status::Idle => "idle",
            Status::Dnd => "dnd",
            Status::Invisible => "invisible",
        }
    }

    /// From the gateway's name; anything unknown is Online.
    pub fn from_wire(s: &str) -> Self {
        Status::ALL.into_iter().find(|st| st.wire() == s).unwrap_or(Status::Online)
    }
}

impl Default for Status {
    /// A fresh user is Online: the default presence for a new `Signal<Status>`
    /// (e.g. a component's default-valued status prop) and for any user before
    /// they pick a different status.
    fn default() -> Self {
        Status::Online
    }
}

/// Unix seconds right now (wall clock) — from the runtime's installed time
/// source (`js Date` on web, `SystemTime` elsewhere). Used for message
/// timestamps of messages the user sends this session.
pub fn now_secs() -> i64 {
    runtime_shared::time::epoch_millis() / 1_000
}
