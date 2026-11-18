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

use chat_model::{Id, User, Channel, Me, Message, Presence, Relation, Millis, Status};
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

/// Who has reacted with what, per message. The app applies a reaction frame's `count` as the emoji's TOTAL (and a total of 0
/// drops the pill), but idea-chat's `ReactionChanged` event names one user and carries no total. So the sets are seeded from
/// every message row that crosses (history and live rows carry the full lists) and moved by each event, and the frame says
/// how many people now hold the reaction.
static REACTORS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<(Id, String), std::collections::BTreeSet<Id>>>> = std::sync::OnceLock::new();

fn reactors() -> &'static std::sync::Mutex<std::collections::HashMap<(Id, String), std::collections::BTreeSet<Id>>> {
    REACTORS.get_or_init(Default::default)
}

/// The reducer's current visible world (empty until the first snapshot).
pub(crate) fn snapshot() -> State {
    let held = state().lock().unwrap_or_else(move |p| p.into_inner());
    held.clone()
}

/// Fold one event per the per-connection sequence. Apply advances and returns
/// true; a Duplicate returns false; a Gap drops the state (untrustworthy) and
/// returns false, so the client waits for the next snapshot.
pub(crate) fn fold(event: Event, seq: u64) -> bool {
    // The cursor lock is held only during `step`: `drop_state` and the
    // state lock below re-take the same mutexes, and a non-recursive
    // std mutex deadlocks if folded with the guard still in hand.
    let step = {
        let mut cur = cursor().lock().unwrap_or_else(move |p| p.into_inner());
        cur.step(seq)
    };
    match step {
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
pub(crate) fn take_snapshot(world: State, seq: u64) {
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
    match Id::parse_str(&raw) {
        Ok(parsed) => parsed,
        Err(_) => Id::nil(),
    }
}

fn uuid_to_string(id: Id) -> String {
    id.to_string()
}

/// The presence word the UI draws. chat-model's `Busy` is "do not disturb"
/// with a name, so RWND's dnd rides it rather than collapsing to online
/// (the wire study's open question §9 asked what the policy should be once
/// the server's vocabulary grows; invisible has no counterpart and rides
/// offline in `client_set_status`).
pub(crate) fn presence_word(status: Presence) -> String {
    match status {
        Presence::Online => "online".to_string(),
        Presence::Idle => "idle".to_string(),
        Presence::Busy => "dnd".to_string(),
        Presence::Offline => "offline".to_string(),
    }
}

/// RWND's rank word for a member of a roles-carrying space: the owner is
/// "owner"; a member holding a role tagged admin (`IsAdmin`) is "moderator";
/// everyone else is "member". chat-model's `Role` is a named record now, so
/// the word comes from ownership and the admin tag rather than a fixed enum —
/// the closest RWND rank to a holder of an admin-tagged role is moderator.
pub(crate) fn role_word(space: chat_model::Space, user: Id, member: chat_model::Member) -> String {
    if user == space.owner {
        "owner".to_string()
    } else if member.roles.iter().any(move |r| chat_model::perms::covers(
        space.roles.get(r).map(move |role| role.permissions).unwrap_or(0),
        chat_model::perms::ADMIN,
    )) {
        "moderator".to_string()
    } else {
        "member".to_string()
    }
}

/// The rank word `user` holds in space `space` of the current snapshot: reads
/// the space's members and roles off the reducer, so callers that hold only
/// ids (the profile card, a member row) need not thread the space struct.
/// A person not in the space (or an unknown space) reads as "member".
pub(crate) fn role_word_of(space: Id, user: Id) -> String {
    let world = snapshot();
    match world.spaces.get(&space) {
        None => "member".to_string(),
        Some(found_space) => match found_space.members.get(&user) {
            None => "member".to_string(),
            Some(member) => role_word(found_space.clone(), user, member.clone()),
        },
    }
}

/// The relation word `FriendChanged.action` and the profile's `relationship`
/// speak. RWND names the four states exactly as chat-model orders them.
pub(crate) fn relation_word(relation: Relation) -> String {
    match relation {
        Relation::Friend => "friends".to_string(),
        Relation::Incoming => "incoming".to_string(),
        Relation::Outgoing => "outgoing".to_string(),
        Relation::Blocked => "blocked".to_string(),
    }
}

pub(crate) fn millis_at(at: Millis) -> i64 {
    at
}

// --- events → the UI's frame vocabulary ----------------------------------

/// Translate one applied live event into the frames the UI listens to.
pub(crate) fn frames_for(event: Event) -> Vec<WsServerMsg> {
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
        Event::SpaceRenamed { space, .. } => out.push(settings_changed(space)),
        Event::SpaceRemoved { space } => out.push(settings_changed(space)),
        Event::MemberJoined { space, user, .. } => out.push(member_changed(space, user.id, true)),
        Event::MemberRoles { space, .. } => out.push(roles_changed(space)),
        // Ownership moved: the space's settings row (its owner) re-reads.
        Event::OwnerChanged { space, owner: _ } => out.push(settings_changed(space)),
        // The space's role set restated: the roles screen and the channel list
        // (roles and their masks decide which channels a person sees) re-read.
        Event::RolesChanged { space, .. } => out.push(roles_changed(space)),
        // The everyone baseline restated: what every member may do moved, so
        // the same screens that a role change refreshes must re-read here too.
        Event::BaselineChanged { space, .. } => out.push(roles_changed(space)),
        Event::MemberLeft { space, user } => out.push(member_changed(space, user, false)),
        Event::CategoriesChanged { space, .. } => out.push(settings_changed(space)),
        Event::SpaceOrder { spaces } => out.push(settings_changed(space_order_first(spaces.clone()))),
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
        Event::Read { .. } => {},
        Event::VoiceJoined { .. } => {},
        Event::VoiceUpdated { .. } => {},
        Event::VoiceLeft { .. } => {},
        Event::RingUpdated { .. } => {},
        Event::RingEnded { .. } => {},
        Event::KeyRequested { .. } => {},
        Event::KeyDelivered { .. } => {},
        Event::MutedChanged { .. } => {},
        Event::DevicesChanged { .. } => {},
        Event::Resync => drop_state(),
        Event::ChannelRemoved { .. } => {},
    }
    out
}

/// The refresh frames a snapshot owes the UI. A snapshot replaces the world
/// wholesale: on connect, and again whenever the server restates it (making or
/// joining a space re-derives the connection's topics, so idea-chat answers with
/// a fresh snapshot rather than a space event). The screens re-read the state only
/// when a frame tells them to, so each space and each direct conversation in the
/// new world gets the frame its event would have produced: the server list
/// re-reads on a settings frame, and the open server's channels on a roles frame
/// (roles and overwrites decide which channels a person sees, which is exactly
/// what a restated world may have changed).
pub(crate) fn frames_for_world(world: &State) -> Vec<WsServerMsg> {
    let mut out: Vec<WsServerMsg> = Vec::new();
    for space in world.space_order.iter() {
        out.push(settings_changed(*space));
        out.push(roles_changed(*space));
    }
    for channel in world.channels.values() {
        if channel.space.is_none() {
            out.push(channel_arrived(channel.clone()));
        }
    }
    out
}

fn space_order_first(spaces: Vec<Id>) -> Id {
    match spaces.first() {
        Some(id) => *id,
        None => Id::nil(),
    }
}

pub(crate) fn profile_frame(user: User) -> WsServerMsg {
    let display = display_of(user.clone());
    WsServerMsg::ProfileChanged {
        user_id: see(user.id),
        username: user.handle,
        display_name: display,
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
        None => "removed".to_string(),
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
/// A stored body written the way people read it: the server keeps a mention as `<@uuid>` and a channel link as `<#uuid>`, but
/// the app's mention pills and channel links work on `@handle` and `#name`. Tokens it cannot name stay as they are.
pub(crate) fn readable_body(body: &str, world: &State) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let token = tail.get(0..39).filter(|t| t.ends_with('>') && (t.starts_with("<@") || t.starts_with("<#")));
        match token.and_then(|t| uuid::Uuid::parse_str(&t[2..38]).ok().map(|id| (t, id))) {
            Some((t, id)) => {
                let name = if t.starts_with("<@") {
                    world.users.get(&id).map(|u| format!("@{}", u.handle))
                } else {
                    world.channels.get(&id).map(|c| format!("#{}", c.name))
                };
                match name {
                    Some(n) => out.push_str(&n),
                    None => out.push_str(t),
                }
                rest = &tail[39..];
            }
            None => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn edited(message: Message) -> WsServerMsg {
    let content = readable_body(&message.body, &snapshot());
    WsServerMsg::MessageEdited {
        message_id: see(message.id),
        channel_id: see(message.channel),
        content,
        edited_at: millis_at(message.edited_at.unwrap_or(message.at)),
        mentioned_user_ids: message.mentions.iter().map(move |u| see(*u)).collect(),
        seq: next_frame_seq(),
    }
}

pub(crate) fn reaction(added: bool, channel: Id, message: Id, user: Id, emoji: String) -> WsServerMsg {
    let seq = next_frame_seq();
    let total = {
        let mut held = reactors().lock().unwrap_or_else(|p| p.into_inner());
        let who = held.entry((message, emoji.clone())).or_default();
        if added { who.insert(user); } else { who.remove(&user); }
        who.len() as _
    };
    if added {
        WsServerMsg::ReactionAdd { message_id: see(message), channel_id: see(channel), user_id: see(user), emoji, count: total, seq }
    } else {
        WsServerMsg::ReactionRemove { message_id: see(message), channel_id: see(channel), user_id: see(user), emoji, count: total, seq }
    }
}

/// A full message frame from a live `MessageCreated` event or a history page
/// (`new: false` for history). Author name and reactions cross through the
/// reducer state and the id table.
pub(crate) fn message_row(message: Message, new: bool) -> WsServerMsg {
    let world = snapshot();
    let content = readable_body(&message.body, &world);
    let viewer = match world.me {
        Some(me) => me.user.id,
        None => message.author,
    };
    let author_username = match world.users.get(&message.author) {
        Some(user) => user.handle.clone(),
        None => uuid_to_string(message.author),
    };
    let mut reactions: Vec<(String, u32, bool)> = Vec::new();
    for entry in message.reactions.iter() {
        reactions.push((entry.emoji.clone(), entry.users.len() as u32, entry.users.iter().any(move |u| *u == viewer)));
        reactors().lock().unwrap_or_else(|p| p.into_inner())
            .insert((message.id, entry.emoji.clone()), entry.users.iter().copied().collect());
    }
    let mut attachments: Vec<MessageAttachment> = Vec::new();
    for file in message.attachments.iter() {
        attachments.push(MessageAttachment {
            id: see(file.id),
            filename: file.name.clone(),
            mime: file.content_type.clone(),
            url: file.url.clone(),
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
        content,
        created_at: millis_at(message.at),
        edited_at: match message.edited_at {
            Some(at) => Some(millis_at(at)),
            None => None,
        },
        new,
        mentioned_user_ids: message.mentions.iter().map(move |u| see(*u)).collect(),
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
        // The call is awaited: a future that is only created does nothing, so a send
        // written without `.await` never reaches the server. A refusal shows as an
        // error frame, the way a failed history page does.
        let incoming = self.incoming;
        driver::spawn_detached(async move {
            if let Err(e) = client_send(channel_id, content, reply_to_message_id, attachments, sticker_id).await {
                incoming.set_always(Some(error_frame(reason(e))));
            }
        });
    }

    pub fn edit_message(self: GatewayHandle, message_id: i64, content: String) {
        let incoming = self.incoming;
        driver::spawn_detached(async move {
            if let Err(e) = client_edit(message_id, content).await {
                incoming.set_always(Some(error_frame(reason(e))));
            }
        });
    }

    pub fn delete_message(self: GatewayHandle, message_id: i64) {
        let incoming = self.incoming;
        driver::spawn_detached(async move {
            if let Err(e) = client_delete(message_id).await {
                incoming.set_always(Some(error_frame(reason(e))));
            }
        });
    }

    pub fn set_status(self: GatewayHandle, status: String) {
        let incoming = self.incoming;
        driver::spawn_detached(async move {
            if let Err(e) = client_set_status(status).await {
                incoming.set_always(Some(error_frame(reason(e))));
            }
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
        match upload_data_url(input.filename.clone(), input.data.clone(), model::Purpose::Message).await {
            Err(e) => return Err(e),
            Ok(id) => uploads.push(id),
        }
    }
    let mut args = Vec::new();
    args.push(client::arg(wire_of(channel_id)));
    args.push(client::arg(content));
    args.push(match reply_to_message_id {
        Some(id) => client::arg(wire_of(id)),
        None => Value::Null,
    });
    args.push(client::arg(uploads));
    // Stickers are RWND-only: a sticker send has no idea-chat counterpart and
    // the server refuses an empty body, so a sticker-only send fails there.
    match sticker_id {
        Some(_) => Err(model::not_supported()),
        None => client::call_as::<Message>("send", args).await,
    }
}

async fn client_edit(message_id: i64, content: String) -> Result<Message, ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(wire_of(message_id)));
    args.push(client::arg(content));
    client::call_as::<Message>("edit", args).await
}

async fn client_delete(message_id: i64) -> Result<(), ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(wire_of(message_id)));
    match client::post("delete", args).await {
        Err(e) => Err(e),
        Ok(_) => Ok(()),
    }
}

async fn client_set_status(status: String) -> Result<Me, ServerError> {
    let presence = match status.as_str() {
        "idle" => Presence::Idle,
        "away" => Presence::Idle,
        "dnd" => Presence::Busy,
        "busy" => Presence::Busy,
        "invisible" => Presence::Offline,
        _ => Presence::Online,
    };
    let mut args = Vec::new();
    args.push(client::arg(Status { presence, text: None }));
    client::call_as::<Me>("set_status", args).await
}

pub(crate) async fn history(channel_id: i64, before_id: Option<i64>) -> Result<Page, ServerError> {
    let window = match before_id {
        Some(id) => Window::before(wire_of(id)),
        None => Window::latest(),
    };
    let mut args = Vec::new();
    args.push(client::arg(wire_of(channel_id)));
    args.push(client::arg(window));
    client::call_as::<Page>("history", args).await
}

pub(crate) fn reason(e: ServerError) -> String {
    crate::errors::friendly(&e)
}

/// The upload triplet (`docs/idea-chat-wire.md` §3): `begin_upload` → PUT the
/// bytes → `finish_upload`. `data_url` is RWND's inline `data:<mime>;base64,`
/// attachment form; on this server the bytes go through the presigned slot
/// instead. Answers the upload id the `send` / `set_avatar` / `set_space_icon`
/// calls take.
pub(crate) async fn upload_data_url(name: String, data_url: String, purpose: model::Purpose) -> Result<Id, ServerError> {
    if !data_url.starts_with("data:") {
        return Err(ServerError::failed("only inline image data uploads on this server"));
    }
    let Some(sep) = data_url.find(";base64,") else {
        return Err(ServerError::failed("the upload data is malformed"));
    };
    let mime = &data_url[5 .. sep];
    let raw = &data_url[sep + 8 ..];
    let bytes = match crate::profile_settings::b64_decode(raw) {
        Ok(bytes) => bytes,
        Err(why) => return Err(ServerError::Codec(why)),
    };
    upload_bytes(name.to_string(), bytes, mime.to_string(), purpose).await
}

pub(crate) async fn upload_bytes(name: String, bytes: Vec<u8>, mime: String, purpose: model::Purpose) -> Result<Id, ServerError> {
    let mut args = Vec::new();
    args.push(client::arg(name));
    args.push(client::arg(bytes.len() as u64));
    args.push(client::arg(mime));
    args.push(client::arg(purpose));
    let slot = match client::call_as::<model::Slot>("begin_upload", args).await {
        Err(e) => return Err(e),
        Ok(slot) => slot,
    };
    match client::put(slot.put_url, slot.content_type, bytes).await {
        Err(e) => return Err(e),
        Ok(_) => {},
    }
    let upload_id = slot.id;
    let mut finish = Vec::new();
    finish.push(client::arg(upload_id));
    finish.push(Value::Null);
    finish.push(Value::Null);
    match client::post("finish_upload", finish).await {
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
// The socket authenticates itself with `live_ticket`, so the caller's bearer
// is not carried here.
pub(crate) fn connect(_token: String) -> GatewayHandle {
    let incoming: Signal<Option<WsServerMsg>> = signal(None);
    let status: Signal<SocketStatus> = signal(SocketStatus::Connecting);
    let sender: Signal<Option<SocketSender<Up>>> = signal(None);
    let alive = ScopeAlive::current();
    driver::spawn_detached(async move {
        let ticket = ticket().await;
        match ticket {
            Err(_) => {
                if alive.get() {
                    status.set(SocketStatus::Error);
                }
            }
            Ok(ticket) => {
                let mut ticket_args = Vec::new();
                ticket_args.push(client::arg(ticket));
                let url = client::origin() + "/_srv/_ws/live?args=" + &client::args_hex(to_string(&ticket_args).unwrap_or_default());
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
                                        Down::Snapshot { seq, state } => {
                                            take_snapshot(state.clone(), seq);
                                            // The screens watch ONE signal, and setting it twice in the same instant leaves only the
                                            // last value for them to read: a new server's settings frame (which puts its tile in the
                                            // rail and its name in the header) was overwritten by its roles frame. Spread the frames
                                            // out so each one is seen.
                                            for (index, frame) in frames_for_world(&state).into_iter().enumerate() {
                                                runtime_core::after_ms_detached((index as i32) * 25, move || incoming.set_always(Some(frame)));
                                            }
                                        }
                                        Down::Event { seq, event } => {
                                            if fold(event.clone(), seq) {
                                                for frame in frames_for(event.clone()).iter() {
                                                    incoming.set_always(Some(frame.clone()));
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
    client::call_as::<String>("live_ticket", vec![serde_json::Value::Null]).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat_model::{Id, User, Message, Presence, MessageKind, Relation, Me, Status};
    use chat_model::live::{Event, State};
    use chat_model::{Channel, ChannelKind, Role};

    fn uid(n: u128) -> Id {
        Id::from_u128(n)
    }

    #[test]
    fn a_snapshot_tells_the_screens_which_spaces_the_new_world_holds() {
        // idea-chat answers a new or joined space with a fresh snapshot, not a
        // space event; without a frame the server list never re-reads the state.
        let empty = State::default();
        assert_eq!(frames_for_world(&empty).len(), 0);

        let mut world = State::default();
        world.space_order = vec![uid(1), uid(2)];
        let frames = frames_for_world(&world);
        assert_eq!(frames.len(), 4);
        let mut settings: Vec<i64> = Vec::new();
        let mut roles: Vec<i64> = Vec::new();
        for frame in frames.iter() {
            match frame {
                WsServerMsg::ServerSettingsChanged { server_id, .. } => settings.push(*server_id),
                WsServerMsg::RolesChanged { server_id, .. } => roles.push(*server_id),
                _ => panic!("a space in a snapshot answers a settings and a roles frame"),
            }
        }
        assert_eq!(settings, vec![see(uid(1)), see(uid(2))]);
        assert_eq!(roles, vec![see(uid(1)), see(uid(2))]);
    }

    #[test]
    fn a_message_created_event_lands_as_one_message_frame() {
        // Recorded `MessageCreated` JSON from the wire study: the reducer
        // turns it into the `WsServerMsg::Message` the UI's frame watcher
        // listens to, with history-page rows marked `new: false`.
        let created = Event::MessageCreated {
            message: Message {
                id: uid(100),
                channel: uid(10),
                author: uid(7),
                kind: MessageKind::Text,
                body: "hello there".to_string(),
                attachments: Vec::new(),
                reply_to: None,
                reactions: Vec::new(),
                thread: None,
                mentions: Vec::new(),
                mentions_everyone: false,
                at: 1_700_000_000_000,
                edited_at: None,
            }
        };
        let frames = frames_for(created);
        assert_eq!(frames.len(), 1);
        match frames.get(0) {
            Some(WsServerMsg::Message { message_id, content, new, .. }) => {
                assert_eq!(*content == "hello there".to_string(), true);
                assert_eq!(*new, true);
                let _ = message_id;
            },
            Some(other) => {
                let _ = other;
                panic!("a MessageCreated event answered the wrong frame");
            },
            None => panic!("no frame for a MessageCreated event"),
        }
    }

    #[test]
    fn an_edit_carries_the_fresh_body_and_edited_at() {
        let edited = Event::MessageEdited {
            message: Message {
                id: uid(100),
                channel: uid(10),
                author: uid(7),
                kind: MessageKind::Text,
                body: "hello *there*".to_string(),
                attachments: Vec::new(),
                reply_to: None,
                reactions: Vec::new(),
                thread: None,
                mentions: Vec::new(),
                mentions_everyone: false,
                at: 1_700_000_000_000,
                edited_at: Some(1_700_000_005_000),
            }
        };
        let frames = frames_for(edited);
        match frames.get(0) {
            Some(WsServerMsg::MessageEdited { content, edited_at, .. }) => {
                assert_eq!(*content == "hello *there*".to_string(), true);
                assert_eq!(*edited_at, 1_700_000_005_000);
            },
            Some(other) => {
                let _ = other;
                panic!("wrong frame for an edit");
            },
            None => panic!("no frame for an edit"),
        }
    }

    #[test]
    fn typing_and_presence_events_answer_the_ui_frames() {
        // PresenceChanged carries chat-model's Busy; the UI word is RWND's
        // "dnd" (the presence policy — chat-model has no Invisible, so
        // invisible rides offline where the status is set).
        let typed = Event::Typing { channel: uid(10), user: uid(7) };
        let frames = frames_for(typed.clone());
        assert_eq!(frames.len(), 1);
        let frame = frames.get(0);
        match frame {
            Some(WsServerMsg::Typing { channel_id, user_id, .. }) => {
                let _ = channel_id;
                let _ = user_id;
            },
            Some(other) => {
                let _ = other;
                panic!("wrong frame for typing");
            },
            None => panic!("no frame for typing"),
        }
        assert_eq!(presence_word(Presence::Busy), "dnd".to_string());
        assert_eq!(presence_word(Presence::Offline), "offline".to_string());
    }

    #[test]
    fn relation_events_word_themselves_in_the_rwnd_vocab() {
        let changed = Event::RelationChanged {
            user: User { id: uid(7), handle: "al".to_string(), display_name: "Al".to_string(), avatar: None, bio: "".to_string() },
            relation: Some(Relation::Friend),
        };
        let frames = frames_for(changed);
        assert_eq!(frames.len(), 1);
        match frames.get(0) {
            Some(WsServerMsg::FriendChanged { action, .. }) => assert_eq!(*action == "friends".to_string(), true),
            Some(other) => {
                let _ = other;
                panic!("wrong frame for a relation change");
            },
            None => panic!("no frame for a relation change"),
        }
    }

    #[test]
    fn read_events_fold_but_answer_no_frame() {
        // `mark_read`'s fact lands in the reducer (unread counts read it)
        // without a frame: RWND's own unread badge comes from the state.
        assert_eq!(frames_for(Event::Read { channel: uid(10), last_read: uid(100) }).len(), 0);
    }

    #[test]
    fn the_cursor_ignores_a_duplicate_and_drops_on_a_gap() {
        // Per-connection seqs: apply 1,2; the replayed 2 is a Duplicate;
        // 5 after 2 is a Gap, which drops the state until a fresh snapshot.
        let mut world = State::default();
        take_snapshot(world.clone(), 0);
        let mut applied = 0;
        if fold(Event::Typing { channel: uid(10), user: uid(7) }, 1) {
            applied += 1;
        }
        if fold(Event::Typing { channel: uid(10), user: uid(7) }, 2) {
            applied += 1;
        }
        assert_eq!(applied, 2);
        // The overlap from a resnapshot at seq 2 replays the event and is
        // ignored; a jump to 5 throws the world.
        assert_eq!(fold(Event::Typing { channel: uid(10), user: uid(7) }, 2), false);
        assert_eq!(fold(Event::Typing { channel: uid(10), user: uid(7) }, 5), false);
        world = snapshot();
        assert_eq!(world, State::default());
    }

    #[test]
    fn history_paging_tags_the_window_with_the_wire_id() {
        // `history(channel, Window::Before{id})`: the selector serde'd the
        // chat-api way (tag `at`), the id as a UUID string.
        let before = Window::before(uid(42));
        let held = serde_json::to_string(&client::arg(before)).unwrap_or_default();
        assert_eq!(held.contains("\"before\""), true);
        assert_eq!(held.contains("00000000-0000-0000-0000-00000000002a"), true);
    }

    #[test]
    fn a_status_choice_lands_as_the_viewers_frame() {
        let updated = Event::MeUpdated {
            me: Me {
                user: User { id: uid(7), handle: "al".to_string(), display_name: "Al".to_string(), avatar: None, bio: "".to_string() },
                email: "al@example.com".to_string(),
                status: Status { presence: Presence::Idle, text: None },
            }
        };
        match frames_for(updated).get(0) {
            Some(WsServerMsg::StatusChoice { status, .. }) => assert_eq!(*status == "idle".to_string(), true),
            Some(other) => {
                let _ = other;
                panic!("wrong frame for a status choice");
            },
            None => panic!("no frame for a status choice"),
        }
    }

    // --- the codec round-trips: frames in the wire format, through serde and
    // the live-event translation, as the live connection sees them -----------
    //
    // Note the tags: the `Down` frame is tagged "t", but the event inside it
    // is tagged "e" (chat-model fixes this; the wire study's §2 example
    // printed "t" for the inner event — the source wins and the doc is fixed).
    // `MessageKind` carries its own "k" tag; `Presence` is a plain string.

    #[test]
    fn a_send_frame_survives_the_codec_and_lands_as_a_message_row() {
        // `Down::Event` of a `MessageCreated` — the recorded wire JSON for a
        // `send`'s fact, exactly as idea_state/chat-model's serde shapes it.
        let held = r#"{"t":"event","seq":3,"event":{"e":"message_created","message":{"id":"00000000-0000-0000-0000-000000000064","channel":"00000000-0000-0000-0000-00000000000a","author":"00000000-0000-0000-0000-000000000007","kind":{"k":"text"},"body":"hello there","attachments":[],"reply_to":null,"reactions":[],"thread":null,"mentions":[],"mentions_everyone":false,"at":1700000000000,"edited_at":null}}}"#;
        let frame = serde_json::from_str::<Down>(held);
        match frame {
            Err(e) => panic!("the frame wire shape moved: {e:?}"),
            Ok(frame) => {
                // re-encode and decode back: the codec round-trips
                let again = serde_json::to_string(&frame).unwrap_or_default();
                match serde_json::from_str::<Down>(again.as_str()) {
                    Err(e) => panic!("re-encode failed: {e:?}"),
                    Ok(back) => assert_eq!(back == frame, true),
                }
                match frame {
                    Down::Event { seq, event } => {
                        assert_eq!(seq, 3);
                        let frames = frames_for(event.clone());
                        assert_eq!(frames.len(), 1);
                        match frames.get(0) {
                            Some(WsServerMsg::Message { content, new, .. }) => {
                                assert_eq!(*content == "hello there".to_string(), true);
                                assert_eq!(*new, true);
                            },
                            _ => panic!("a MessageCreated frame answered the wrong UI frame"),
                        }
                    },
                    _ => panic!("the recorded wire frame was not an event"),
                }
            },
        }
    }

    #[test]
    fn an_edit_and_a_delete_frame_round_trip_through_the_codec() {
        // `edit`'s fact answers `MessageEdited` (the fresh body and edited_at);
        // `delete`'s answers `MessageDeleted {channel, message}`.
        let edit = r#"{"t":"event","seq":4,"event":{"e":"message_edited","message":{"id":"00000000-0000-0000-0000-000000000064","channel":"00000000-0000-0000-0000-00000000000a","author":"00000000-0000-0000-0000-000000000007","kind":{"k":"text"},"body":"hello *there*","attachments":[],"reply_to":null,"reactions":[],"thread":null,"mentions":[],"mentions_everyone":false,"at":1700000000000,"edited_at":1700000005000}}}"#;
        let deleted = r#"{"t":"event","seq":5,"event":{"e":"message_deleted","channel":"00000000-0000-0000-0000-00000000000a","message":"00000000-0000-0000-0000-000000000064"}}"#;
        let texts: Vec<String> = vec![edit.to_string(), deleted.to_string()];
        for text in texts.iter() {
            let frame = serde_json::from_str::<Down>(text.as_str());
            match frame {
                Err(why) => panic!("the frame wire shape moved: {why:?}"),
                Ok(frame) => {
                    let again = serde_json::to_string(&frame).unwrap_or_default();
                    match serde_json::from_str::<Down>(again.as_str()) {
                        Err(e) => panic!("re-encode failed: {e:?}"),
                        Ok(back) => assert_eq!(back == frame, true),
                    }
                    match frame {
                        Down::Event { seq, event } => {
                            let frames = frames_for(event.clone());
                            assert_eq!(frames.len(), 1);
                            if seq == 4 {
                                match frames.get(0) {
                                    Some(WsServerMsg::MessageEdited { content, edited_at, .. }) => {
                                        assert_eq!(*content == "hello *there*".to_string(), true);
                                        assert_eq!(*edited_at, 1_700_000_005_000);
                                    },
                                    _ => panic!("an edit answered a wrong UI frame"),
                                }
                            }
                            if seq == 5 {
                                match frames.get(0) {
                                    Some(WsServerMsg::MessageDeleted { .. }) => {},
                                    _ => panic!("a delete answered a wrong UI frame"),
                                }
                            }
                        },
                        _ => panic!("recorded wires were not events"),
                    }
                },
            }
        }
    }

    #[test]
    fn a_history_page_round_trips_through_the_codec() {
        // The `history` RPC's answer is a chat_model::Page — messages oldest
        // first with the older/present flags; its rows arrive as `new: false`.
        let page = Page {
            messages: vec![
                Message {
                    id: uid(100), channel: uid(10), author: uid(7),
                    kind: MessageKind::Text, body: "one".to_string(),
                    attachments: Vec::new(), reply_to: None, reactions: Vec::new(),
                    thread: None, mentions: Vec::new(), mentions_everyone: false,
                    at: 1_700_000_000_000, edited_at: None,
                },
                Message {
                    id: uid(101), channel: uid(10), author: uid(8),
                    kind: MessageKind::Text, body: "two".to_string(),
                    attachments: Vec::new(), reply_to: None, reactions: Vec::new(),
                    thread: None, mentions: Vec::new(), mentions_everyone: false,
                    at: 1_700_000_001_000, edited_at: None,
                },
            ],
            older: true,
            present: false,
        };
        let held = serde_json::to_string(&page).unwrap_or_default();
        let back = serde_json::from_str::<Page>(held.as_str());
        match back {
            Err(why) => panic!("the page wire shape moved: {why:?}"),
            Ok(restored) => {
                assert_eq!(restored, page);
                let frames: Vec<WsServerMsg> = restored.messages.iter().map(move |m| message_row(m.clone(), false)).collect();
                assert_eq!(frames.len(), 2);
                match frames.get(0) {
                    Some(WsServerMsg::Message { new, .. }) => assert_eq!(*new, false),
                    _ => panic!("a history row answered a wrong UI frame"),
                }
            },
        }
    }

    #[test]
    fn a_read_frame_folds_to_an_unread_count_of_zero() {
        // `mark_read`'s fact on the wire, folded into a small world that has
        // one channel sitting on one unread message: after the fold the badge
        // is zero and the frame stays out (the state, not a frame, answers it).
        let mut world = State::default();
        world.channels.insert(uid(10), Channel {
            id: uid(10),
            kind: ChannelKind::Text,
            space: Some(uid(1)),
            name: "general".to_string(),
            topic: "".to_string(),
            category: None,
            position: 0,
            parent: None,
            starter: None,
            view: 0,
            post: 0,
            members: Vec::new(),
            archived: false,
            overwrites: std::collections::BTreeMap::new(),
            last_message: Some(uid(100)),
            message_count: 1,
            created_by: uid(2),
            created_at: 0,
        });
        let read = r#"{"t":"event","seq":6,"event":{"e":"read","channel":"00000000-0000-0000-0000-00000000000a","last_read":"00000000-0000-0000-0000-000000000064"}}"#;
        let frame = serde_json::from_str::<Down>(read);
        match frame {
            Err(why) => panic!("the read frame shape moved: {why:?}"),
            Ok(frame) => {
                match frame {
                    Down::Event { seq, event } => {
                        assert_eq!(seq, 6);
                        assert_eq!(frames_for(event.clone()).len(), 0);
                        world.apply(&event);
                        let counts = crate::backend::idea_chat::impl_messages::unread_of(world.clone(), uid(1));
                        assert_eq!(counts.len(), 1);
                        match counts.get(0) {
                            Some(row) => assert_eq!(row.count, 0),
                            None => panic!("no unread row for the read channel"),
                        }
                    },
                    _ => panic!("recorded read frame was not an event"),
                }
            },
        }
    }

    #[test]
    fn a_presence_round_trip_lands_both_ways() {
        // Viewer side: `set_status`'s fresh `Me` event answers a StatusChoice;
        // witness side: `PresenceChanged` answers a Presence frame.
        let me_json = r#"{"t":"event","seq":7,"event":{"e":"me_updated","me":{"user":{"id":"00000000-0000-0000-0000-000000000007","handle":"al","display_name":"al","avatar":null,"bio":""},"email":"al","status":{"presence":"idle","text":null}}}}"#;
        let presence_json = r#"{"t":"event","seq":8,"event":{"e":"presence_changed","user":"00000000-0000-0000-0000-000000000008","status":{"presence":"busy","text":null}}}"#;
        let texts: Vec<(String, String)> = vec![
            (me_json.to_string(), "idle".to_string()),
            (presence_json.to_string(), "dnd".to_string()),
        ];
        for pair in texts.iter() {
            let text = pair.0.clone();
            let expect_word = pair.1.clone();
            let is_me = pair.0 == me_json;
            let frame = serde_json::from_str::<Down>(text.as_str());
            match frame {
                Err(why) => panic!("the presence frame shape moved: {why:?}"),
                Ok(frame) => {
                    let again = serde_json::to_string(&frame).unwrap_or_default();
                    match serde_json::from_str::<Down>(again.as_str()) {
                        Err(e) => panic!("re-encode failed: {e:?}"),
                        Ok(back) => assert_eq!(back == frame, true),
                    }
                    match frame {
                        Down::Event { seq, event } => {
                            assert_eq!(seq, if is_me { 7 } else { 8 });
                            let frames = frames_for(event.clone());
                            assert_eq!(frames.len(), 1);
                            match frames.get(0) {
                                Some(WsServerMsg::StatusChoice { status, .. }) => assert_eq!(*status == expect_word, true),
                                Some(WsServerMsg::Presence { status, .. }) => assert_eq!(*status == expect_word, true),
                                _ => panic!("a presence event answered a wrong UI frame"),
                            }
                        },
                        _ => panic!("recorded presence was not an event"),
                    }
                },
            }
        }
    }
}
