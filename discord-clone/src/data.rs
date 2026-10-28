//! Mock Discord data — the seed for the server rail, channel sidebar, and
//! message view. Everything is `&'static` so the reactive closures can hold
//! cheap references. Swap this for a network fetch later; the `app()` tree
//! only reads these, so the UI stays untouched.
use crate::theme;



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
    /// Slowmode cooldown in seconds. `None` = slowmode off (the channel shows
    /// no `SlowmodeIndicator` at all); `Some(n)` = on, with an `n`-second
    /// cooldown that the header's indicator displays. Static seed data for
    /// now — a real value would come from the gateway. Only a couple of the
    /// mock channels set it, so the indicator is gated per-channel rather than
    /// unconditional (the "every channel shows a pill" anti-pattern this field
    /// exists to prevent).
    pub slowmode_secs: Option<u32>,
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
            Status::Online => theme::SUCCESS,
            Status::Idle => theme::WARNING,
            Status::DoNotDisturb => theme::DANGER,
            Status::Offline => theme::TEXT_FAINT,
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
    /// Per-server role surfaced as a badge in the member panel
    /// (the mock seed assigns one per server; the live panel gets it
    /// from the backend `MemberInfo` instead).
    pub role: &'static str,
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
    name: "Nova",
    color: theme::ACCENT,
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
            // The signed-in user owns the mock servers — surfaced as "Owner".
            role: "Owner",
        }
    }
}



// (mock profile table removed — the mock member panel/profile panel is gone)


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
                        topic: "General discussion — be kind to each other!",
                        slowmode_secs: None,
                        unread: 3,
                        mentions: 0,
                        messages: &[
                            Message {
                                author: "miri",
                                color: theme::ACCENT,
                                body: "the classic. the compiler was being generous",
                                age_secs: 3600 * 9 + 95,
                            },
                            Message {
                                author: "dyno",
                                color: "#57f287",
                                body: "just spent 20 minutes fighting a lifetime that was actually a missing Clone",
                                age_secs: 3600 * 9 + 42,
                            },
                            Message {
                                author: "miri",
                                color: theme::ACCENT,
                                body: "morning all ☕ anyone up for a borrow-checker war story?",
                                age_secs: 3600 * 9,
                            },
                        ],
                    },
                    TextChannel {
                        id: 102,
                        name: "showcase",
                        topic: "Ship something and tell us about it",
                        slowmode_secs: Some(30),
                        unread: 0,
                        mentions: 0,
                        messages: &[
                            // Content that warrants two message-formatting widgets
                            // on a GENUINE, deliberate instance (not a per-row
                            // splice): the body carries a real URL (so
                            // `message_row` renders a `LinkEmbedPreview` card
                            // below it) AND wraps one clause in Discord's
                            // `||…||` spoiler markers (so `message_body` renders
                            // that clause as a click-to-reveal `SpoilerTag`).
                            // No other seed message has a URL or a spoiler, so
                            // this is the single row that shows both.
                            Message {
                                author: "dyno",
                                color: "#57f287",
                                body: "here it is: https://github.com/dyno/wasm-forge — and the one gotcha that ate an hour was ||passing the linker flag through the profile section, not the target one|| but it's in the README",
                                age_secs: 3600 * 7 + 75,
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
                        slowmode_secs: None,
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
                            Member { name: "miri", color: theme::ACCENT, status: Status::Online, role: "Owner" },
                            Member { name: "dyno", color: "#57f287", status: Status::Online, role: "Moderator" },
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
            Member { name: "miri", color: theme::ACCENT, status: Status::Online, role: "Owner" },
            Member { name: "dyno", color: "#57f287", status: Status::Online, role: "Moderator" },
            Member { name: "nara", color: "#eb459e", status: Status::Idle, role: "Admin" },
            Member { name: "wren", color: "#3ba55c", status: Status::DoNotDisturb, role: "Member" },
            Member { name: "juno", color: "#fee75c", status: Status::Offline, role: "Member" },
        ],
    },
    Server {
        id: 2,
        name: "Pixel Forge",
        initial: "P",
        accent: theme::ACCENT,
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
                        slowmode_secs: Some(60),
                        unread: 2,
                        mentions: 0,
                        messages: &[
                            Message {
                                author: "sol",
                                color: "#ed4245",
                                body: "that glow on the emblem is doing a lot — love it",
                                age_secs: 3600 * 5 + 15,
                            },
                            Message {
                                author: "kai",
                                color: "#fee75c",
                                body: "first pass on the title screen, still a mess but the lighting feels right",
                                age_secs: 3600 * 5,
                            },
                        ],
                    },
                    TextChannel {
                        id: 202,
                        name: "assets",
                        topic: "Share textures, fonts, and sounds",
                        slowmode_secs: None,
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
                    speakers: &[Member { name: "kai", color: "#fee75c", status: Status::Online, role: "Owner" }],
                }],
            },
        ],
        members: &[
            Member { name: "kai", color: "#fee75c", status: Status::Online, role: "Owner" },
            Member { name: "sol", color: "#ed4245", status: Status::Online, role: "Moderator" },
            Member { name: "pix", color: "#eb459e", status: Status::DoNotDisturb, role: "Admin" },
            Member { name: "ash", color: "#57f287", status: Status::Offline, role: "Member" },
        ],
    },
    Server {
        id: 3,
        name: "Trailhead",
        initial: "T",
        accent: theme::SUCCESS,
        categories: &[
            Category {
                id: 3100,
                title: "TEXT CHANNELS",
                voice: &[],
                text: &[TextChannel {
                    id: 301,
                    name: "trips",
                    topic: "Plan the next outing",
                    slowmode_secs: None,
                    unread: 0,
                    mentions: 0,
                    messages: &[
                        Message {
                            author: "rio",
                            color: "#3ba55c",
                            body: "bring layers, the ridge is windy",
                            age_secs: 3600 * 16 + 30,
                        },
                        Message {
                            author: "rio",
                            color: "#3ba55c",
                            body: "weekend hike is a go — 14 people confirmed, meeting at the north lot 7am",
                            age_secs: 3600 * 16,
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
            Member { name: "rio", color: "#3ba55c", status: Status::Online, role: "Owner" },
            Member { name: "sam", color: theme::ACCENT, status: Status::Idle, role: "Moderator" },
            Member { name: "lee", color: theme::WARNING, status: Status::Offline, role: "Member" },
        ],
    },
];
