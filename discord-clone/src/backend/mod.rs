//! The chat-backend seam: the single interface the UI talks to, and today's
//! implementation of it.
//!
//! The client reaches its chat backend only through this module. The
//! interface is a set of traits grouped by area — [`MessagesApi`] (read
//! state, pins, reactions) and [`DmsApi`] (direct messages) — and a realtime
//! [`EventStreamApi`] for the gateway connection: [`connect`] returns a
//! [`Gateway`] handle whose verbs ([`Gateway.request_history`] etc.) carry
//! the outbound frames and whose [`Gateway.incoming`] feed yields everything
//! the backend pushes. The implementation files beside this one
//! ([`messages_impl`], [`dms_impl`], [`realtime`]) fill the traits in with
//! today's `discord-server` RPC stubs and its gateway socket; a different
//! backend later replaces only those files, not the UI. Only the CALLS moved
//! here — the wire types still ride in from `discord_server` (a shared types
//! crate is a later decision). Audio, video and screenshare stay out of this
//! seam and get their own interface later.
//!
//! UI sites reach an operation as `crate::backend::<verb>` (the free
//! functions below) or through the [`Gateway`] handle — never a raw RPC, a
//! wire frame, or a trait method — so the backend never has to be imported
//! or constructed at a call site.

use runtime_core::Signal;
use server::ServerError;
use server::SocketStatus;

pub use discord_server::server_settings::{
    AttachmentInput,
    MarkReadResult,
    PinResult,
    PinnedMessage,
    ReactionResult,
    UnreadCount,
    WsClientMsg,
    WsServerMsg,
};
pub use discord_server::dms::{Dm, DmMessage, SendDmMessageResult};

// --- the interface, by area -------------------------------------------------

/// Channel-message operations over the REST half of the backend: read state
/// (unread counts, mark-read), pins, and reactions. Sending, editing,
/// deleting, loading history pages and the typing indicator are realtime,
/// and ride the [`Gateway`] handle instead.
pub trait MessagesApi {
    async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, ServerError>;
    async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, ServerError>;

    async fn pin_message(message_id: i64) -> Result<PinResult, ServerError>;
    async fn unpin_message(message_id: i64) -> Result<PinResult, ServerError>;
    async fn list_pinned(channel_id: i64) -> Result<Vec<PinnedMessage>, ServerError>;
    async fn pinned_count(channel_id: i64) -> Result<i64, ServerError>;

    async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, ServerError>;
    async fn remove_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, ServerError>;
}

/// Direct messages: open a conversation, load its messages, send into it,
/// and change its membership (1:1 and group).
pub trait DmsApi {
    async fn list_dms() -> Result<Vec<Dm>, ServerError>;
    async fn start_dm(user_id: i64) -> Result<Dm, ServerError>;
    async fn send_dm_message(channel_id: i64, content: String) -> Result<SendDmMessageResult, ServerError>;
    async fn list_dm_messages(channel_id: i64, limit: Option<i64>) -> Result<Vec<DmMessage>, ServerError>;

    async fn create_group_dm(user_ids: Vec<i64>, name: String) -> Result<Dm, ServerError>;
    async fn add_group_dm_member(dm_id: i64, user_id: i64) -> Result<Dm, ServerError>;
    async fn remove_group_dm_member(dm_id: i64, user_id: i64) -> Result<(), ServerError>;
    async fn rename_group_dm(dm_id: i64, name: String) -> Result<Dm, ServerError>;
}

/// The realtime event stream: a backend offers exactly one connection
/// point; [`connect`] hands back the [`Gateway`] handle that carries every
/// frame the backend pushes (and the client's own outbound verbs).
pub trait EventStreamApi {
    fn connect(token: String) -> Gateway;
}

/// Today's backend: the implementation files below implement the areas
/// above for this type.
pub struct CassetteBackend;

// --- the realtime handle ----------------------------------------------------

/// The event-stream handle: what the UI holds after [`connect`]. Cheap to
/// clone. Its verbs ask the backend for a history page, a message send /
/// edit / delete, the typing indicator, or a status pick; [`incoming`]
/// yields every frame the backend pushes and [`connection_status`] reports
/// the connection. The UI only calls these — it never builds a frame or
/// connects itself.
#[derive(Clone, Copy)]
pub struct Gateway {
    handle: server::UseSocket<WsServerMsg, WsClientMsg>,
}

impl Gateway {
    pub fn request_history(self: Gateway, channel_id: i64, before_id: Option<i64>) {
        self.handle.send(WsClientMsg::History { channel_id, before_id });
    }
    pub fn send_message(
        self: Gateway,
        channel_id: i64,
        content: String,
        reply_to_message_id: Option<i64>,
        attachments: Vec<AttachmentInput>,
    ) {
        self.handle.send(WsClientMsg::Send {
            channel_id,
            content,
            reply_to_message_id,
            attachments,
        });
    }
    pub fn edit_message(self: Gateway, message_id: i64, content: String) {
        self.handle.send(WsClientMsg::EditMessage { message_id, content });
    }
    pub fn delete_message(self: Gateway, message_id: i64) {
        self.handle.send(WsClientMsg::DeleteMessage { message_id });
    }
    pub fn announce_typing(self: Gateway, channel_id: i64) {
        self.handle.send(WsClientMsg::Typing { channel_id });
    }
    pub fn set_status(self: Gateway, status: String) {
        self.handle.send(WsClientMsg::SetStatus { status });
    }
    pub fn incoming(self: Gateway) -> Signal<Option<WsServerMsg>> {
        self.handle.incoming()
    }
    pub fn connection_status(self: Gateway) -> SocketStatus {
        self.handle.status()
    }
}

// The areas' implementations, with today's server behind them.
mod messages_impl;
mod dms_impl;
mod realtime;

// --- the accessors (the single place the UI gets the backend) ---------------

/// Open the realtime stream: the signed session token authenticates the
/// connect (an empty token gets no working socket).
pub fn connect(token: String) -> Gateway {
    CassetteBackend::connect(token)
}

// The message accessors keep the per-area wrappers' behaviour: a rejection
// becomes friendly text (`Result<_, String>`), so the UI shows the server's
// own message verbatim.
pub async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, String> {
    CassetteBackend::mark_read(channel_id, message_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, String> {
    CassetteBackend::unread_counts(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn pin_message(message_id: i64) -> Result<PinResult, String> {
    CassetteBackend::pin_message(message_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn unpin_message(message_id: i64) -> Result<PinResult, String> {
    CassetteBackend::unpin_message(message_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_pinned(channel_id: i64) -> Result<Vec<PinnedMessage>, String> {
    CassetteBackend::list_pinned(channel_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn pinned_count(channel_id: i64) -> Result<i64, String> {
    CassetteBackend::pinned_count(channel_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    CassetteBackend::add_reaction(message_id, emoji).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    CassetteBackend::remove_reaction(message_id, emoji).await.map_err(|e| crate::errors::friendly(&e))
}

// --- the direct-message accessors -------------------------------------------

/// The DM accessors keep the client glue the per-area wrappers used to hold:
/// a rejection becomes friendly text (`Result<_, String>`), and every
/// returned participant lands in the shared avatar / display-name registries
/// so the DM rows draw the real image on a cold load.

/// Pull every participant of a DM list into the shared registries.
fn remember_dm_avatars(dms: Vec<Dm>) {
    for dm in dms.iter() {
        for p in dm.participants.iter() {
            crate::auth::remember_avatar(p.username.as_str(), p.avatar_url.clone());
            crate::auth::remember_display_name(p.username.as_str(), p.display_name.clone());
        }
    }
}

pub async fn list_dms() -> Result<Vec<Dm>, String> {
    let list = CassetteBackend::list_dms().await.map_err(|e| crate::errors::friendly(&e))?;
    remember_dm_avatars(list.clone());
    Ok(list)
}

pub async fn start_dm(user_id: i64) -> Result<Dm, String> {
    let dm = CassetteBackend::start_dm(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    for p in dm.participants.iter() {
        crate::auth::remember_avatar(p.username.as_str(), p.avatar_url.clone());
        crate::auth::remember_display_name(p.username.as_str(), p.display_name.clone());
    }
    Ok(dm)
}

/// Send into a DM and return the posted message. The backend pushes the
/// message as an event-stream frame to every participant (the sender
/// included), so the caller does NOT append the reply itself — the
/// [`incoming`] feed renders it.
pub async fn send_dm_message(channel_id: i64, content: String) -> Result<DmMessage, String> {
    let result = CassetteBackend::send_dm_message(channel_id, content).await.map_err(|e| crate::errors::friendly(&e))?;
    Ok(result.message)
}

pub async fn list_dm_messages(channel_id: i64, limit: Option<i64>) -> Result<Vec<DmMessage>, String> {
    let history = CassetteBackend::list_dm_messages(channel_id, limit).await.map_err(|e| crate::errors::friendly(&e))?;
    for m in history.iter() {
        crate::auth::remember_avatar(m.author_username.as_str(), m.author_avatar.clone());
    }
    Ok(history)
}

pub async fn create_group_dm(user_ids: Vec<i64>, name: String) -> Result<Dm, String> {
    CassetteBackend::create_group_dm(user_ids, name).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn add_group_dm_member(dm_id: i64, user_id: i64) -> Result<Dm, String> {
    CassetteBackend::add_group_dm_member(dm_id, user_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_group_dm_member(dm_id: i64, user_id: i64) -> Result<(), String> {
    CassetteBackend::remove_group_dm_member(dm_id, user_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_group_dm(dm_id: i64, name: String) -> Result<Dm, String> {
    CassetteBackend::rename_group_dm(dm_id, name).await.map_err(|e| crate::errors::friendly(&e))
}
