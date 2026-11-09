# idea-chat bridge inventory

Repo: `/root/idea-chat` @ commit `6437901` (`chore: lots of protocol decisions and WIP interface`) — full sha `643790136ce86eeb8e38b12cf3909e22af1154d7`.

Generated for a Rust client bridge. All `#[server]` fns live in `crates/chat-api/src` (71 total). Every signed-in fn takes a trailing `user: Auth<Principal>` extractor; on the wire that arg is supplied by the session (bearer token / cookie), **not** in the JSON body array.

## 1. Every `#[server]` fn (name — args in declaration order — return)

Wire protocol: `POST /_srv/<fn-name>`, body = JSON array of the non-extractor args **in declaration order**. `Answer<T> = Result<T, Failure>` where `Failure = ServerError<ApiError>`; the body args are what the bridge must send.

### auth.rs (17)
| fn | args (order) | return |
|---|---|---|
| `sign_up` | `email: String, password: String, handle: String, display_name: String` | `Answer<SignedUp>` |
| `verify_email` | `email: String, code: String` | `Answer<Session>` |
| `resend_code` | `email: String` | `Answer<()>` |
| `sign_in` | `email: String, password: String` | `Answer<Session>` |
| `forgot_password` | `email: String` | `Answer<()>` |
| `reset_password` | `email: String, code: String, password: String` | `Answer<Session>` |
| `whoami` | `user: Auth<Principal>` | `Answer<Me>` |
| `sign_out` | `user: Auth<Principal>` | `Answer<()>` |
| `sign_out_everywhere` | `user: Auth<Principal>` | `Answer<()>` |
| `change_password` | `current: String, password: String, user: Auth<Principal>` | `Answer<Session>` |
| `handle_available` | `handle: String` | `Answer<bool>` |
| `update_profile` | `display_name: Option<String>, bio: Option<String>, user: Auth<Principal>` | `Answer<User>` |
| `set_handle` | `handle: String, user: Auth<Principal>` | `Answer<User>` |
| `set_status` | `status: Status, user: Auth<Principal>` | `Answer<Me>` |
| `register_device` | `name: String, public_key: String, user: Auth<Principal>` | `Answer<Device>` |
| `remove_device` | `device: Id, user: Auth<Principal>` | `Answer<()>` |
| `live_ticket` | `device: Option<Id>, user: Auth<Principal>` | `Answer<String>` |

### spaces.rs (17)
| fn | args (order) | return |
|---|---|---|
| `create_space` | `name: String, user: Auth<Principal>` | `Answer<Id>` |
| `rename_space` | `space: Id, name: String, user: Auth<Principal>` | `Answer<()>` |
| `delete_space` | `space: Id, user: Auth<Principal>` | `Answer<()>` |
| `leave_space` | `space: Id, user: Auth<Principal>` | `Answer<()>` |
| `remove_member` | `space: Id, member: Id, user: Auth<Principal>` | `Answer<()>` |
| `set_role` | `space: Id, member: Id, role: crate::Rank, user: Auth<Principal>` | `Answer<()>` |
| `transfer_space` | `space: Id, to: Id, user: Auth<Principal>` | `Answer<()>` |
| `create_invite` | `space: Id, max_uses: Option<u32>, hours: Option<u32>, user: Auth<Principal>` | `Answer<Invite>` |
| `invites` | `space: Id, user: Auth<Principal>` | `Answer<Vec<Invite>>` |
| `revoke_invite` | `space: Id, code: String, user: Auth<Principal>` | `Answer<()>` |
| `invite_preview` | `code: String, user: Auth<Principal>` | `Answer<InvitePreview>` |
| `join_space` | `code: String, user: Auth<Principal>` | `Answer<Id>` |
| `create_category` | `space: Id, name: String, user: Auth<Principal>` | `Answer<Category>` |
| `rename_category` | `space: Id, category: Id, name: String, user: Auth<Principal>` | `Answer<()>` |
| `delete_category` | `space: Id, category: Id, user: Auth<Principal>` | `Answer<()>` |
| `set_space_order` | `spaces: Vec<Id>, user: Auth<Principal>` | `Answer<()>` |
| `set_muted` | `target: Id, muted: bool, user: Auth<Principal>` | `Answer<()>` |

`crate::Rank` = `chat_model::Role` aliased so `#[server]` does not treat a param named `Role` as the server-kit role extractor (lib.rs:46):
```rust
pub type Rank = chat_model::Role;
```

### channels.rs (12)
| fn | args (order) | return |
|---|---|---|
| `create_channel` | `space: Id, kind: ChannelKind, name: String, category: Option<Id>, user: Auth<Principal>` | `Answer<Channel>` |
| `update_channel` | `channel: Id, name: String, topic: String, category: Option<Id>, view: crate::Rank, post: crate::Rank, user: Auth<Principal>` | `Answer<Channel>` |
| `delete_channel` | `channel: Id, user: Auth<Principal>` | `Answer<()>` |
| `order_channels` | `space: Id, places: Vec<(Id, Option<Id>, i32)>, user: Auth<Principal>` | `Answer<()>` |
| `create_thread` | `parent: Id, name: String, starter: Option<Id>, user: Auth<Principal>` | `Answer<Channel>` |
| `set_archived` | `thread: Id, archived: bool, user: Auth<Principal>` | `Answer<Channel>` |
| `rename_channel` | `channel: Id, name: String, user: Auth<Principal>` | `Answer<Channel>` |
| `open_direct` | `with: Id, user: Auth<Principal>` | `Answer<Channel>` |
| `create_group` | `with: Vec<Id>, name: String, user: Auth<Principal>` | `Answer<Channel>` |
| `add_to_group` | `group: Id, add: Id, user: Auth<Principal>` | `Answer<Channel>` |
| `leave_group` | `group: Id, user: Auth<Principal>` | `Answer<()>` |
| `close_conversation` | `channel: Id, user: Auth<Principal>` | `Answer<()>` |

`create_channel` full source (multi-line, order confirmed):
```rust
#[server]
pub async fn create_channel(
    space: Id,
    kind: ChannelKind,
    name: String,
    category: Option<Id>,
    user: Auth<Principal>,
) -> Answer<Channel> {
```
`update_channel` full source:
```rust
#[server]
pub async fn update_channel(
    channel: Id,
    name: String,
    topic: String,
    category: Option<Id>,
    view: crate::Rank,
    post: crate::Rank,
    user: Auth<Principal>,
) -> Answer<Channel> {
```

### files.rs (4)
| fn | args (order) | return |
|---|---|---|
| `begin_upload` | `name: String, size: u64, content_type: String, purpose: Purpose, user: Auth<Principal>` | `Answer<Slot>` |
| `finish_upload` | `id: Id, width: Option<u32>, height: Option<u32>, user: Auth<Principal>` | `Answer<Attachment>` |
| `set_avatar` | `upload: Option<Id>, user: Auth<Principal>` | `Answer<User>` |
| `set_space_icon` | `space: Id, upload: Option<Id>, user: Auth<Principal>` | `Answer<()>` |

`Purpose` (files.rs):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose { Message, Avatar, Icon }
```

### messages.rs (8)
| fn | args (order) | return |
|---|---|---|
| `history` | `channel: Id, window: Window, user: Auth<Principal>` | `Answer<Page>` |
| `message` | `id: Id, user: Auth<Principal>` | `Answer<Message>` |
| `send` | `channel: Id, body: String, reply_to: Option<Id>, attachments: Vec<Id>, user: Auth<Principal>` | `Answer<Message>` |
| `edit` | `id: Id, body: String, user: Auth<Principal>` | `Answer<Message>` |
| `delete` | `id: Id, user: Auth<Principal>` | `Answer<()>` |
| `react` | `id: Id, emoji: String, add: bool, user: Auth<Principal>` | `Answer<()>` |
| `mark_read` | `channel: Id, last_read: Id, user: Auth<Principal>` | `Answer<()>` |
| `search` | `scope: Scope, query: String, user: Auth<Principal>` | `Answer<Vec<Message>>` |

`Window` / `Scope` tags:
```rust
#[serde(tag = "at", rename_all = "snake_case")]
pub enum Window { Latest, Before { id: Id }, After { id: Id }, Around { id: Id } }

#[serde(tag = "in", rename_all = "snake_case")]
pub enum Scope { Space { id: Id }, Channel { id: Id }, Conversations }
```
`send` full source signature:
```rust
#[server(tags(limit = "120/min"))]
pub async fn send(
    channel: Id,
    body: String,
    reply_to: Option<Id>,
    attachments: Vec<Id>,
    user: Auth<Principal>,
) -> Answer<Message> {
```

### people.rs (5)
| fn | args (order) | return |
|---|---|---|
| `find_user` | `handle: String, user: Auth<Principal>` | `Answer<Option<User>>` |
| `add_friend` | `handle: String, user: Auth<Principal>` | `Answer<Relation>` |
| `accept_friend` | `other: Id, user: Auth<Principal>` | `Answer<()>` |
| `remove_relation` | `other: Id, user: Auth<Principal>` | `Answer<()>` |
| `block` | `other: Id, user: Auth<Principal>` | `Answer<()>` |

### calls.rs (7)
| fn | args (order) | return |
|---|---|---|
| `join_call` | `channel: Id, device: Id, user: Auth<Principal>` | `Answer<CallTicket>` |
| `leave_call` | `user: Auth<Principal>` | `Answer<()>` |
| `update_call` | `muted: bool, deafened: bool, camera: bool, screen: bool, user: Auth<Principal>` | `Answer<()>` |
| `decline_call` | `channel: Id, user: Auth<Principal>` | `Answer<()>` |
| `renew_grant` | `user: Auth<Principal>` | `Answer<String>` |
| `request_key` | `room: Id, user: Auth<Principal>` | `Answer<()>` |
| `deliver_key` | `key: SealedKey, user: Auth<Principal>` | `Answer<()>` |

### live.rs (1) — `#[server::channel]`, the WebSocket, not a POST
```rust
#[server::channel]
pub async fn live(mut ch: server::Socket<Up, Down>, ticket: String)
    -> Result<(), ServerError<ApiError>> {
```
`Up` / `Down` (live.rs):
```rust
pub type Down = idea_state::Down<State, Event>;   // server -> client
pub type Up   = idea_state::Up<Action>;           // client -> server
```

**fn_count total = 71** (17 + 16 + 13 + 4 + 8 + 5 + 7 + 1).

## 2. `crates/chat-model/src` — Id / Millis / State / Event / Action

`Id` and `Millis` (lib.rs:35 / 38):
```rust
/// Every entity's id: a UUIDv7 the server assigned.
pub type Id = uuid::Uuid;

/// Milliseconds since the Unix epoch, UTC.
pub type Millis = i64;
```

`State` (live.rs:29) — full struct:
```rust
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct State {
    pub me: Option<Me>,
    pub users: BTreeMap<Id, User>,
    pub presence: BTreeMap<Id, Status>,
    pub spaces: BTreeMap<Id, Space>,
    pub space_order: Vec<Id>,
    pub channels: BTreeMap<Id, Channel>,
    pub reads: BTreeMap<Id, ReadState>,
    pub relations: BTreeMap<Id, Relation>,
    pub voice: BTreeMap<Id, BTreeMap<Id, VoiceState>>,
    pub rings: BTreeMap<Id, Ring>,
    pub muted: BTreeSet<Id>,
    pub devices: Vec<Device>,
}
```

`Event` (live.rs:60) — serde attrs `#[serde(tag = "e", rename_all = "snake_case")]`:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "e", rename_all = "snake_case")]
pub enum Event {
    UserUpdated { user: User },
    MeUpdated { me: Me },
    PresenceChanged { user: Id, status: Status },
    SpaceUpserted { space: Space, channels: Vec<Channel>, users: Vec<User> },
    SpaceRenamed { space: Id, name: String, icon: Option<String> },
    SpaceRemoved { space: Id },
    MemberJoined { space: Id, user: User, member: Member },
    MemberRole { space: Id, user: Id, role: Role },
    MemberLeft { space: Id, user: Id },
    CategoriesChanged { space: Id, categories: Vec<Category> },
    SpaceOrder { spaces: Vec<Id> },
    ChannelUpserted { channel: Channel },
    ChannelRemoved { channel: Id },
    MessageCreated { message: Message },
    MessageEdited { message: Message },
    MessageDeleted { channel: Id, message: Id },
    ReactionChanged { channel: Id, message: Id, emoji: String, user: Id, added: bool },
    Read { channel: Id, last_read: Id },
    Typing { channel: Id, user: Id },
    RelationChanged { user: User, relation: Option<Relation> },
    VoiceJoined { state: VoiceState },
    VoiceUpdated { state: VoiceState },
    VoiceLeft { channel: Id, user: Id },
    RingUpdated { ring: Ring },
    RingEnded { channel: Id },
    KeyRequested { request: KeyRequest },
    KeyDelivered { key: SealedKey },
    MutedChanged { target: Id, muted: bool },
    DevicesChanged { devices: Vec<Device> },
    Resync,
}
```

`Action` (live.rs:124) — serde attrs `#[serde(tag = "a", rename_all = "snake_case")]`:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "a", rename_all = "snake_case")]
pub enum Action {
    Typing { channel: Id },
    Idle { idle: bool },
}
```

## 3. `crates/idea-state/src/lib.rs` — Up / Down and serde tags

Both tagged with `#[serde(tag = "t", rename_all = "snake_case")]`:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Up<A> {
    Action(A),
    Ping,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Down<S, E> {
    Snapshot { seq: u64, state: S },
    Event { seq: u64, event: E },
    Refused { reason: String },
    Pong,
}
```
Note: `Action(A)` is a *positional* variant — serializes as `{"t":"action","0":<A>}` for the `chat_model::live::Action` enum (which itself re-tags with `{"a":...}`). `Up::Ping` → `{"t":"ping"}`; `Down::Pong` → `{"t":"pong"}`. `Cursor`/`Step` are client-side ordering helpers (not wire frames).

## 4. chat-api shared shapes (srv.rs / auth.rs / files.rs / live.rs / spaces.rs)

`Answer<>` alias and `Failure` (lib.rs:111 / 114):
```rust
pub type Failure = ServerError<ApiError>;
pub type Answer<T> = Result<T, Failure>;
```

`Session` (auth.rs:25):
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// The bearer token, for native clients to keep in the OS keystore. Web
    /// clients get an httpOnly cookie instead and this is `None`.
    pub token: Option<String>,
    pub me: Me,
}
```

`Principal` (lib.rs:53) — what a session stands for:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub user: Id,
    pub epoch: i32,
}
```

`Slot` (files.rs:32):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub id: Id,
    pub put_url: String,
    pub content_type: String,
}
```

`Attachment` — in chat-model/src/domain.rs:226 (re-exported via `chat_model`):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Id,
    pub name: String,
    pub size: u64,
    pub content_type: String,
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}
```

**`live_ticket` return shape**: `Answer<String>` — the bare ticket string. Format from `srv.rs::mint_ticket` (line 371): `t1.<user>.<device|->.<expires ms>.<mac>` where mac is hex HMAC-SHA256 of the body over `app().live_secret`, expires = `now_ms + 60_000`. Client dials the WS with this ticket in the URL.

`Page` (timeline.rs:20 — what `history` returns):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Page {
    pub messages: Vec<Message>,
    pub older: bool,
    pub present: bool,
}
```

### Requested signatures — exact (arg order + Option flags)
```rust
create_space(name: String, user) -> Answer<Id>
rename_space(space: Id, name: String, user) -> Answer<()>
delete_space(space: Id, user) -> Answer<()>              // owner only
create_channel(space: Id, kind: ChannelKind, name: String, category: Option<Id>, user) -> Answer<Channel>
rename_channel(channel: Id, name: String, user) -> Answer<Channel>
delete_channel(channel: Id, user) -> Answer<()>
set_space_icon(space: Id, upload: Option<Id>, user) -> Answer<()>
transfer_space(space: Id, to: Id, user) -> Answer<()>    // owner only
invite_preview(code: String, user) -> Answer<InvitePreview>
join_space(code: String, user) -> Answer<Id>
leave_space(space: Id, user) -> Answer<()>
remove_member(space: Id, member: Id, user) -> Answer<()>
```
`user` in each = `Auth<Principal>` (session extractor, not in the body array).

## Option args summary (which fns carry Option args, in order)
- `update_profile` — `display_name: Option<String>, bio: Option<String>`
- `live_ticket` — `device: Option<Id>`
- `create_invite` — `max_uses: Option<u32>, hours: Option<u32>` (both `None` = unlimited)
- `create_channel` — `category: Option<Id>`
- `update_channel` — `category: Option<Id>` (also non-Option `view/post: Rank`)
- `order_channels` — `places: Vec<(Id, Option<Id>, i32)>` (tuple, middle is Option)
- `create_thread` — `starter: Option<Id>`
- `finish_upload` — `width: Option<u32>, height: Option<u32>`
- `set_avatar` — `upload: Option<Id>`
- `set_space_icon` — `upload: Option<Id>`
- `send` — `reply_to: Option<Id>`
- `find_user` — return is `Option<User>`

## 5. UUID v7 + Id serialization

- No `crates/chat-model/src/uuid` module exists; uuid config is in `crates/chat-model/Cargo.toml`:
  - line 15: `uuid = { version = "1", features = ["serde", "v7"] }`
- Ids are **minted** by the server in chat-db (`crates/chat-db/src/lib.rs:88`: `uuid::Uuid::now_v7()`) → UUIDv7 confirmed, sorting by creation time.
- **`Id` serialization**: `pub type Id = uuid::Uuid` with `uuid`'s `serde` feature → serializes to a JSON **string** (canonical 36-char UUID form, e.g. `"018f...-..."`). `chat-db/src/lib.rs:498` uses `uuid::Uuid::new_v4().simple()` only for opaque object-storage keys, not entity ids.

## Note: other `#[server]` declarations
`grep '^[[:space:]]*#\[server' crates` also matches `crates/hearth/src/api/**` (24 fns: `instance_status, claim_instance, peek_invite, register, login, logout, me, my_spaces, create_invite, create_space, channel_page, channel_since, thread, send_message, edit_message, delete_message, voice_ticket, voice_remove, mark_read, unread, open_dm, bootstrap, space_members, join_space`). That is a **separate app's API** with a different vocabulary (string ids, `server::State<...>` / `server_kit::Auth<Principal>` extractors, `Result<T, ServerError>` returns — not `Answer<>`). The chat-api bridge above is the relevant one for this inventory.
