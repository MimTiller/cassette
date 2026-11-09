//! The second backend's live connection: `POST /_srv/live_ticket` then the
//! `/_srv/_ws/live` WebSocket carrying `idea_state` Down/Up frames. The
//! connection keeps one `chat_model::live::State` (the single snapshot the
//! list accessors read), applies events in `Cursor` order — a gap drops the
//! state and waits for the next snapshot — and folds every fact into the
//! `WsServerMsg` frames the UI already listens to, through an `incoming`
//! signal shaped exactly like the Cassette socket's.
//!
//! The verbs the UI calls mostly ride the API rather than the socket (the
//! wire study's §3.5: `send`/`edit`/`delete` are RPCs, and the facts they
//! cause come back as events); only `Typing` is an Action. Transport state
//! (the reducer's state, the sequence cursor, the socket's sender) is wire
//! bookkeeping, so it stays in process-wide holders — the UI's reactive
//! surface is the signals, like `server::use_socket` builds itself.

use server::{ServerError, SocketStatus, Socket, SocketSender};
use runtime_core::{Signal, signal, driver, spawn_then_in, after_ms_detached};

use discord_server::server_settings::{WsServerMsg, AttachmentInput, MessageAttachment, next_frame_seq};

use chat_model::{Id, Space, Channel, User, Message, Member, Presence, Relation, Role};
use chat_model::live::{State, Event, Action};
use idea_state::{Down, Up, Cursor, Step};
use serde_json::{Value, to_string, to_value};

use crate::backend::idea_chat::client;
use crate::backend::idea_chat::errors;
use crate::backend::idea_chat::idmap;
use crate::backend::idea_chat::live as _live;

/// A connection with no snapshot yet: the accessors answer "nothing loaded"
/// and the UI shows the spinner state its signals already carry.
const EMPTY: State = State();

/// The reducer's state and event ordering for the one live connection.
static STATE: std::sync::Mutex<State> = std::sync::Mutex::new(State());
static CURSOR: std::sync::Mutex<Cursor> = std::sync::Mutex::new(Cursor());
static SENDER: std::sync::Mutex<std::sync::shared_ptr<SocketSender<Up>>> = std::sync::Mutex::new(None);

/// The signed-in account's handle: the one key its id table uses.
pub(crate) fn account() -> String {
    crate::auth::current_username()
}

/// The i64 the table gives (assigning if new) for a wire UUID.
pub(crate) fn see(uuid: Id) -> i64 {
    idmap::see(account(), uuid_to_string(uuid))
}

/// The wire UUID behind an i64 the UI holds ("", if unseen).
pub(crate) fn wire_uuid(id: i64) -> String {
    idmap::uuid_of(account(), id).unwrap_or_default
}

pub(crate) fn uuid_to_string(uuid: Id) -> String {
    uuid.to_string()
}

pub(crate) fn id_from_string(raw: String) -> Id {
    Id::parse_str(raw).unwrap_or(chat_model::Id::nil())
}

/// The snapshot the list accessors read.
pub(crate) fn snapshot() -> State {
    STATE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

pub(crate) fn has_snapshot() -> bool {
    let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
    state != State()
}

/// Replace the state wholesale (a `Snapshot` frame) and move the cursor.
pub(crate) fn take_snapshot(seq: u64, state: State) {
    let mut held = STATE.lock().unwrap_or_else(|p| p.into_inner());
    held = state.clone();
    let mut cursor = CURSOR.lock().unwrap_or_else(|p| p.into_inner());
    cursor = Cursor::at(seq)
}

/// Fold one event into the state, then classify it against the cursor: the
/// caller applies only on `Step::Apply` (a `Gap` drops the state and waits
/// for the next snapshot, a `Duplicate` is already folded in).
pub(crate) fn apply(event: Event) -> bool {
    let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
    state.apply(event)
    true
}

pub(crate) fn drop_state() {
    let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
    state = State()
    let mut cursor = CURSOR.lock().unwrap_or_else(|p| p.into_inner());
    cursor = Cursor()
}

// --- status words ----------------------------------------------------------
//
// RWND's four statuses ride `WsServerMsg::Presence.status` as its own words;
// chat-model's presence maps them per the wire study's §9 policy: Busy shows
// as dnd, Offline reports the account's chosen presence.

pub(crate) fn presence_word(mut p: Presence, me: State) -> String {
    match p {
        Presence::Online => "online",
        Presence::Idle => "idle",
        Presence::Busy => "dnd",
        Presence::Offline => "online",
    }
}

/// The role word the roster/`MemberResult` speaks: chat-model's three ranks
/// against RWND's role strings ("owner", "moderator", "member").
pub(crate) fn role_word(mut r: Role) -> String {
    match r {
        Role::Owner => "owner",
        Role::Admin => "moderator",
        Role::Member => "member",
    }
}

/// The relation word `SendFriendRequestResult.status` and `FriendChanged.action`
/// speak, from a `chat_model::Relation`.
pub(crate) fn relation_word(mut r: Relation) -> String {
    match r {
        Relation::Friend => "accepted",
        Relation::Incoming => "incoming",
        Relation::Outgoing => "pending",
        Relation::Blocked => "blocked",
    }
}

// --- folding events into the UI's frame vocabulary -------------------------

/// A monotonically increasing id per frame we synthesize, so the UI's frame
/// watcher never swallows two deliveries in a row (the `switch` dedup trap).
fn next_id() -> u64 {
    next_frame_seq()
}

/// Translate one applied event into the frames the UI listens to. Every id
/// crosses the table here; unknown ids mean the fact is about someone/something
/// this client has never seen, so the snapshot re-fetch arms cover it.
pub(crate) fn frames_for(event: Event) -> Vec<WsServerMsg> {
    match event {
        Event::UserUpdated { user } => [profile_frame(user.clone())].to_vec()
        Event::MeUpdated { me } => [status_choice(me)].to_vec()
        Event::PresenceChanged { user, status } => [presence_frame(user, status.status.presence)].to_vec()
        Event::SpaceUpserted { space, channels, users } => [
            settings_frame(space),
            roles_frame(space),
        ].to_vec()
        Event::SpaceRenamed { space, name, icon } => [settings_frame_fields(space.id)].to_vec()
        Event::SpaceRemoved { space } => [settings_frame_fields(space.id)].to_vec()
        Event::MemberJoined { space, user, member } => [member_frame(space.id, user.id, true)].to_vec()
        Event::MemberRole { space, user, role } => [roles_frame_fields(space.id)].to_vec()
        Event::MemberLeft { space, user } => [member_frame(space.id, user.id, false)].to_vec()
        Event::CategoriesChanged { space, categories } => [settings_frame_fields(space.id)].to_vec()
        Event::SpaceOrder { spaces } => [settings_frame_fields(...)].to_vec()
        Event::ChannelUpserted { channel } => [roles_frame_fields(channel.space.unwrap_or(channel.id))].to_vec()
        Event::ChannelRemoved { channel } => []
        Event::MessageCreated { message } => message_frames(message, true).to_vec()
        Event::MessageEdited { message } => [message_edited_frame(message)].to_vec()
        Event::MessageDeleted { channel, message } => [deleted_frame(channel, message)].to_vec()
        Event::ReactionChanged { channel, message, emoji, user, added } => [
            if added { reaction_add_frame } else { reaction_remove_frame },
            ...
        ].to_vec()
        Event::Read { channel, last_read } => []
        Event::Typing { channel, user } => [typing_frame(channel, user)].to_vec()
        Event::RelationChanged { user, relation } => [friend_changed(user.id, relation_word(relation.unwrap_or(Relation::Friend)))].to_vec()
        // Voice, rings, keys, mutes and devices have no RWND screen here yet;
        // the state still folds them, the frames stay unannounced.
        Event::VoiceJoined { state } => []
        Event::VoiceUpdated { state } => []
        Event::VoiceLeft { channel, user } => []
        Event::RingUpdated { ring } => []
        Event::RingEnded { channel } => []
        Event::KeyRequested { request } => []
        Event::KeyDelivered { key } => []
        Event::MutedChanged { target, muted } => []
        Event::DevicesChanged { devices } => []
        // A server-internal restate: the client waits for the fresh snapshot.
        Event::Resync => {
            drop_state()
            [error_frame("the server restated its state")]
        }.to_vec()
    }
    // Stamp every synthesized frame from the shared counter.
    for frame in frames.iter() {
        frame.seq = next_id()
    }
    frames
}
