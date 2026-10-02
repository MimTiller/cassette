//! Mock Discord data — the seed for the server rail, channel sidebar, and
//! message view. Everything is `&'static` so the reactive closures can hold
//! cheap references. Swap this for a network fetch later; the `app()` tree
//! only reads these, so the UI stays untouched.



pub struct Server {
    pub id: u32,
    pub name: &'static str,
    /// Single-letter avatar shown in the rail.
    pub initial: &'static str,
    /// Brand tint for the avatar chip (reserved for the rail accent; not yet
    /// rendered — allowed so the data model stays complete).
    #[allow(dead_code)]
    pub accent: &'static str,
    /// Sidebar sections in display order. Text channels carry messages;
    /// voice channels are presence rows (non-interactive in this build).
    pub categories: &'static [Category],
    pub members: &'static [Member],
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
        // The mock data always has at least one text channel per server;
        // `unwrap_or_else(|| panic!(…))` would leak a debug-only message.
        panic!("server {} has no text channels", self.id)
    }

    /// A text channel by id (None if the id is a voice channel's or unknown).
    pub fn find_text_channel(&self, id: u32) -> Option<&TextChannel> {
        for c in self.categories {
            if let Some(t) = c.text.iter().find(|t| t.id == id) {
                return Some(t);
            }
        }
        None
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

    /// Total unread messages across this server's text channels — the mock
    /// feed for the server-rail badge (voice channels carry no messages).
    pub fn total_unread(&self) -> u32 {
        self.categories
            .iter()
            .flat_map(|c| c.text.iter().map(|t| t.unread))
            .sum()
    }

    /// Total @mentions across this server's text channels.
    pub fn total_mentions(&self) -> u32 {
        self.categories
            .iter()
            .flat_map(|c| c.text.iter().map(|t| t.mentions))
            .sum()
    }
}

/// One sidebar section, Discord-style: "TEXT CHANNELS" / "VOICE CHANNELS".
pub struct Category {
    pub id: u32,
    pub title: &'static str,
    pub text: &'static [TextChannel],
    pub voice: &'static [VoiceChannel],
}

pub struct TextChannel {
    pub id: u32,
    pub name: &'static str,
    pub topic: &'static str,
    pub messages: &'static [Message],
    /// Mock unread count for the sidebar badge. Static seed data for now —
    /// the real value will come from the gateway; the app clears it locally
    /// when the user opens the channel (see `app.rs` `read_marks`).
    pub unread: u32,
    /// Count of @mentions specifically — the number shown in the red badge.
    /// `unread` (total unread) only brightens the channel name now.
    pub mentions: u32,
}

/// A voice-channel row in the sidebar: name plus the connected member names
/// shown beneath it (Discord's voice layout, without the live voice stack).
pub struct VoiceChannel {
    pub id: u32,
    pub name: &'static str,
    pub speakers: &'static [Member],
}

pub struct Message {
    pub author: &'static str,
    /// Hex for the author chip / name.
    pub color: &'static str,
    pub body: &'static str,
    /// How old the message is, in **seconds** (a plain static offset, not an
    /// absolute instant) — resolved against the live wall clock at render time
    /// by `Message::ts_secs(now)`. Storing age rather than a frozen Unix time
    /// is what keeps "Today at …" true whenever the app is opened: a `const`
    /// Unix timestamp would go stale the moment the real clock moves past it.
    pub age_secs: i64,
}

/// Resolve a message's age to a real Unix-seconds instant, given "now".
impl Message {
    pub fn ts_secs(&self, now_secs: i64) -> i64 {
        now_secs - self.age_secs
    }
}

/// Presence status, Discord's four states. Ordering doubles as the
/// member-list grouping order (online first, offline last).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Online,
    Idle,
    DoNotDisturb,
    Offline,
}

impl Status {
    /// Profile-strip name, Discord's casing: "Online", "Idle", …
    pub fn name(self) -> &'static str {
        match self {
            Status::Online => "Online",
            Status::Idle => "Idle",
            Status::DoNotDisturb => "Do Not Disturb",
            Status::Offline => "Offline",
        }
    }

    /// Group header text, e.g. "ONLINE — 4".
    pub fn label(self) -> &'static str {
        match self {
            Status::Online => "ONLINE",
            Status::Idle => "IDLE",
            Status::DoNotDisturb => "DO NOT DISTURB",
            Status::Offline => "OFFLINE",
        }
    }

    /// The status dot color (Discord's palette).
    pub fn color(self) -> &'static str {
        match self {
            Status::Online => "#23a559",
            Status::Idle => "#f0b232",
            Status::DoNotDisturb => "#f23f43",
            Status::Offline => "#80848e",
        }
    }

    /// The next status in the profile-strip click cycle:
    /// Online → Idle → DoNotDisturb → back to Online.
    pub fn next(self) -> Self {
        match self {
            Status::Online => Status::Idle,
            Status::Idle => Status::DoNotDisturb,
            Status::DoNotDisturb => Status::Online,
            Status::Offline => Status::Online,
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

pub struct Member {
    pub name: &'static str,
    /// Hex tint for the avatar chip.
    pub color: &'static str,
    pub status: Status,
}

/// The signed-in user's static identity. The live presence status is NOT part
/// of this constant — it's app-level signal state (see `app()`), so the
/// profile strip and member panel can change it without touching the data.
/// The user is also the author of every message sent this session.
pub struct User {
    pub name: &'static str,
    /// Hex tint for the avatar chip.
    pub color: &'static str,
}

pub const USER: User = User {
    name: "you",
    color: "#5865f2",
};

/// The current user rendered as a regular member entry (the member panel and
/// voice rows treat "you" like any other presence row). `status` is the
/// live presence from app-level signal state — this constant never holds it.
impl User {
    pub fn as_member(&self, status: Status) -> Member {
        Member {
            name: self.name,
            color: self.color,
            status,
        }
    }
}

/// Message ages are stored inline on each `Message` as `age_secs` (see the
/// field doc on `Message::age_secs`) — resolved to real instants at render
/// time. All sit within the last ~20h so they read "Today at …" in any
/// reasonable local timezone.

/// Unix seconds right now (wall clock) — from the runtime's installed time
/// source (`js Date` on web, `SystemTime` elsewhere). Used for message
/// timestamps of messages the user sends this session.
pub fn now_secs() -> i64 {
    runtime_shared::time::epoch_millis() / 1_000
}

// --- Direct messages (mock) --------------------------------------------------
// A `Dm` is a one-to-one conversation between the signed-in user (`USER`) and a
// single other participant. The list + conversation view (`crate::dm_view`)
// reads only these; sent DMs are kept separately in an app-level session log.
// Same shape as `Message` (age stored inline, resolved at render) but kept as a
// distinct type so the DM model can evolve (read cursors, typing, …) without
// touching channel `Message`s.

pub struct DmMessage {
    pub author: &'static str,
    /// Hex for the author chip / name.
    pub color: &'static str,
    pub body: &'static str,
    /// How old the message is, in **seconds** — resolved against the live wall
    /// clock at render time (see `Message::age_secs`).
    pub age_secs: i64,
}

/// Resolve a DM message's age to a real Unix-seconds instant, given "now".
impl DmMessage {
    pub fn ts_secs(&self, now_secs: i64) -> i64 {
        now_secs - self.age_secs
    }
}

/// One direct-message conversation. `user` is the other participant (the
/// signed-in user is always the implicit other side); `messages` is in send
/// order (oldest first).
pub struct Dm {
    pub id: u32,
    /// The other participant in this conversation.
    pub user: Member,
    /// Messages so far, oldest first.
    pub messages: &'static [DmMessage],
}

/// The age of this conversation's most recent message (for the list's "recent
/// conversations" sort). A conversation with no messages reads as the most
/// recent (0) so an empty thread sorts to the top.
impl Dm {
    pub fn last_age_secs(&self) -> i64 {
        self.messages.last().map(|m| m.age_secs).unwrap_or(0)
    }
}

/// Look up a DM conversation by id (None if unknown).
pub fn find_dm(id: u32) -> Option<&'static Dm> {
    DMS.iter().find(|d| d.id == id)
}

/// Mock DM seed. The ids are chosen so `dm_view::mock_unread`'s stub
/// (unread on 1, 2, 4) lines up with real conversations.
pub static DMS: &[Dm] = &[
    Dm {
        id: 1,
        user: Member { name: "miri", color: "#5865f2", status: Status::Online },
        messages: &[
            DmMessage {
                author: "miri",
                color: "#5865f2",
                body: "still hitting that borrow error from this morning?",
                age_secs: 3600 * 3,
            },
            DmMessage {
                author: "you",
                color: "#5865f2",
                body: "almost — the lifetime on the helper was the whole thing",
                age_secs: 3600 * 3 - 60,
            },
            DmMessage {
                author: "miri",
                color: "#5865f2",
                body: "the classic. send me the patch when it lands",
                age_secs: 3600 * 3 - 90,
            },
        ],
    },
    Dm {
        id: 2,
        user: Member { name: "dyno", color: "#57f287", status: Status::Online },
        messages: &[
            DmMessage {
                author: "dyno",
                color: "#57f287",
                body: "that wasm tool you linked — does it handle wasm-bindgen out of the box?",
                age_secs: 3600 * 5,
            },
            DmMessage {
                author: "you",
                color: "#5865f2",
                body: "yeah, it shells out to wasm-bindgen-cli under the hood",
                age_secs: 3600 * 5 - 45,
            },
            DmMessage {
                author: "dyno",
                color: "#57f287",
                body: "perfect, that's exactly what I needed",
                age_secs: 3600 * 5 - 80,
            },
            DmMessage {
                author: "you",
                color: "#5865f2",
                body: "let me know if it chokes on your setup",
                age_secs: 3600 * 5 - 120,
            },
        ],
    },
    Dm {
        id: 3,
        user: Member { name: "rio", color: "#3ba55c", status: Status::Idle },
        messages: &[
            DmMessage {
                author: "rio",
                color: "#3ba55c",
                body: "did you grab the extra pair of layers for the hike?",
                age_secs: 3600 * 12,
            },
            DmMessage {
                author: "you",
                color: "#5865f2",
                body: "got a rain shell + a base layer, should be plenty",
                age_secs: 3600 * 12 - 30,
            },
        ],
    },
    Dm {
        id: 4,
        user: Member { name: "sol", color: "#ed4245", status: Status::Online },
        messages: &[
            DmMessage {
                author: "sol",
                color: "#ed4245",
                body: "the new emblem palette — thoughts on a slightly warmer gold?",
                age_secs: 3600 * 8,
            },
            DmMessage {
                author: "you",
                color: "#5865f2",
                body: "warmer reads better against the dark bg, yes",
                age_secs: 3600 * 8 - 60,
            },
            DmMessage {
                author: "sol",
                color: "#ed4245",
                body: "awesome, updating the swatch now",
                age_secs: 3600 * 8 - 90,
            },
        ],
    },
    Dm {
        id: 5,
        user: Member { name: "juno", color: "#fee75c", status: Status::Offline },
        messages: &[
            DmMessage {
                author: "juno",
                color: "#fee75c",
                body: "was out all day, seeing a ton of messages — what did I miss?",
                age_secs: 3600 * 18,
            },
            DmMessage {
                author: "you",
                color: "#5865f2",
                body: "the showcase thread mostly, and the hike plan",
                age_secs: 3600 * 18 - 40,
            },
        ],
    },
];

pub static SERVERS: &[Server] = &[
    Server {
        id: 1,
        name: "Rust & Beers",
        initial: "R",
        accent: "#b45a2c",
        categories: &[
            Category {
                id: 1100,
                title: "TEXT CHANNELS",
                voice: &[],
                text: &[
                    TextChannel {
                        id: 101,
                        name: "general",
                        topic: "Lounge — talk about anything, Rust strongly encouraged",
                        unread: 3,
                        mentions: 0,
                        messages: &[
                            Message {
                                author: "miri",
                                color: "#5865f2",
                                body: "morning all ☕ anyone up for a borrow-checker war story?",
                                age_secs: 3600 * 9,
                            },
                            Message {
                                author: "dyno",
                                color: "#57f287",
                                body: "just spent 20 minutes fighting a lifetime that was actually a missing Clone",
                                age_secs: 3600 * 9 + 42,
                            },
                            Message {
                                author: "miri",
                                color: "#5865f2",
                                body: "the classic. the compiler was being generous",
                                age_secs: 3600 * 9 + 95,
                            },
                        ],
                    },
                    TextChannel {
                        id: 102,
                        name: "showcase",
                        topic: "Ship something and tell us about it",
                        unread: 0,
                        mentions: 0,
                        messages: &[
                            Message {
                                author: "dyno",
                                color: "#57f287",
                                body: "wrote a tiny WASM build tool over the weekend — 400 lines, zero deps",
                                age_secs: 3600 * 7,
                            },
                            Message {
                                author: "nara",
                                color: "#eb459e",
                                body: "link? I need that energy",
                                age_secs: 3600 * 7 + 20,
                            },
                        ],
                    },
                    TextChannel {
                        id: 103,
                        name: "random",
                        topic: "Everything else",
                        unread: 1,
                        mentions: 1,
                        messages: &[Message {
                            author: "nara",
                            color: "#eb459e",
                            body: "hot take: semicolons are load-bearing and we should stop pretending otherwise",
                            age_secs: 3600 * 20,
                        }],
                    },
                ],
            },
            Category {
                id: 1200,
                title: "VOICE CHANNELS",
                text: &[],
                voice: &[
                    VoiceChannel {
                        id: 104,
                        name: "Lounge",
                        speakers: &[
                            Member { name: "miri", color: "#5865f2", status: Status::Online },
                            Member { name: "dyno", color: "#57f287", status: Status::Online },
                        ],
                    },
                    VoiceChannel {
                        id: 105,
                        name: "Pairing Room",
                        speakers: &[],
                    },
                ],
            },
        ],
        members: &[
            Member { name: "miri", color: "#5865f2", status: Status::Online },
            Member { name: "dyno", color: "#57f287", status: Status::Online },
            Member { name: "nara", color: "#eb459e", status: Status::Idle },
            Member { name: "wren", color: "#3ba55c", status: Status::DoNotDisturb },
            Member { name: "juno", color: "#fee75c", status: Status::Offline },
        ],
    },
    Server {
        id: 2,
        name: "Pixel Forge",
        initial: "P",
        accent: "#5865f2",
        categories: &[
            Category {
                id: 2100,
                title: "TEXT CHANNELS",
                voice: &[],
                text: &[
                    TextChannel {
                        id: 201,
                        name: "work-in-progress",
                        topic: "Post renders, get feedback",
                        unread: 2,
                        mentions: 0,
                        messages: &[
                            Message {
                                author: "kai",
                                color: "#fee75c",
                                body: "first pass on the title screen, still a mess but the lighting feels right",
                                age_secs: 3600 * 5,
                            },
                            Message {
                                author: "sol",
                                color: "#ed4245",
                                body: "that glow on the emblem is doing a lot — love it",
                                age_secs: 3600 * 5 + 15,
                            },
                        ],
                    },
                    TextChannel {
                        id: 202,
                        name: "assets",
                        topic: "Share textures, fonts, and sounds",
                        unread: 5,
                        mentions: 0,
                        messages: &[Message {
                            author: "sol",
                            color: "#ed4245",
                            body: "dropped a free pixel-font pack into the drive, grab it while it's there",
                            age_secs: 3600 * 3,
                        }],
                    },
                ],
            },
            Category {
                id: 2200,
                title: "VOICE CHANNELS",
                text: &[],
                voice: &[VoiceChannel {
                    id: 203,
                    name: "Crit Room",
                    speakers: &[Member { name: "kai", color: "#fee75c", status: Status::Online }],
                }],
            },
        ],
        members: &[
            Member { name: "kai", color: "#fee75c", status: Status::Online },
            Member { name: "sol", color: "#ed4245", status: Status::Online },
            Member { name: "pix", color: "#eb459e", status: Status::DoNotDisturb },
            Member { name: "ash", color: "#57f287", status: Status::Offline },
        ],
    },
    Server {
        id: 3,
        name: "Trailhead",
        initial: "T",
        accent: "#23a559",
        categories: &[
            Category {
                id: 3100,
                title: "TEXT CHANNELS",
                voice: &[],
                text: &[TextChannel {
                    id: 301,
                    name: "trips",
                    topic: "Plan the next outing",
                    unread: 0,
                    mentions: 0,
                    messages: &[
                        Message {
                            author: "rio",
                            color: "#3ba55c",
                            body: "weekend hike is a go — 14 people confirmed, meeting at the north lot 7am",
                            age_secs: 3600 * 16,
                        },
                        Message {
                            author: "rio",
                            color: "#3ba55c",
                            body: "bring layers, the ridge is windy",
                            age_secs: 3600 * 16 + 30,
                        },
                    ],
                }],
            },
            Category {
                id: 3200,
                title: "VOICE CHANNELS",
                text: &[],
                voice: &[VoiceChannel {
                    id: 302,
                    name: "Basecamp",
                    speakers: &[],
                }],
            },
        ],
        members: &[
            Member { name: "rio", color: "#3ba55c", status: Status::Online },
            Member { name: "sam", color: "#5865f2", status: Status::Idle },
            Member { name: "lee", color: "#f0b232", status: Status::Offline },
        ],
    },
];
