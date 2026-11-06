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
    ChannelResult,
    MarkReadResult,
    PinResult,
    PinnedMessage,
    ReactionResult,
    ServerSettingsResult,
    UnreadCount,
    WsClientMsg,
    WsServerMsg,
};
pub use discord_server::dms::{Dm, DmMessage, SendDmMessageResult};
pub use discord_server::servers::{ChannelSummary, ServerSummary};
pub use discord_server::invites::{Invite, RedeemResult};
pub use discord_server::friends::{FriendList, FriendUser, SendFriendRequestResult};
pub use discord_server::blocks::BlockedUser;
pub use discord_server::auth::{
    LoginResult,
    SignupResult,
    ChangeUsernameResult,
    ChangePasswordResult,
    LogoutResult,
    AvatarResult,
};
pub use discord_server::password_resets::ResetPasswordResult;
pub use discord_server::profiles::{ProfileFields, UserProfile};
pub use discord_server::notification_settings::NotificationSetting;
pub use discord_server::server_emoji::ServerEmoji;
pub use discord_server::gifs::GifPage;
pub use discord_server::search::SearchResults;

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

/// Servers and channels: discover them, create them, and edit their
/// settings (name, icon, ownership) — plus leaving a server. A channel's
/// permission edits are `PermissionsApi`'s; reading a member roster is
/// `MembersApi`'s.
pub trait ServersApi {
    async fn list_servers() -> Result<Vec<ServerSummary>, ServerError>;
    async fn create_server(name: String) -> Result<ServerSummary, ServerError>;
    async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, ServerError>;

    async fn create_channel(
        server_id: i64,
        name: String,
        kind: String,
        parent_channel_id: Option<i64>,
        private: bool,
    ) -> Result<ChannelResult, ServerError>;
    async fn rename_channel(
        server_id: i64,
        channel_id: i64,
        new_name: String,
    ) -> Result<ChannelResult, ServerError>;
    async fn delete_channel(
        server_id: i64,
        channel_id: i64,
    ) -> Result<ChannelResult, ServerError>;

    async fn rename_server(
        server_id: i64,
        new_name: String,
    ) -> Result<ServerSettingsResult, ServerError>;
    async fn set_server_icon(
        server_id: i64,
        icon: Option<String>,
    ) -> Result<ServerSettingsResult, ServerError>;
    async fn transfer_server_ownership(
        server_id: i64,
        new_owner_id: i64,
    ) -> Result<ServerSettingsResult, ServerError>;
    async fn leave_server(server_id: i64) -> Result<(), ServerError>;
}

/// Invites: mint a shareable code for a server, and join through one.
pub trait InvitesApi {
    async fn create_invite(
        server_id: i64,
        expires_at: Option<i64>,
        max_uses: Option<i64>,
    ) -> Result<Invite, ServerError>;
    async fn redeem_invite(code: String) -> Result<RedeemResult, ServerError>;
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

/// Social graph: the signed-in user's friends, requests, and blocks. A
/// mutation answers with the single changed relationship; the refreshed
/// lists the UI re-renders from come from the accessors below.
pub trait SocialApi {
    async fn list_friends() -> Result<FriendList, ServerError>;
    async fn send_friend_request(username: String) -> Result<SendFriendRequestResult, ServerError>;
    async fn accept_friend_request(user_id: i64) -> Result<FriendUser, ServerError>;
    async fn decline_or_cancel(user_id: i64) -> Result<FriendUser, ServerError>;
    async fn remove_friend(user_id: i64) -> Result<FriendUser, ServerError>;

    async fn list_blocked() -> Result<Vec<BlockedUser>, ServerError>;
    async fn block_user(user_id: i64) -> Result<BlockedUser, ServerError>;
    async fn unblock_user(user_id: i64) -> Result<BlockedUser, ServerError>;
}

/// The realtime event stream: a backend offers exactly one connection
/// point; [`connect`] hands back the [`Gateway`] handle that carries every
/// frame the backend pushes (and the client's own outbound verbs).
pub trait EventStreamApi {
    fn connect(token: String) -> Gateway;
}

/// The signed-in account itself: sign in and out, rename, change password,
/// the avatar, and the profile (display name, about). The identity providers
/// in `crate::identity` sit on this area.
pub trait AccountApi {
    async fn login(username: String, password: String) -> Result<LoginResult, ServerError>;
    async fn signup(username: String, password: String) -> Result<SignupResult, ServerError>;
    async fn logout() -> Result<LogoutResult, ServerError>;
    async fn logout_everywhere() -> Result<LogoutResult, ServerError>;
    async fn change_password(current_password: String, new_password: String) -> Result<ChangePasswordResult, ServerError>;
    async fn change_username(new_username: String) -> Result<ChangeUsernameResult, ServerError>;
    async fn reset_password(token: String, new_password: String) -> Result<ResetPasswordResult, ServerError>;

    async fn set_avatar(avatar_url: Option<String>) -> Result<AvatarResult, ServerError>;

    async fn get_user_profile(user_id: i64, server_id: Option<i64>) -> Result<UserProfile, ServerError>;
    async fn set_profile(display_name: String, about: String) -> Result<ProfileFields, ServerError>;
}

/// Notification preferences: the user's whole settings table, and saving
/// one row (mute windows and notification levels per server/channel).
pub trait NotificationsApi {
    async fn notification_settings(server_id: Option<i64>) -> Result<Vec<NotificationSetting>, ServerError>;
    async fn set_notification_setting(setting: NotificationSetting) -> Result<NotificationSetting, ServerError>;
}

/// Custom emoji: the server's emoji table, and upload / rename / delete.
pub trait EmojiApi {
    async fn list_server_emoji(server_id: i64) -> Result<Vec<ServerEmoji>, ServerError>;
    async fn upload_emoji(server_id: i64, name: String, data_url: String) -> Result<ServerEmoji, ServerError>;
    async fn rename_emoji(emoji_id: i64, name: String) -> Result<ServerEmoji, ServerError>;
    async fn delete_emoji(emoji_id: i64) -> Result<(), ServerError>;
}

/// The GIF search service: paging through results for a query.
pub trait GifApi {
    async fn gif_search(query: String, offset: u32) -> Result<GifPage, ServerError>;
}

/// Text search over one server's messages.
pub trait SearchApi {
    async fn search_messages(server_id: i64, query: String, tz_offset_minutes: i64, offset: i64) -> Result<SearchResults, ServerError>;
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
mod invites_impl;
mod realtime;
mod servers_impl;
mod social_impl;
mod account_impl;
mod notif_impl;
mod emoji_impl;
mod gif_impl;
mod search_impl;

// --- the accessors (the single place the UI gets the backend) ---------------

/// Open the realtime stream: the signed session token authenticates the
/// connect (an empty token gets no working socket).
pub fn connect(token: String) -> Gateway {
    CassetteBackend::connect(token)
}

// --- the server and channel accessors ---------------------------------------

pub async fn list_servers() -> Result<Vec<ServerSummary>, String> {
    CassetteBackend::list_servers().await.map_err(|e| crate::errors::friendly(&e))
}

// --- the invite accessors ---------------------------------------------------

pub async fn create_invite(
    server_id: i64,
    expires_at: Option<i64>,
    max_uses: Option<i64>,
) -> Result<Invite, String> {
    CassetteBackend::create_invite(server_id, expires_at, max_uses)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn redeem_invite(code: String) -> Result<RedeemResult, String> {
    CassetteBackend::redeem_invite(code).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_server(name: String) -> Result<ServerSummary, String> {
    CassetteBackend::create_server(name).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, String> {
    CassetteBackend::list_channels(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_channel(
    server_id: i64,
    name: String,
    kind: String,
    parent_channel_id: Option<i64>,
    private: bool,
) -> Result<ChannelResult, String> {
    CassetteBackend::create_channel(server_id, name, kind, parent_channel_id, private)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_channel(
    server_id: i64,
    channel_id: i64,
    new_name: String,
) -> Result<ChannelResult, String> {
    CassetteBackend::rename_channel(server_id, channel_id, new_name)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_channel(
    server_id: i64,
    channel_id: i64,
) -> Result<ChannelResult, String> {
    CassetteBackend::delete_channel(server_id, channel_id)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_server(
    server_id: i64,
    new_name: String,
) -> Result<ServerSettingsResult, String> {
    CassetteBackend::rename_server(server_id, new_name)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_server_icon(
    server_id: i64,
    icon: Option<String>,
) -> Result<ServerSettingsResult, String> {
    CassetteBackend::set_server_icon(server_id, icon)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn transfer_server_ownership(
    server_id: i64,
    new_owner_id: i64,
) -> Result<ServerSettingsResult, String> {
    CassetteBackend::transfer_server_ownership(server_id, new_owner_id)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn leave_server(server_id: i64) -> Result<(), String> {
    CassetteBackend::leave_server(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

// --- the message accessors --------------------------------------------------
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

// --- the friends and blocks accessors ---------------------------------------

pub async fn list_friends() -> Result<FriendList, String> {
    CassetteBackend::list_friends().await.map_err(|e| crate::errors::friendly(&e))
}

/// Send a friend request, then answer with the actor's refreshed list — the
/// backend's `FriendChanged` frame only reaches the other user.
pub async fn send_friend_request(username: String) -> Result<FriendList, String> {
    CassetteBackend::send_friend_request(username).await.map_err(|e| crate::errors::friendly(&e))?;
    CassetteBackend::list_friends().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn accept_friend_request(user_id: i64) -> Result<FriendList, String> {
    CassetteBackend::accept_friend_request(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    CassetteBackend::list_friends().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn decline_or_cancel(user_id: i64) -> Result<FriendList, String> {
    CassetteBackend::decline_or_cancel(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    CassetteBackend::list_friends().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_friend(user_id: i64) -> Result<FriendList, String> {
    CassetteBackend::remove_friend(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    CassetteBackend::list_friends().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_blocked() -> Result<Vec<BlockedUser>, String> {
    CassetteBackend::list_blocked().await.map_err(|e| crate::errors::friendly(&e))
}

/// Block (or, with `block` false, unblock) `user_id`, then answer with the
/// refreshed blocked list so the message list can re-filter.
pub async fn set_blocked(user_id: i64, block: bool) -> Result<Vec<BlockedUser>, String> {
    if block {
        CassetteBackend::block_user(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    } else {
        CassetteBackend::unblock_user(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    }
    CassetteBackend::list_blocked().await.map_err(|e| crate::errors::friendly(&e))
}

// --- the account accessors --------------------------------------------------

pub async fn login(username: String, password: String) -> Result<LoginResult, String> {
    CassetteBackend::login(username, password).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn signup(username: String, password: String) -> Result<SignupResult, String> {
    CassetteBackend::signup(username, password).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn logout() -> Result<LogoutResult, String> {
    CassetteBackend::logout().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn logout_everywhere() -> Result<LogoutResult, String> {
    CassetteBackend::logout_everywhere().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn change_password(current_password: String, new_password: String) -> Result<ChangePasswordResult, String> {
    CassetteBackend::change_password(current_password, new_password).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn change_username(new_username: String) -> Result<ChangeUsernameResult, String> {
    CassetteBackend::change_username(new_username).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn reset_password(token: String, new_password: String) -> Result<String, String> {
    CassetteBackend::reset_password(token, new_password).await
        .map(|r| r.username)
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_avatar(avatar_url: Option<String>) -> Result<AvatarResult, String> {
    CassetteBackend::set_avatar(avatar_url).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn get_user_profile(user_id: i64, server_id: Option<i64>) -> Result<UserProfile, String> {
    CassetteBackend::get_user_profile(user_id, server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_profile(display_name: String, about: String) -> Result<ProfileFields, String> {
    CassetteBackend::set_profile(display_name, about).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn notification_settings(server_id: Option<i64>) -> Result<Vec<NotificationSetting>, String> {
    CassetteBackend::notification_settings(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_notification_setting(setting: NotificationSetting) -> Result<NotificationSetting, String> {
    CassetteBackend::set_notification_setting(setting).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_server_emoji(server_id: i64) -> Result<Vec<ServerEmoji>, String> {
    CassetteBackend::list_server_emoji(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn upload_emoji(server_id: i64, name: String, data_url: String) -> Result<ServerEmoji, String> {
    CassetteBackend::upload_emoji(server_id, name, data_url).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_emoji(emoji_id: i64, name: String) -> Result<ServerEmoji, String> {
    CassetteBackend::rename_emoji(emoji_id, name).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_emoji(emoji_id: i64) -> Result<(), String> {
    CassetteBackend::delete_emoji(emoji_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn gif_search(query: String, offset: u32) -> Result<GifPage, String> {
    CassetteBackend::gif_search(query, offset).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn search_messages(server_id: i64, query: String, tz_offset_minutes: i64, offset: i64) -> Result<SearchResults, String> {
    CassetteBackend::search_messages(server_id, query, tz_offset_minutes, offset).await.map_err(|e| crate::errors::friendly(&e))
}
