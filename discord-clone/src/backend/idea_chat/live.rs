//! The second backend's live connection: the `GatewayHandle` the UI holds and
//! the reducer that folds `idea_state` Down/Up frames. The Cassette socket
//! (`server::use_socket`) delivers `WsServerMsg` frames directly; idea-chat's
//! wire carries a reducer state plus a per-connection event sequence, so this
//! mirrors `use_socket`'s own loop and translates each applied event into the
//! `WsServerMsg` frames the UI's frame watcher already listens to.
//!
//! The reducer state and cursor are wire bookkeeping (process-wide holders,
//! like the socket's own internals), not reactive app state; the UI's reactive
//! surface stays the signals the handle carries.

use server::{ServerError, Socket, SocketStatus, SocketSender};
use runtime_core::{Signal, signal, driver, ScopeAlive};
use serde_json::{Value, to_string};

use discord_server::server_settings::{WsServerMsg, AttachmentInput, MessageAttachment, next_frame_seq};

use chat_model::{Id, User, Channel, Me, Message, Presence, Relation, Role, Millis, Status};
use chat_model::live::{State, Event, Action};
use chat_model::timeline::{Page};
use idea_state::{Cursor, Step};
use crate::backend::idea_chat::model;
use crate::backend::idea_chat::model::{Down, Up, Window};
use crate::backend::idea_chat::idmap;
use crate::backend::idea_chat::client;

// --- the reducer's world ---------------------------------------------------

static STATE: std::sync::OnceLock<std::sync::Mutex<State>> = std::sync::OnceLock::new();
static CURSOR: std::sync::OnceLock<std::sync::Mutex<Cursor>> = std::sync::OnceLock::new();

fn state() -> &'static std::sync::Mutex<State> {
    STATE.get_or_init(Default::default)
}

fn cursor() -> &'static std::sync::Mutex<Cursor> {
    CURSOR.get_or_init(Default::default)
}

/// The reducer's current visible world (empty until the first snapshot).
pub(crate) fn snapshot() -> State {
    let held = state().lock().unwrap_or_else(move |p| p.into_inner());
    held.clone()
}

/// Fold one event per the per-connection sequence. Apply advances and returns
/// true; a Duplicate returns false; a Gap drops the state (untrustworthy) and
/// returns false, so the client waits for the next snapshot.
pub(crate) fn fold(mut event: Event, seq: u64) -> bool {
    let mut cur = cursor().lock().unwrap_or_else(move |p| p.into_inner());
    match cur.step(seq) {
        Step::Apply => {
            let mut world = state().lock().unwrap_or_else(move |p| p.into_inner());
            world.apply(&event);
            true
        },
        Step::Duplicate => false,
        Step::Gap => {
            drop_state();
            false
        },
    }
}

/// Replace the state wholesale and set the cursor to the snapshot's sequence.
pub(crate) fn take_snapshot(mut world: State, seq: u64) {
    *state().lock().unwrap_or_else(move |p| p.into_inner()) = world;
    *cursor().lock().unwrap_or_else(move |p| p.into_inner()) = Cursor::at(seq);
}

pub(crate) fn drop_state() {
    *state().lock().unwrap_or_else(move |p| p.into_inner()) = State::default();
    *cursor().lock().unwrap_or_else(move |p| p.into_inner()) = Cursor::at(0);
}

// --- id table + vocabulary words -----------------------------------------

/// The signed-in account's handle: the one key its id table uses.
fn account() -> String {
    crate::auth::current_username()
}

/// The i64 the table gives for a wire UUID (assigning it if unseen).
pub(crate) fn see(uuid: Id) -> i64 {
    idmap::see(account(), uuid_to_string(uuid))
}

/// The wire UUID behind an i64 the UI holds.
pub(crate) fn wire_of(id: i64) -> Id {
    let raw = idmap::uuid_of(account(), id).unwrap_or_default();
    match Id::from_str(raw) {
        Some(parsed) => parsed,
        None => Id::nil(),
    }
}

fn uuid_to_string(id: Id) -> String {
    id.to_string()
}

pub(crate) fn presence_word(status: Presence) -> String {
    match status {
        Presence::Online => "online",
        Presence::Idle => "idle",
        Presence::Busy => "dnd",
        Presence::Offline => "offline",
    }
}

/// chat-model has three ranks; RWND's "moderator" is chat-model's `Admin`.
pub(crate) fn role_word(role: Role) -> String {
    match role {
        Role::Owner => "owner",
        Role::Admin => "moderator",
        Role::Member => "member",
    }
}

/// The relation word `FriendChanged.action` and the profile's `relationship`
/// speak. RWND names the four states exactly as chat-model orders them.
pub(crate) fn relation_word(relation: Relation) -> String {
    match relation {
        Relation::Friend => "friends",
        Relation::Incoming => "incoming",
        Relation::Outgoing => "outgoing",
        Relation::Blocked => "blocked",
    }
}

pub(crate) fn millis_at(at: Millis) -> i64 {
    at
}

// --- events → the UI's frame vocabulary ----------------------------------

/// Translate one applied live event into the frames the UI listens to.
pub(crate) fn frames_for(mut event: Event) -> Vec<WsServerMsg> {
    let mut out: Vec<WsServerMsg> = Vec::new();
    match event {
        Event::UserUpdated { user } => out.push(profile_frame(user.clone())),
        Event::MeUpdated { me } => out.push(status_choice(me.clone())),
        Event::PresenceChanged { user, status } => out.push(presence(user, status.presence)),
        Event::SpaceUpserted { space, channels, users } => {
            out.push(settings_changed(space.id));
            for channel in channels.iter() {
                out.push(channel_arrived(channel.clone()));
            }
            for user in users.iter() {
                out.push(profile_frame(user.clone()));
            }
        },
        Event::SpaceRenamed { space } => out.push(settings_changed(space)),
        Event::SpaceRemoved { space } => out.push(settings_changed(space)),
        Event::MemberJoined { space, user } => out.push(member_changed(space, user, true)),
        Event::MemberRole { space } => out.push(roles_changed(space)),
        Event::MemberLeft { space, user } => out.push(member_changed(space, user, false)),
        Event::CategoriesChanged { space } => out.push(settings_changed(space)),
        Event::SpaceOrder { spaces } => out.push(settings_changed(space_order_first(spaces))),
        Event::ChannelUpserted { channel } => out.push(channel_arrived(channel.clone())),
        Event::MessageCreated { message } => out.push(message_row(message.clone(), true)),
        Event::MessageEdited { message } => out.push(edited(message.clone())),
        Event::MessageDeleted { channel, message } => out.push(deleted(channel, message)),
        Event::ReactionChanged { channel, message, emoji, user, added } => out.push(
            reaction(added, channel, message, user, emoji),
        ),
        Event::Typing { channel, user } => out.push(typing(channel, user)),
        Event::RelationChanged { user, relation } => out.push(friend(user.id, relation)),
        // Read, voice, rings, call keys, mutes and devices have no RWND screen
        // behind this backend yet; the state still folds them, the frames stay
        // out. A server-internal restate: this client waits for a fresh
        // snapshot.
        Event::Read { channel, last_read } => {},
        Event::VoiceJoined { state } => {},
        Event::VoiceUpdated { state } => {},
        Event::VoiceLeft { channel, user } => {},
        Event::RingUpdated { ring } => {},
        Event::RingEnded { channel } => {},
        Event::KeyRequested { request } => {},
        Event::KeyDelivered { key } => {},
        Event::MutedChanged { target, muted } => {},
        Event::DevicesChanged { devices } => {},
        Event::Resync => drop_state(),
    }
    out
}

fn space_order_first(mut spaces: Vec<Id>) -> Id {
    match spaces.first() {
        Some(id) => id,
        None => Id::nil(),
    }
}

pub(crate) fn profile_frame(user: User) -> WsServerMsg {
    WsServerMsg::ProfileChanged {
        user_id: see(user.id),
        username: user.handle,
        display_name: display_of(user),
        avatar_decoration: None,
        seq: next_frame_seq(),
    }
}

pub(crate) fn status_choice(me: Me) -> WsServerMsg {
    WsServerMsg::StatusChoice { status: presence_word(me.status.presence), seq: next_frame_seq() }
}

pub(crate) fn presence(user: Id, status: Presence) -> WsServerMsg {
    WsServerMsg::Presence { user_id: see(user), status: presence_word(status), seq: next_frame_seq() }
}

pub(crate) fn settings_changed(space: Id) -> WsServerMsg {
    WsServerMsg::ServerSettingsChanged { server_id: see(space), seq: next_frame_seq() }
}

pub(crate) fn roles_changed(space: Id) -> WsServerMsg {
    WsServerMsg::RolesChanged { server_id: see(space), seq: next_frame_seq() }
}

pub(crate) fn channel_arrived(channel: Channel) -> WsServerMsg {
    match channel.space {
        None => WsServerMsg::DmChanged { dm_id: see(channel.id), seq: next_frame_seq() },
        Some(space) => WsServerMsg::ServerSettingsChanged { server_id: see(space), seq: next_frame_seq() },
    }
}

pub(crate) fn member_changed(space: Id, user: Id, joined: bool) -> WsServerMsg {
    WsServerMsg::MemberChanged { server_id: see(space), user_id: see(user), joined, seq: next_frame_seq() }
}

pub(crate) fn typing(channel: Id, user: Id) -> WsServerMsg {
    WsServerMsg::Typing { channel_id: see(channel), user_id: see(user), seq: next_frame_seq() }
}

pub(crate) fn friend(user: Id, relation: Option<Relation>) -> WsServerMsg {
    let action = match relation {
        Some(other) => relation_word(other),
        None => "removed",
    };
    WsServerMsg::FriendChanged { friend_id: see(user), action, seq: next_frame_seq() }
}

pub(crate) fn deleted(channel: Id, message: Id) -> WsServerMsg {
    WsServerMsg::MessageDeleted { message_id: see(message), channel_id: see(channel), seq: next_frame_seq() }
}

pub(crate) fn error_frame(reason: String) -> WsServerMsg {
    WsServerMsg::Error { reason, seq: next_frame_seq() }
}

pub(crate) fn display_of(user: User) -> Option<String> {
    if user.display_name == user.handle {
        None
    } else {
        Some(user.display_name)
    }
}

/// An edited-message frame: replace the stored content for the id.
pub(crate) fn edited(message: Message) -> WsServerMsg {
    WsServerMsg::MessageEdited {
        message_id: see(message.id),
        channel_id: see(message.channel),
        content: message.body,
        edited_at: millis_at(message.edited_at.unwrap_or(message.at)),
        mentioned_user_ids: message.mentions.iter().map(move |u| see(u)).collect(),
        seq: next_frame_seq(),
    }
}

pub(crate) fn reaction(added: bool, channel: Id, message: Id, user: Id, emoji: String) -> WsServerMsg {
    let seq = next_frame_seq();
    if added {
        WsServerMsg::ReactionAdd { message_id: see(message), channel_id: see(channel), user_id: see(user), emoji, count: 0, seq }
    } else {
        WsServerMsg::ReactionRemove { message_id: see(message), channel_id: see(channel), user_id: see(user), emoji, count: 0, seq }
    }
}

/// A full message frame from a live `MessageCreated` event or a history page
/// (`new: false` for history). Author name and reactions cross through the
/// reducer state and the id table.
pub(crate) fn message_row(message: Message, new: bool) -> WsServerMsg {
    let world = snapshot();
    let viewer = match world.me {
        Some(me) => me.user.id,
        None => message.author,
    };
    let author_username = match world.users.get(&message.author) {
        Some(user) => user.handle,
        None => uuid_to_string(message.author),
    };
    let mut reactions: Vec<(String, u32, bool)> = Vec::new();
    for entry in message.reactions.iter() {
        reactions.push((entry.emoji, entry.users.len() as u32, entry.users.iter().any(move |u| u == viewer)));
    }
    let mut attachments: Vec<MessageAttachment> = Vec::new();
    for file in message.attachments.iter() {
        attachments.push(MessageAttachment {
            id: see(file.id),
            filename: file.name,
            mime: file.content_type,
            url: file.url,
            width: file.width,
            height: file.height,
        });
    }
    WsServerMsg::Message {
        message_id: see(message.id),
        channel_id: see(message.channel),
        server_id: match world.channels.get(&message.channel).and_then(move |c| c.space) {
            Some(space) => see(space),
            None => 0,
        },
        author_id: see(message.author),
        author_username: author_username.to_string(),
        author_avatar: None,
        author_decoration: None,
        content: message.body,
        created_at: millis_at(message.at),
        edited_at: match message.edited_at {
            Some(at) => Some(millis_at(at)),
            None => None,
        },
        new,
        mentioned_user_ids: message.mentions.iter().map(move |u| see(u)).collect(),
        reply_to_message_id: match message.reply_to {
            Some(reply) => Some(see(reply.id)),
            None => None,
        },
        reactions,
        attachments,
        embeds: Vec::new(),
        sticker: None,
        seq: next_frame_seq(),
    }
}

// --- the handle the UI holds ---------------------------------------------

#[derive(Clone, Copy)]
pub struct GatewayHandle {
    incoming: Signal<Option<WsServerMsg>>,
    status: Signal<SocketStatus>,
    sender: Signal<Option<SocketSender<Up>>>,
}

impl GatewayHandle {
    pub fn incoming(self: GatewayHandle) -> Signal<Option<WsServerMsg>> {
        self.incoming
    }

    pub fn connection_status(self: GatewayHandle) -> SocketStatus {
        self.status.get()
    }

    /// The only live-socket Action: "I am typing here" — ephemeral, shown
    /// transiently by the client.
    pub fn announce_typing(self: GatewayHandle, channel_id: i64) {
        match self.sender.get() {
            Some(tx) => {
                let _ = tx.send(Up::Action(Action::Typing { channel: wire_of(channel_id) }));
            }
            None => {}
        }
    }

    /// send / edit / delete are RPCs (the wire study's §3.5): the calls fire
    /// detached and the facts they cause arrive back as live events, so
    /// nothing is injected here — only the socket's typing is an Action.
    pub fn send_message(self: GatewayHandle, channel_id: i64, content: String, reply_to_message_id: Option<i64>, attachments: Vec<AttachmentInput>, sticker_id: Option<i64>) {
        driver::spawn_detached(async move {
            let _ = client_send(channel_id, content, reply_to_message_id, attachments, sticker_id);
        });
    }

    pub fn edit_message(self: GatewayHandle, message_id: i64, content: String) {
        driver::spawn_detached(async move {
            let _ = client_edit(message_id, content);
        });
    }

    pub fn delete_message(self: GatewayHandle, message_id: i64) {
        driver::spawn_detached(async move {
            let _ = client_delete(message_id);
        });
    }

    pub fn set_status(self: GatewayHandle, status: String) {
        driver::spawn_detached(async move {
            let _ = client_set_status(status);
        });
    }

    /// History is a page RPC whose rows arrive as the same `Message { new:
    /// false }` frames the Cassette socket injects, so the UI's scroll path is
    /// shared. Anchored on the handle's scope so a closed screen drops it.
    pub fn request_history(self: GatewayHandle, channel_id: i64, before_id: Option<i64>) {
        let incoming = self.incoming;
        runtime_core::spawn_then_in(
            &ScopeAlive::current(),
            history(channel_id, before_id),
            move |page| {
                match page {
                    Ok(rows) => {
                        for message in rows.messages.iter() {
                            incoming.set_always(Some(message_row(message.clone(), false)));
                        }
                        incoming.set_always(Some(WsServerMsg::History {
                            channel_id,
                            has_more: rows.older,
                            seq: next_frame_seq(),
                        }));
                    }
                    Err(e) => incoming.set_always(Some(error_frame(reason(e)))),
                }
            },
        );
    }
}

// --- the RPC verbs the handle runs --------------------------------------

async fn client_send(channel_id: i64, content: String, reply_to_message_id: Option<i64>, attachments: Vec<AttachmentInput>, sticker_id: Option<i64>) -> Result<Message, ServerError> {
    let mut uploads: Vec<Id> = Vec::new();
    for input in attachments.iter() {
        match upload_data_url(input.filename, input.data, model::Purpose::Message) {
            Err(e) => return Err(e),
            Ok(id) => uploads.push(id),
        }
    }
    let mut args = Vec::new();
    args.push(client::arg(wire_of(channel_id)));
    args.push(client::arg(content));
    args.push(match reply_to_message_id {
        Some(id) => client::arg(wire_of(id)),
        None => Value::null,
    });
    args.push(client::arg(uploads));
    // Stickers are RWND-only: a sticker send has no idea-chat counterpart and
    // the server refuses an empty body, so a sticker-only send fails there.
    match sticker_id {
        Some(_) => Err(model::not_supported()),
        None => client::call_as::<Message>("send", args),
    }
}

async fn client_edit(message_id: i64, content: String) -> Result<Message, ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(wire_of(message_id)));
    args.push(client::arg(content));
    client::call_as::<Message>("edit", args)
}

async fn client_delete(message_id: i64) -> Result<(), ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(wire_of(message_id)));
    match client::post("delete", args) {
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}

async fn client_set_status(status: String) -> Result<Me, ServerError> {
    let presence = match status {
        "idle" => Presence::Idle,
        "away" => Presence::Idle,
        "dnd" => Presence::Busy,
        "busy" => Presence::Busy,
        "invisible" => Presence::Offline,
        _ => Presence::Online,
    };
    let mut args = Vec::new();
    args.push(client::arg(Status { presence, text: None }));
    client::call_as::<Me>("set_status", args)
}

pub(crate) async fn history(channel_id: i64, before_id: Option<i64>) -> Result<Page, ServerError> {
    let window = match before_id {
        Some(id) => Window::before(wire_of(id)),
        None => Window::latest(),
    };
    let mut args = Vec::new();
    args.push(client::arg(wire_of(channel_id)));
    args.push(client::arg(window));
    client::call_as::<Page>("history", args)
}

pub(crate) fn reason(e: ServerError) -> String {
    crate::errors::friendly(&e)
}

/// The upload triplet (`docs/idea-chat-wire.md` §3): `begin_upload` → PUT the
/// bytes → `finish_upload`. `data_url` is RWND's inline `data:<mime>;base64,`
/// attachment form; on this server the bytes go through the presigned slot
/// instead. Answers the upload id the `send` / `set_avatar` / `set_space_icon`
/// calls take.
pub(crate) async fn upload_data_url(mut name: String, data_url: String, purpose: model::Purpose) -> Result<Id, ServerError> {
    if !data_url.starts_with("data:") {
        return Err(ServerError::failed("only inline image data uploads on this server"));
    }
    let Some(sep) = data_url.find(";base64,") else {
        return Err(ServerError::failed("the upload data is malformed"));
    };
    let mime = data_url[5 .. sep];
    let raw = data_url[sep + 8 ..];
    let bytes = match crate::profile_settings::b64_decode(raw) {
        Ok(bytes) => bytes,
        Err(why) => return Err(ServerError::Codec(why)),
    };
    upload_bytes(name, bytes, mime.to_string(), purpose)
}

pub(crate) async fn upload_bytes(mut name: String, mut bytes: Vec<u8>, mut mime: String, purpose: model::Purpose) -> Result<Id, ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(name));
    args.push(client::arg(bytes.len() as u64));
    args.push(client::arg(mime));
    args.push(client::arg(purpose));
    let slot = match client::call_as::<model::Slot>("begin_upload", args) {
        Err(e) => return Err(e),
        Ok(slot) => slot,
    };
    match client::put(slot.put_url, slot.content_type, bytes) {
        Err(e) => return Err(e),
        Ok(_) => {},
    }
    let upload_id = slot.id;
    let mut finish = Vec::new();
    finish.push(client::arg(upload_id));
    finish.push(Value::null);
    finish.push(Value::null);
    match client::post("finish_upload", finish) {
        Err(e) => Err(e),
        Ok(_) => Ok(upload_id),
    }
}

// --- the live connection itself -----------------------------------------

/// Open the second backend's stream: `live_ticket` → `/_srv/_ws/live?args=<hex
/// of [ticket]>`, then keep the reducer state and translate every applied
/// event into `WsServerMsg` deliveries — mirroring `server::use_socket`'s own
/// loop, since the payloads here are `idea_state` frames rather than the
/// `WsServerMsg` the Cassette socket carries. The session bearer/cookie
/// authenticates `live_ticket` itself; the ticket rides the URL because a
/// native WebSocket open carries no headers.
pub(crate) fn connect(mut token: String) -> GatewayHandle {
    let incoming: Signal<Option<WsServerMsg>> = signal(None);
    let status: Signal<SocketStatus> = signal(SocketStatus::Connecting);
    let sender: Signal<Option<SocketSender<Up>>> = signal(None);
    let alive = ScopeAlive::current();
    driver::spawn_detached(async move {
        let ticket = ticket().await;
        match ticket {
            Err(why) => {
                if alive.get() {
                    status.set(SocketStatus::Error);
                }
            }
            Ok(ticket) => {
                let mut ticket_args = Vec::new();
                ticket_args.push(client::arg(ticket));
                let url = client::origin() + "/_srv/_ws/live?args=" + client::args_hex(to_string(ticket_args).unwrap_or_default());
                match Socket::<Down, Up>::connect(&url).await {
                    Err(_) => {
                        if alive.get() {
                            status.set(SocketStatus::Error);
                        }
                    }
                    Ok(mut sock) => {
                        let tx = sock.sender();
                        if !alive.get() {
                            tx.close();
                            return;
                        }
                        sender.set_always(Some(tx));
                        status.set_always(SocketStatus::Open);
                        while let Some(res) = sock.recv().await {
                            if !alive.get() {
                                return;
                            }
                            match res {
                                Err(_) => {
                                    if alive.get() {
                                        status.set_always(SocketStatus::Error);
                                    }
                                    return;
                                }
                                Ok(frame) => {
                                    match frame {
                                        Down::Snapshot { seq, state } => take_snapshot(state.clone(), seq),
                                        Down::Event { seq, event } => {
                                            if fold(event.clone(), seq) {
                                                for frame in frames_for(event.clone()).iter() {
                                                    incoming.set_always(Some(frame));
                                                }
                                            }
                                        }
                                        Down::Refused { reason } => incoming.set_always(Some(error_frame(reason))),
                                        Down::Pong => {}
                                    }
                                }
                            }
                        }
                        if alive.get() {
                            status.set_always(SocketStatus::Closed);
                        }
                    }
                }
            }
        }
    });
    GatewayHandle { incoming, status, sender }
}

/// `POST /_srv/live_ticket` with `[null]` (the optional device id): a signed
/// one-connection ticket good for a minute, validated server-side before any
/// frame is exchanged.
async fn ticket() -> Result<String, ServerError> {
    client::call_as::<String>("live_ticket", Vec::new())
}
