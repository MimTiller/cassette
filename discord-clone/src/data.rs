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
            // The signed-in user owns the mock servers — surfaced as "Owner".
            role: "Owner",
        }
    }
}

/// Per-member display data shown in the member profile panel and the member
/// list's role badge. Keyed by the member's **name** (the profile panel and
/// member rows address members by name), so the profile panel can look a
/// member up from the selected-name signal. Every field is `&'static str`
/// (or a plain integer) so values can be passed into reactive `#[prop]`s
/// without allocation.
///
/// Each mock member carries distinct values here — different activity, role,
/// boost tier, bio, and nickname — so no two profiles render byte-for-byte
/// identical. (Previously every profile re-used one hardcoded literal.)
#[allow(dead_code)] // pub mock data: fields ship with the profile fixture set
pub struct MemberProfile {
    /// The rich-presence status verb, e.g. "Playing", "Listening to".
    pub activity_verb: &'static str,
    /// The rich-presence detail, e.g. the game / app / track title.
    pub activity_detail: &'static str,
    /// The member's role name shown on the role badge.
    pub role_name: &'static str,
    /// The role badge fill (each role carries its own accent color).
    pub role_color: &'static str,
    /// The "about you" bio shown (and editable) in the profile panel.
    pub bio: &'static str,
    /// The server boost-level badge tier (0-3), rendered as "Level {n}".
    pub boost_level: u32,
    /// The member's display nickname (distinct from their account name).
    pub nickname: &'static str,
    /// The sender name on the member's friend-request card.
    pub friend_sender: &'static str,
}

/// The fallback profile for a name with no explicit entry (e.g. the
/// signed-in user "you") — the previous single shared literal, kept as the
/// default so an unknown name still renders a coherent profile.
pub const DEFAULT_MEMBER_PROFILE: MemberProfile = MemberProfile {
    activity_verb: "Playing",
    activity_detail: "Some Game Title",
    role_name: "Member",
    role_color: "#949ba4",
    bio: "Tell us about yourself...",
    boost_level: 2,
    nickname: "Nova",
    friend_sender: "Fen",
};

/// The per-member mock profile table — one row per mock member, keyed by name.
static MEMBER_PROFILES: &[(&str, MemberProfile)] = &[
    (
        "miri",
        MemberProfile {
            activity_verb: "Playing",
            activity_detail: "Hollow Knight",
            role_name: "Moderator",
            role_color: "#5865f2",
            bio: "Borrow-checker whisperer. Ask me about lifetimes, I won't blink.",
            boost_level: 2,
            nickname: "miri",
            friend_sender: "dyno",
        },
    ),
    (
        "dyno",
        MemberProfile {
            activity_verb: "Listening to",
            activity_detail: "Lo-fi beats",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Building tiny WASM tools for fun. Zero-deps or it didn't happen.",
            boost_level: 1,
            nickname: "dyno",
            friend_sender: "sol",
        },
    ),
    (
        "nara",
        MemberProfile {
            activity_verb: "Playing",
            activity_detail: "Stardew Valley",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Semicolons are load-bearing (fight me). Weekend farmhand.",
            boost_level: 0,
            nickname: "nara",
            friend_sender: "miri",
        },
    ),
    (
        "wren",
        MemberProfile {
            activity_verb: "Watching",
            activity_detail: "RustConf talks",
            role_name: "Admin",
            role_color: "#23a559",
            bio: "Do not disturb — I'm mid-session. Ping me after the conference ends.",
            boost_level: 3,
            nickname: "wren",
            friend_sender: "juno",
        },
    ),
    (
        "juno",
        MemberProfile {
            activity_verb: "Playing",
            activity_detail: "Celeste",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Out most of the day, sorry for the late replies. Speedrunner-ish.",
            boost_level: 1,
            nickname: "juno",
            friend_sender: "nara",
        },
    ),
    (
        "kai",
        MemberProfile {
            activity_verb: "Playing",
            activity_detail: "Blender",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Rendering things, mostly badly. Title screen lighting feels right though.",
            boost_level: 1,
            nickname: "kai",
            friend_sender: "pix",
        },
    ),
    (
        "sol",
        MemberProfile {
            activity_verb: "Working on",
            activity_detail: "A pixel font",
            role_name: "Moderator",
            role_color: "#5865f2",
            bio: "Emblem colorist. Warmer golds, colder blues. Drop free packs in #assets.",
            boost_level: 2,
            nickname: "sol",
            friend_sender: "kai",
        },
    ),
    (
        "pix",
        MemberProfile {
            activity_verb: "Playing",
            activity_detail: "Aseprite",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Pixel art, one sprite at a time. Ask me for the font pack link.",
            boost_level: 0,
            nickname: "pix",
            friend_sender: "sol",
        },
    ),
    (
        "ash",
        MemberProfile {
            activity_verb: "Listening to",
            activity_detail: "Synthwave",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Gone a while, back soon. Leave a render in WIP and I'll critique it.",
            boost_level: 1,
            nickname: "ash",
            friend_sender: "kai",
        },
    ),
    (
        "rio",
        MemberProfile {
            activity_verb: "Playing",
            activity_detail: "Trail Maps",
            role_name: "Moderator",
            role_color: "#5865f2",
            bio: "Organizing the next hike. Bring layers — the ridge is windy.",
            boost_level: 2,
            nickname: "rio",
            friend_sender: "sam",
        },
    ),
    (
        "sam",
        MemberProfile {
            activity_verb: "Playing",
            activity_detail: "Chess",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Just here for the weekend trip. Meeting at the north lot, 7am?",
            boost_level: 0,
            nickname: "sam",
            friend_sender: "rio",
        },
    ),
    (
        "lee",
        MemberProfile {
            activity_verb: "Working on",
            activity_detail: "A trail log",
            role_name: "Member",
            role_color: "#949ba4",
            bio: "Offline a lot, sorry! Will confirm the hike when I'm back.",
            boost_level: 0,
            nickname: "lee",
            friend_sender: "sam",
        },
    ),
];

/// Look up a member's profile display data by name. Returns the matching row,
/// or [`DEFAULT_MEMBER_PROFILE`] for an unknown name (e.g. the signed-in user
/// "you", which is not in the mock table).
pub fn member_profile(name: &str) -> &'static MemberProfile {
    MEMBER_PROFILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, p)| p)
        .unwrap_or(&DEFAULT_MEMBER_PROFILE)
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
        user: Member { name: "miri", color: "#5865f2", status: Status::Online, role: "Member" },
        messages: &[
            DmMessage {
                author: "miri",
                color: "#5865f2",
                body: "still hitting that borrow error from this morning?",
                age_secs: 3600 * 3,
            },
            DmMessage {
                author: "Nova",
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
            DmMessage {
                author: "miri",
                color: "#5865f2",
                // A GIF sent as a DM: the row renders it as the image
                // (message_body) and the list preview shows "GIF".
                body: "https://media.giphy.com/media/JIX9t2j0ZTN9S/giphy.gif#480x360",
                age_secs: 60,
            },
        ],
    },
    Dm {
        id: 2,
        user: Member { name: "dyno", color: "#57f287", status: Status::Online, role: "Member" },
        messages: &[
            DmMessage {
                author: "dyno",
                color: "#57f287",
                body: "that wasm tool you linked — does it handle wasm-bindgen out of the box?",
                age_secs: 3600 * 5,
            },
            DmMessage {
                author: "Nova",
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
                author: "Nova",
                color: "#5865f2",
                body: "let me know if it chokes on your setup",
                age_secs: 3600 * 5 - 120,
            },
        ],
    },
    Dm {
        id: 3,
        user: Member { name: "rio", color: "#3ba55c", status: Status::Idle, role: "Member" },
        messages: &[
            DmMessage {
                author: "rio",
                color: "#3ba55c",
                body: "did you grab the extra pair of layers for the hike?",
                age_secs: 3600 * 12,
            },
            DmMessage {
                author: "Nova",
                color: "#5865f2",
                body: "got a rain shell + a base layer, should be plenty",
                age_secs: 3600 * 12 - 30,
            },
        ],
    },
    Dm {
        id: 4,
        user: Member { name: "sol", color: "#ed4245", status: Status::Online, role: "Member" },
        messages: &[
            DmMessage {
                author: "sol",
                color: "#ed4245",
                body: "the new emblem palette — thoughts on a slightly warmer gold?",
                age_secs: 3600 * 8,
            },
            DmMessage {
                author: "Nova",
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
        user: Member { name: "juno", color: "#fee75c", status: Status::Offline, role: "Member" },
        messages: &[
            DmMessage {
                author: "juno",
                color: "#fee75c",
                body: "was out all day, seeing a ton of messages — what did I miss?",
                age_secs: 3600 * 18,
            },
            DmMessage {
                author: "Nova",
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
                        topic: "General discussion — be kind to each other!",
                        slowmode_secs: None,
                        unread: 3,
                        mentions: 0,
                        messages: &[
                            Message {
                                author: "miri",
                                color: "#5865f2",
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
                                color: "#5865f2",
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
                            Member { name: "miri", color: "#5865f2", status: Status::Online, role: "Owner" },
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
            Member { name: "miri", color: "#5865f2", status: Status::Online, role: "Owner" },
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
            Member { name: "sam", color: "#5865f2", status: Status::Idle, role: "Moderator" },
            Member { name: "lee", color: "#f0b232", status: Status::Offline, role: "Member" },
        ],
    },
];
