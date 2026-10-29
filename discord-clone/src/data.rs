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

/// Presence status. Only the online state is used anywhere in the app.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Online,
}

impl Status {
    /// Profile-strip name, Discord's casing: "Online", "Idle", …
    pub fn name(self) -> &'static str {
        match self {
            Status::Online => "Online",
        }
    }

    /// The status dot color (Discord's palette).
    pub fn color(self) -> &'static str {
        match self {
            Status::Online => theme::SUCCESS,
        }
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
