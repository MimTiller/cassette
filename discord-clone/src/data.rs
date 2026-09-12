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
    pub channels: &'static [Channel],
    pub members: &'static [Member],
}

pub struct Channel {
    pub id: u32,
    pub name: &'static str,
    pub topic: &'static str,
    pub messages: &'static [Message],
}

pub struct Message {
    pub author: &'static str,
    /// Hex for the author chip / name.
    pub color: &'static str,
    pub body: &'static str,
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
}

pub struct Member {
    pub name: &'static str,
    /// Hex tint for the avatar chip.
    pub color: &'static str,
    pub status: Status,
}

pub static SERVERS: &[Server] = &[
    Server {
        id: 1,
        name: "Rust & Beers",
        initial: "R",
        accent: "#b45a2c",
        channels: &[
            Channel {
                id: 101,
                name: "general",
                topic: "Lounge — talk about anything, Rust strongly encouraged",
                messages: &[
                    Message {
                        author: "miri",
                        color: "#5865f2",
                        body: "morning all ☕ anyone up for a borrow-checker war story?",
                    },
                    Message {
                        author: "dyno",
                        color: "#57f287",
                        body: "just spent 20 minutes fighting a lifetime that was actually a missing Clone",
                    },
                    Message {
                        author: "miri",
                        color: "#5865f2",
                        body: "the classic. the compiler was being generous",
                    },
                ],
            },
            Channel {
                id: 102,
                name: "showcase",
                topic: "Ship something and tell us about it",
                messages: &[
                    Message {
                        author: "dyno",
                        color: "#57f287",
                        body: "wrote a tiny WASM build tool over the weekend — 400 lines, zero deps",
                    },
                    Message {
                        author: "nara",
                        color: "#eb459e",
                        body: "link? I need that energy",
                    },
                ],
            },
            Channel {
                id: 103,
                name: "random",
                topic: "Everything else",
                messages: &[Message {
                    author: "nara",
                    color: "#eb459e",
                    body: "hot take: semicolons are load-bearing and we should stop pretending otherwise",
                }],
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
        channels: &[
            Channel {
                id: 201,
                name: "work-in-progress",
                topic: "Post renders, get feedback",
                messages: &[
                    Message {
                        author: "kai",
                        color: "#fee75c",
                        body: "first pass on the title screen, still a mess but the lighting feels right",
                    },
                    Message {
                        author: "sol",
                        color: "#ed4245",
                        body: "that glow on the emblem is doing a lot — love it",
                    },
                ],
            },
            Channel {
                id: 202,
                name: "assets",
                topic: "Share textures, fonts, and sounds",
                messages: &[Message {
                    author: "sol",
                    color: "#ed4245",
                    body: "dropped a free pixel-font pack into the drive, grab it while it's there",
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
        channels: &[
            Channel {
                id: 301,
                name: "trips",
                topic: "Plan the next outing",
                messages: &[
                    Message {
                        author: "rio",
                        color: "#3ba55c",
                        body: "weekend hike is a go — 14 people confirmed, meeting at the north lot 7am",
                    },
                    Message {
                        author: "rio",
                        color: "#3ba55c",
                        body: "bring layers, the ridge is windy",
                    },
                ],
            },
        ],
        members: &[
            Member { name: "rio", color: "#3ba55c", status: Status::Online },
            Member { name: "sam", color: "#5865f2", status: Status::Idle },
            Member { name: "lee", color: "#f0b232", status: Status::Offline },
        ],
    },
];
