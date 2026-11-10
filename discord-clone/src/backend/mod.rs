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
    WsServerMsg,
};
/// The gateway's CLIENT frame type: only the Cassette socket's verbs speak it,
/// so the idea-chat build (whose `Gateway` posts RPCs instead of frames) leaves
/// it out, keeping that build warning-free.
#[cfg(not(feature = "idea-chat"))]
pub use discord_server::server_settings::WsClientMsg;
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
/// Notification rows the settings screens draw: RWND's own vocabulary, so
/// only the Cassette build reads them.
#[cfg(not(feature = "idea-chat"))]
pub use discord_server::notification_settings::{
    Level,
    Scope,
    MUTED_UNTIL_TURNED_OFF,
    is_muted,
};
pub use discord_server::server_emoji::ServerEmoji;
#[cfg(not(feature = "idea-chat"))]
pub use discord_server::server_emoji::MAX_PER_SERVER;
pub use discord_server::stickers::{Sticker, MAX_PER_SERVER as STICKERS_MAX_PER_SERVER};
pub use discord_server::gifs::GifPage;
#[cfg(not(feature = "idea-chat"))]
pub use discord_server::gifs::{gif_body, gif_message};
pub use discord_server::search::SearchResults;
pub use discord_server::server_settings::{MemberInfo, MemberResult};
pub use discord_server::server_roles::{RolesSnapshot, CustomRole};
pub use discord_server::permissions::{Permissions, Target, MyPermissions, Overwrite};
pub use discord_server::custom_themes::{
    CustomTheme,
    SaveCustomThemeInput,
    RenameCustomThemeInput,
    DeleteCustomThemeInput,
};

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
    async fn send_dm_message(channel_id: i64, content: String, sticker_id: Option<i64>) -> Result<SendDmMessageResult, ServerError>;
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
    /// Wear a catalog avatar decoration (`None` takes it off); returns what is now worn.
    async fn set_avatar_decoration(decoration: Option<String>) -> Result<Option<String>, ServerError>;
    /// Put a catalog effect on the profile card (`None` takes it off); returns what is now set.
    async fn set_profile_effect(effect: Option<String>) -> Result<Option<String>, ServerError>;
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

/// A server's stickers: the list, upload, edit and delete.
pub trait StickersApi {
    async fn list_stickers(server_id: i64) -> Result<Vec<Sticker>, ServerError>;
    async fn upload_sticker(
        server_id: i64,
        name: String,
        description: String,
        related_emoji: String,
        data_url: String,
    ) -> Result<Sticker, ServerError>;
    async fn edit_sticker(
        sticker_id: i64,
        name: String,
        description: String,
        related_emoji: String,
    ) -> Result<Sticker, ServerError>;
    async fn delete_sticker(sticker_id: i64) -> Result<(), ServerError>;
}

/// The GIF search service: paging through results for a query.
pub trait GifApi {
    async fn gif_search(query: String, offset: u32) -> Result<GifPage, ServerError>;
}

/// Text search over one server's messages.
pub trait SearchApi {
    async fn search_messages(server_id: i64, query: String, tz_offset_minutes: i64, offset: i64) -> Result<SearchResults, ServerError>;
}

/// A server's members: the roster, and adding, kicking, banning, timing out
/// or re-adding a member.
pub trait MembersApi {
    async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, ServerError>;
    async fn add_member(server_id: i64, user_id: i64) -> Result<MemberResult, ServerError>;
    async fn remove_member(server_id: i64, user_id: i64) -> Result<MemberResult, ServerError>;
    async fn ban_member(server_id: i64, user_id: i64, reason: Option<String>) -> Result<MemberResult, ServerError>;
    async fn timeout_member(server_id: i64, user_id: i64, duration_secs: i64) -> Result<MemberResult, ServerError>;
}

/// Named roles: the snapshot, and create / rename / recolour / reorder /
/// delete, plus giving or taking a role from a member.
pub trait RolesApi {
    async fn server_roles(server_id: i64) -> Result<RolesSnapshot, ServerError>;
    async fn create_role(server_id: i64, name: String, color: String, mentionable: bool) -> Result<CustomRole, ServerError>;
    async fn update_role(role_id: i64, name: String, color: String, mentionable: bool, permissions: Option<Permissions>) -> Result<CustomRole, ServerError>;
    async fn move_role(role_id: i64, position: i64) -> Result<CustomRole, ServerError>;
    async fn delete_role(role_id: i64) -> Result<(), ServerError>;
    async fn assign_role(role_id: i64, user_id: i64, assigned: bool) -> Result<(), ServerError>;
}

/// What the caller may do: the server-wide answer and per-channel overwrites.
pub trait PermissionsApi {
    async fn my_permissions(server_id: i64) -> Result<MyPermissions, ServerError>;
    async fn channel_overwrites(channel_id: i64) -> Result<Vec<Overwrite>, ServerError>;
    async fn set_channel_overwrite(channel_id: i64, target: Target, allow: Permissions, deny: Permissions) -> Result<Vec<Overwrite>, ServerError>;
    async fn set_everyone_permissions(server_id: i64, permissions: Permissions) -> Result<Permissions, ServerError>;
}

/// Colour themes a person makes for themselves: the per-account store
/// behind the custom-theme screens.
pub trait ThemesApi {
    async fn list_custom_themes() -> Result<Vec<CustomTheme>, ServerError>;
    async fn save_custom_theme(
        name: String,
        base: String,
        colors: Vec<String>,
    ) -> Result<CustomTheme, ServerError>;
    async fn rename_custom_theme(
        from: String,
        to: String,
    ) -> Result<CustomTheme, ServerError>;
    async fn delete_custom_theme(name: String) -> Result<(), ServerError>;
}

/// Today's backend: the implementation files below implement the areas
/// above for this type.
pub struct CassetteBackend;

/// The SECOND implementation of every area trait above, talked to over the
/// idea-chat wire (see `docs/idea-chat-wire.md`). Selected at build time
/// below by the `idea-chat` feature; RWND-only areas answer
/// `Err("not supported on this server")`.
#[cfg(feature = "idea-chat")]
pub struct IdeaChatBackend;

/// THE ONE SWITCH POINT: what `Backend` means this build. With the
/// `idea-chat` feature off (the default) the accessors below compile
/// against CassetteBackend exactly as today; with it on they compile
/// against IdeaChatBackend instead. No call site changes.
#[cfg(not(feature = "idea-chat"))]
type Backend = CassetteBackend;
#[cfg(feature = "idea-chat")]
type Backend = IdeaChatBackend;

// --- the realtime handle ----------------------------------------------------

/// The event-stream handle: what the UI holds after [`connect`]. Cheap to
/// clone. Its verbs ask the backend for a history page, a message send /
/// edit / delete, the typing indicator, or a status pick; [`incoming`]
/// yields every frame the backend pushes and [`connection_status`] reports
/// the connection. The UI only calls these — it never builds a frame or
/// connects itself.
/// Cassette flavor: the live gateway socket, typed at both ends.
#[cfg(not(feature = "idea-chat"))]
#[derive(Clone, Copy)]
pub struct Gateway {
    handle: server::UseSocket<WsServerMsg, WsClientMsg>,
}

#[cfg(not(feature = "idea-chat"))]
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
        sticker_id: Option<i64>,
    ) {
        self.handle.send(WsClientMsg::Send {
            channel_id,
            content,
            reply_to_message_id,
            attachments,
            sticker_id,
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

/// The idea-chat flavor: the same handle shape, backed by the second backend's
/// live connection ([`crate::backend::idea_chat::realtime`]). Its verbs post the
/// idea-chat RPCs (send / edit / delete are calls, not frames — the facts they
/// cause arrive as live events) and only `announce_typing` is a socket Action;
/// [`incoming`] yields the `WsServerMsg` frames the live loop synthesizes from
/// those events.
#[cfg(feature = "idea-chat")]
#[derive(Clone, Copy)]
pub struct Gateway {
    handle: crate::backend::idea_chat::live::GatewayHandle,
}

#[cfg(feature = "idea-chat")]
impl Gateway {
    pub fn request_history(self: Gateway, channel_id: i64, before_id: Option<i64>) {
        self.handle.request_history(channel_id, before_id);
    }
    pub fn send_message(
        self: Gateway,
        channel_id: i64,
        content: String,
        reply_to_message_id: Option<i64>,
        attachments: Vec<AttachmentInput>,
        sticker_id: Option<i64>,
    ) {
        self.handle.send_message(channel_id, content, reply_to_message_id, attachments, sticker_id);
    }
    pub fn edit_message(self: Gateway, message_id: i64, content: String) {
        self.handle.edit_message(message_id, content);
    }
    pub fn delete_message(self: Gateway, message_id: i64) {
        self.handle.delete_message(message_id);
    }
    pub fn announce_typing(self: Gateway, channel_id: i64) {
        self.handle.announce_typing(channel_id);
    }
    pub fn set_status(self: Gateway, status: String) {
        self.handle.set_status(status);
    }
    pub fn incoming(self: Gateway) -> Signal<Option<WsServerMsg>> {
        self.handle.incoming()
    }
    pub fn connection_status(self: Gateway) -> SocketStatus {
        self.handle.connection_status()
    }
}

// The areas' implementations, with today's server behind them.
#[cfg(not(feature = "idea-chat"))]
mod messages_impl;
#[cfg(not(feature = "idea-chat"))]
mod dms_impl;
#[cfg(not(feature = "idea-chat"))]
mod invites_impl;
#[cfg(not(feature = "idea-chat"))]
mod realtime;
#[cfg(not(feature = "idea-chat"))]
mod servers_impl;
#[cfg(not(feature = "idea-chat"))]
mod social_impl;
#[cfg(not(feature = "idea-chat"))]
mod account_impl;
#[cfg(not(feature = "idea-chat"))]
mod notif_impl;
#[cfg(not(feature = "idea-chat"))]
mod emoji_impl;
#[cfg(not(feature = "idea-chat"))]
mod stickers_impl;
#[cfg(not(feature = "idea-chat"))]
mod gif_impl;
#[cfg(not(feature = "idea-chat"))]
mod search_impl;
#[cfg(not(feature = "idea-chat"))]
mod members_impl;
#[cfg(not(feature = "idea-chat"))]
mod roles_impl;
#[cfg(not(feature = "idea-chat"))]
mod permissions_impl;
mod themes_impl;

// The SECOND backend's areas, talked to over the idea-chat wire.
#[cfg(feature = "idea-chat")]
mod idea_chat;

// --- the accessors (the single place the UI gets the backend) ---------------

/// Seed the idea-chat client's schema config (the open question in wire doc
/// §7: the `x-srv-schema` header is sent only when a value has been
/// configured).
#[cfg(feature = "idea-chat")]
pub fn install_idea_chat_config() {
    idea_chat::client::install_srv_schema();
}

/// Open the realtime stream: the signed session token authenticates the
/// connect (an empty token gets no working socket).
pub fn connect(token: String) -> Gateway {
    Backend::connect(token)
}

// --- the server and channel accessors ---------------------------------------

pub async fn list_custom_themes() -> Result<Vec<CustomTheme>, String> {
    CassetteBackend::list_custom_themes().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn save_custom_theme(name: String, base: String, colors: Vec<String>) -> Result<CustomTheme, String> {
    CassetteBackend::save_custom_theme(name, base, colors).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_custom_theme(from: String, to: String) -> Result<CustomTheme, String> {
    CassetteBackend::rename_custom_theme(from, to).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_custom_theme(name: String) -> Result<(), String> {
    CassetteBackend::delete_custom_theme(name).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_servers() -> Result<Vec<ServerSummary>, String> {
    Backend::list_servers().await.map_err(|e| crate::errors::friendly(&e))
}

// --- the invite accessors ---------------------------------------------------

pub async fn create_invite(
    server_id: i64,
    expires_at: Option<i64>,
    max_uses: Option<i64>,
) -> Result<Invite, String> {
    Backend::create_invite(server_id, expires_at, max_uses)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn redeem_invite(code: String) -> Result<RedeemResult, String> {
    Backend::redeem_invite(code).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_server(name: String) -> Result<ServerSummary, String> {
    Backend::create_server(name).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, String> {
    Backend::list_channels(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_channel(
    server_id: i64,
    name: String,
    kind: String,
    parent_channel_id: Option<i64>,
    private: bool,
) -> Result<ChannelResult, String> {
    Backend::create_channel(server_id, name, kind, parent_channel_id, private)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_channel(
    server_id: i64,
    channel_id: i64,
    new_name: String,
) -> Result<ChannelResult, String> {
    Backend::rename_channel(server_id, channel_id, new_name)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_channel(
    server_id: i64,
    channel_id: i64,
) -> Result<ChannelResult, String> {
    Backend::delete_channel(server_id, channel_id)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_server(
    server_id: i64,
    new_name: String,
) -> Result<ServerSettingsResult, String> {
    Backend::rename_server(server_id, new_name)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_server_icon(
    server_id: i64,
    icon: Option<String>,
) -> Result<ServerSettingsResult, String> {
    Backend::set_server_icon(server_id, icon)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn transfer_server_ownership(
    server_id: i64,
    new_owner_id: i64,
) -> Result<ServerSettingsResult, String> {
    Backend::transfer_server_ownership(server_id, new_owner_id)
        .await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn leave_server(server_id: i64) -> Result<(), String> {
    Backend::leave_server(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

// --- the message accessors --------------------------------------------------
// The message accessors keep the per-area wrappers' behaviour: a rejection
// becomes friendly text (`Result<_, String>`), so the UI shows the server's
// own message verbatim.
pub async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, String> {
    Backend::mark_read(channel_id, message_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, String> {
    Backend::unread_counts(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn pin_message(message_id: i64) -> Result<PinResult, String> {
    Backend::pin_message(message_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn unpin_message(message_id: i64) -> Result<PinResult, String> {
    Backend::unpin_message(message_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_pinned(channel_id: i64) -> Result<Vec<PinnedMessage>, String> {
    Backend::list_pinned(channel_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn pinned_count(channel_id: i64) -> Result<i64, String> {
    Backend::pinned_count(channel_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    Backend::add_reaction(message_id, emoji).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    Backend::remove_reaction(message_id, emoji).await.map_err(|e| crate::errors::friendly(&e))
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
            crate::auth::remember_avatar_decoration(p.username.as_str(), p.avatar_decoration.clone());
            crate::auth::remember_display_name(p.username.as_str(), p.display_name.clone());
        }
    }
}

pub async fn list_dms() -> Result<Vec<Dm>, String> {
    let list = Backend::list_dms().await.map_err(|e| crate::errors::friendly(&e))?;
    remember_dm_avatars(list.clone());
    Ok(list)
}

pub async fn start_dm(user_id: i64) -> Result<Dm, String> {
    let dm = Backend::start_dm(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    for p in dm.participants.iter() {
        crate::auth::remember_avatar(p.username.as_str(), p.avatar_url.clone());
        crate::auth::remember_avatar_decoration(p.username.as_str(), p.avatar_decoration.clone());
        crate::auth::remember_display_name(p.username.as_str(), p.display_name.clone());
    }
    Ok(dm)
}

/// Send into a DM and return the posted message. The backend pushes the
/// message as an event-stream frame to every participant (the sender
/// included), so the caller does NOT append the reply itself — the
/// [`incoming`] feed renders it.
pub async fn send_dm_message(channel_id: i64, content: String, sticker_id: Option<i64>) -> Result<DmMessage, String> {
    let result = Backend::send_dm_message(channel_id, content, sticker_id).await.map_err(|e| crate::errors::friendly(&e))?;
    Ok(result.message)
}

pub async fn list_dm_messages(channel_id: i64, limit: Option<i64>) -> Result<Vec<DmMessage>, String> {
    let history = Backend::list_dm_messages(channel_id, limit).await.map_err(|e| crate::errors::friendly(&e))?;
    for m in history.iter() {
        crate::auth::remember_avatar(m.author_username.as_str(), m.author_avatar.clone());
        crate::auth::remember_avatar_decoration(m.author_username.as_str(), m.author_decoration.clone());
    }
    Ok(history)
}

pub async fn create_group_dm(user_ids: Vec<i64>, name: String) -> Result<Dm, String> {
    Backend::create_group_dm(user_ids, name).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn add_group_dm_member(dm_id: i64, user_id: i64) -> Result<Dm, String> {
    Backend::add_group_dm_member(dm_id, user_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_group_dm_member(dm_id: i64, user_id: i64) -> Result<(), String> {
    Backend::remove_group_dm_member(dm_id, user_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_group_dm(dm_id: i64, name: String) -> Result<Dm, String> {
    Backend::rename_group_dm(dm_id, name).await.map_err(|e| crate::errors::friendly(&e))
}

// --- the friends and blocks accessors ---------------------------------------

pub async fn list_friends() -> Result<FriendList, String> {
    let list = Backend::list_friends().await.map_err(|e| crate::errors::friendly(&e))?;
    remember_friend_avatars(list.clone());
    Ok(list)
}

/// Pull every participant of a friends list into the shared avatar and
/// display-name registries so the friends rows draw uploaded images on a cold
/// load.
fn remember_friend_avatars(list: FriendList) {
    for group in
        list
            .incoming
            .iter()
            .chain(list.outgoing.iter())
            .chain(list.accepted.iter())
    {
        crate::auth::remember_avatar(group.username.as_str(), group.avatar_url.clone());
        crate::auth::remember_avatar_decoration(group.username.as_str(), group.avatar_decoration.clone());
        crate::auth::remember_display_name(group.username.as_str(), group.display_name.clone());
    }
}

/// Send a friend request, then answer with the actor's refreshed list — the
/// backend's `FriendChanged` frame only reaches the other user.
pub async fn send_friend_request(username: String) -> Result<FriendList, String> {
    Backend::send_friend_request(username).await.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn accept_friend_request(user_id: i64) -> Result<FriendList, String> {
    Backend::accept_friend_request(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn decline_or_cancel(user_id: i64) -> Result<FriendList, String> {
    Backend::decline_or_cancel(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn remove_friend(user_id: i64) -> Result<FriendList, String> {
    Backend::remove_friend(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn list_blocked() -> Result<Vec<BlockedUser>, String> {
    Backend::list_blocked().await.map_err(|e| crate::errors::friendly(&e))
}

/// Block (or, with `block` false, unblock) `user_id`, then answer with the
/// refreshed blocked list so the message list can re-filter.
pub async fn set_blocked(user_id: i64, block: bool) -> Result<Vec<BlockedUser>, String> {
    if block {
        Backend::block_user(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    } else {
        Backend::unblock_user(user_id).await.map_err(|e| crate::errors::friendly(&e))?;
    }
    Backend::list_blocked().await.map_err(|e| crate::errors::friendly(&e))
}

// --- the account accessors --------------------------------------------------

pub async fn login(username: String, password: String) -> Result<LoginResult, String> {
    Backend::login(username, password).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn signup(username: String, password: String) -> Result<SignupResult, String> {
    Backend::signup(username, password).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn logout() -> Result<LogoutResult, String> {
    Backend::logout().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn logout_everywhere() -> Result<LogoutResult, String> {
    Backend::logout_everywhere().await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn change_password(current_password: String, new_password: String) -> Result<ChangePasswordResult, String> {
    Backend::change_password(current_password, new_password).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn change_username(new_username: String) -> Result<ChangeUsernameResult, String> {
    Backend::change_username(new_username).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn reset_password(token: String, new_password: String) -> Result<String, String> {
    Backend::reset_password(token, new_password).await
        .map(|r| r.username)
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_avatar(avatar_url: Option<String>) -> Result<AvatarResult, String> {
    Backend::set_avatar(avatar_url).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn get_user_profile(user_id: i64, server_id: Option<i64>) -> Result<UserProfile, String> {
    Backend::get_user_profile(user_id, server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_profile(display_name: String, about: String) -> Result<ProfileFields, String> {
    Backend::set_profile(display_name, about).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_avatar_decoration(decoration: Option<String>) -> Result<Option<String>, String> {
    Backend::set_avatar_decoration(decoration).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_profile_effect(effect: Option<String>) -> Result<Option<String>, String> {
    Backend::set_profile_effect(effect).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn notification_settings(server_id: Option<i64>) -> Result<Vec<NotificationSetting>, String> {
    Backend::notification_settings(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_notification_setting(setting: NotificationSetting) -> Result<NotificationSetting, String> {
    Backend::set_notification_setting(setting).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_server_emoji(server_id: i64) -> Result<Vec<ServerEmoji>, String> {
    Backend::list_server_emoji(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn upload_emoji(server_id: i64, name: String, data_url: String) -> Result<ServerEmoji, String> {
    Backend::upload_emoji(server_id, name, data_url).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_emoji(emoji_id: i64, name: String) -> Result<ServerEmoji, String> {
    Backend::rename_emoji(emoji_id, name).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_emoji(emoji_id: i64) -> Result<(), String> {
    Backend::delete_emoji(emoji_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_stickers(server_id: i64) -> Result<Vec<Sticker>, String> {
    Backend::list_stickers(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn upload_sticker(
    server_id: i64,
    name: String,
    description: String,
    related_emoji: String,
    data_url: String,
) -> Result<Sticker, String> {
    Backend::upload_sticker(server_id, name, description, related_emoji, data_url).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn edit_sticker(
    sticker_id: i64,
    name: String,
    description: String,
    related_emoji: String,
) -> Result<Sticker, String> {
    Backend::edit_sticker(sticker_id, name, description, related_emoji).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_sticker(sticker_id: i64) -> Result<(), String> {
    Backend::delete_sticker(sticker_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn gif_search(query: String, offset: u32) -> Result<GifPage, String> {
    Backend::gif_search(query, offset).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn search_messages(server_id: i64, query: String, tz_offset_minutes: i64, offset: i64) -> Result<SearchResults, String> {
    Backend::search_messages(server_id, query, tz_offset_minutes, offset).await.map_err(|e| crate::errors::friendly(&e))
}

// --- members, roles and permissions -----------------------------------------

pub async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, String> {
    let members = Backend::list_members(server_id).await.map_err(|e| crate::errors::friendly(&e))?;
    // The roster is authoritative for avatars: register (or wipe) every row
    // so the member panels, message rows and DMs draw the real image.
    for m in members.iter() {
        crate::auth::remember_avatar(m.username.as_str(), m.avatar_url.clone());
        crate::auth::remember_avatar_decoration(m.username.as_str(), m.avatar_decoration.clone());
    }
    Ok(members)
}

pub async fn add_member(server_id: i64, user_id: i64) -> Result<MemberResult, String> {
    Backend::add_member(server_id, user_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_member(server_id: i64, user_id: i64) -> Result<MemberResult, String> {
    Backend::remove_member(server_id, user_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn ban_member(server_id: i64, user_id: i64, reason: Option<String>) -> Result<MemberResult, String> {
    Backend::ban_member(server_id, user_id, reason).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn timeout_member(server_id: i64, user_id: i64, duration_secs: i64) -> Result<MemberResult, String> {
    Backend::timeout_member(server_id, user_id, duration_secs).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn server_roles(server_id: i64) -> Result<RolesSnapshot, String> {
    Backend::server_roles(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_role(server_id: i64, name: String, color: String, mentionable: bool) -> Result<CustomRole, String> {
    Backend::create_role(server_id, name, color, mentionable).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn update_role(role_id: i64, name: String, color: String, mentionable: bool, permissions: Option<Permissions>) -> Result<CustomRole, String> {
    Backend::update_role(role_id, name, color, mentionable, permissions).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn move_role(role_id: i64, position: i64) -> Result<CustomRole, String> {
    Backend::move_role(role_id, position).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_role(role_id: i64) -> Result<(), String> {
    Backend::delete_role(role_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn assign_role(role_id: i64, user_id: i64, assigned: bool) -> Result<(), String> {
    Backend::assign_role(role_id, user_id, assigned).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn my_permissions(server_id: i64) -> Result<MyPermissions, String> {
    Backend::my_permissions(server_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn channel_overwrites(channel_id: i64) -> Result<Vec<Overwrite>, String> {
    Backend::channel_overwrites(channel_id).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_channel_overwrite(channel_id: i64, target: Target, allow: Permissions, deny: Permissions) -> Result<Vec<Overwrite>, String> {
    Backend::set_channel_overwrite(channel_id, target, allow, deny).await.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_everyone_permissions(server_id: i64, permissions: Permissions) -> Result<Permissions, String> {
    Backend::set_everyone_permissions(server_id, permissions).await.map_err(|e| crate::errors::friendly(&e))
}
