//! Mock Discord data — the seed for the server rail, channel sidebar, and
//! message view. Everything is `&'static` so the reactive closures can hold
//! cheap references.
use crate::theme;

pub struct Server {
    pub id: u32,
    /// Sidebar sections in display order.
    pub categories: &'static [Category],
}

/// Look up a text channel by id across this server's categories.
impl Server {
    /// The first text channel in category order — the default selection when
    /// a server is clicked in the rail (voice channels aren't selectable).
    pub fn first_text_channel(&self) -> &TextChannel {
        for c in self.categories {
            if let Some(t) = c.text.first() {
                return t;
            }
        }
        panic!("server {} has no text channels", self.id)
    }

    /// A voice channel by id (None if the id is a text channel's or unknown).
    pub fn find_voice_channel(&self, id: u32) -> Option<&VoiceChannel> {
        for c in self.categories {
            if let Some(v) = c.voice.iter().find(|v| v.id == id) {
                return Some(v);
            }
        }
        None
    }
}

/// One sidebar section, Discord-style: "TEXT CHANNELS" / "VOICE CHANNELS".
pub struct Category {
    pub id: u32,
    pub text: &'static [TextChannel],
    pub voice: &'static [VoiceChannel],
}

pub struct TextChannel {
    pub id: u32,
}

/// A voice-channel row in the sidebar: name plus the connected member names
/// shown beneath it (Discord's voice layout, without the live voice stack).
pub struct VoiceChannel {
    pub id: u32,
    pub name: &'static str,
}

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

/// The signed-in user's static identity. The live presence status is NOT part
/// of this constant — it's app-level signal state (see `app()`), so the
/// profile strip and member panel can change it without touching the data.
pub struct User {
    pub name: &'static str,
    /// Hex tint for the avatar chip.
    pub color: &'static str,
}

pub const USER: User = User {
    name: "Nova",
    color: theme::ACCENT,
};

/// Unix seconds right now (wall clock) — from the runtime's installed time
/// source (`js Date` on web, `SystemTime` elsewhere). Used for message
/// timestamps of messages the user sends this session.
pub fn now_secs() -> i64 {
    runtime_shared::time::epoch_millis() / 1_000
}

pub static SERVERS: &[Server] = &[
    Server {
        id: 1,
        categories: &[
            Category {
                id: 1100,
                text: &[
                    TextChannel { id: 101 },
                    TextChannel { id: 102 },
                    TextChannel { id: 103 },
                ],
                voice: &[],
            },
            Category {
                id: 1200,
                text: &[],
                voice: &[
                    VoiceChannel { id: 104, name: "Lounge" },
                    VoiceChannel { id: 105, name: "Pairing Room" },
                ],
            },
        ],
    },
    Server {
        id: 2,
        categories: &[
            Category {
                id: 2100,
                text: &[TextChannel { id: 201 }, TextChannel { id: 202 }],
                voice: &[],
            },
            Category {
                id: 2200,
                text: &[],
                voice: &[VoiceChannel { id: 203, name: "Crit Room" }],
            },
        ],
    },
    Server {
        id: 3,
        categories: &[
            Category {
                id: 3100,
                text: &[TextChannel { id: 301 }],
                voice: &[],
            },
            Category {
                id: 3200,
                text: &[],
                voice: &[VoiceChannel { id: 302, name: "Basecamp" }],
            },
        ],
    },
];
