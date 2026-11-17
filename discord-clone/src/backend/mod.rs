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
pub use discord_server::instance::{
    CallTestResult,
    ChatTestResult,
    ClaimAdminResult,
    InstanceInfo,
    InstanceSettingsResult,
};
pub use discord_server::profiles::{ProfileFields, UserProfile};
pub use discord_server::notification_settings::NotificationSetting;
/// Notification rows the settings screens draw: RWND's own vocabulary, so
/// only the Cassette build reads them.
pub use discord_server::notification_settings::{
    Level,
    Scope,
    MUTED_UNTIL_TURNED_OFF,
    is_muted,
};
pub use discord_server::server_emoji::ServerEmoji;
pub use discord_server::server_emoji::MAX_PER_SERVER;
pub use discord_server::stickers::{Sticker, MAX_PER_SERVER as STICKERS_MAX_PER_SERVER};
pub use discord_server::gifs::GifPage;
pub use discord_server::gifs::{gif_body, gif_message};
pub use discord_server::search::SearchResults;
pub use discord_server::server_settings::{MemberInfo, MemberResult};
pub use discord_server::server_roles::{RolesSnapshot, CustomRole};
pub use discord_server::permissions::{Permissions, Target, MyPermissions, Overwrite};
pub use discord_server::voice::{CallTicket, LeaveVoiceResult, VoiceStateResult};
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

/// Voice calls: joining a voice channel asks the backend for the server's
/// provider-agnostic call ticket, leaving clears the roster entry, and the
/// state query says which provider is enabled (and re-issues the ticket
/// when the caller is already in the channel, so a fresh browser restores
/// its voice bar without a new click). The UI treats a ticket as opaque
/// provider data and never talks to a provider directly.
pub trait CallApi {
    async fn join_voice(channel_id: i64) -> Result<CallTicket, ServerError>;
    async fn leave_voice(channel_id: i64) -> Result<LeaveVoiceResult, ServerError>;
    async fn voice_state(channel_id: i64) -> Result<VoiceStateResult, ServerError>;
    /// Mute and deafen the caller's own connection (the server owns the
    /// flags: a browser's own toggles are invisible to peers, so every client
    /// must see the same icons). Refuses unless the caller is connected to
    /// this channel.
    async fn set_voice_flags(
        channel_id: i64,
        muted: bool,
        deafened: bool,
        camera: bool,
        screen_share: bool,
    ) -> Result<(), ServerError>;
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

/// The instance store: what the host's admin area configures without touching
/// env files. `instance_info` is public (nothing secret, so a client can ask
/// before signing in — including whether the CALLING user is an instance
/// admin, which gates the settings gear); reads/writes/tests of the store
/// itself are admin-only server-side and answer the plain refusal otherwise.
pub trait InstanceApi {
    async fn instance_info() -> Result<InstanceInfo, ServerError>;
    async fn get_instance_settings() -> Result<InstanceSettingsResult, ServerError>;
    async fn set_instance_settings(
        call_provider: String,
        chat_backend: String,
        idea_chat_url: String,
    ) -> Result<InstanceSettingsResult, ServerError>;
    async fn test_call_provider(
        call_provider: String,
    ) -> Result<CallTestResult, ServerError>;
    async fn claim_instance_admin() -> Result<ClaimAdminResult, ServerError>;
    async fn test_chat_backend(idea_chat_url: String) -> Result<ChatTestResult, ServerError>;
}

/// Today's backend: the implementation files below implement the areas
/// above for this type.
pub struct CassetteBackend;

/// The SECOND implementation of every area trait above, talked to over the
/// idea-chat wire (see `docs/idea-chat-wire.md`). Selected at build time
/// below by the `idea-chat` feature; RWND-only areas answer
/// `Err("not supported on this server")`.
pub struct IdeaChatBackend;

/// THE ONE SWITCH POINT is now runtime: both backends compile into every
/// build, and each accessor below runs its call against the
/// implementation picked at app start from `instance_info.chat_backend`
/// (swapped live when an instance admin saves a new chat backend).
/// The storage namespace and key for the persisted chat-backend pick —
/// a startup config, like the session identity entries in `crate::auth`.
const CHAT_BACKEND_NS: &str = "discord-clone:chat";
const CHAT_BACKEND_KEY: &str = "chat_backend";

static CHAT_IDEA_CHAT: std::sync::OnceLock<std::sync::atomic::AtomicBool> = std::sync::OnceLock::new();

/// Pick the chat backend every accessor below runs against: `true` speaks
/// the idea-chat server, `false` this RWND server. Startup config like
/// the server base URL, so a process-wide flag, not reactive app state.
/// Re-apply the persisted startup choice: the flag must exist BEFORE
/// `connect` runs at app start, so the flavor comes from the storage
/// entry the last `choose_chat_backend` wrote — a browser reload onto a
/// server whose backend just switched picks the right socket with no
/// server round-trip. A save's choice lands here; the saved-but-not-yet-
/// applied switch shows through the instance-info seed at login.
pub fn install_chat_backend_choice() {
    let store = storage::platform_storage(CHAT_BACKEND_NS);
    if let Some(name) = store.get_now(CHAT_BACKEND_KEY).ok().flatten() {
        choose_chat_backend(name == "idea-chat".to_string())
    }
}

/// Persist the startup choice beside the flag so the next page load sees it.
pub fn choose_chat_backend(idea_chat: bool) {
    CHAT_IDEA_CHAT
        .get_or_init(
            || std::sync::atomic::AtomicBool::new(false)
        )
        .store(idea_chat, std::sync::atomic::Ordering::SeqCst);
    let store = storage::platform_storage(CHAT_BACKEND_NS);
    let name = if idea_chat { "idea-chat".to_string() } else { "rwnd".to_string() };
    let _ = store.set_now(&CHAT_BACKEND_KEY, &name);
}

/// The active choice, read by every accessor below.
pub fn chat_idea_chat() -> bool {
    CHAT_IDEA_CHAT
        .get_or_init(
            || std::sync::atomic::AtomicBool::new(false)
        )
        .load(std::sync::atomic::Ordering::SeqCst)
}

// --- the capability report --------------------------------------------------

pub(crate) mod capabilities_impl;

/// The search placeholder label per search source (on-device says so).
pub(crate) use capabilities_impl::search_label;
/// The sentence a voice row shows when no call provider is enabled.
pub(crate) use capabilities_impl::VOICE_NOT_ENABLED;
/// The call provider the voice rows compare against.
pub(crate) use capabilities_impl::VoiceProvider;
/// The cached report for UI sites (reads the report signal under the hood,
/// so a capability change re-renders the sites that read it).
pub(crate) use capabilities_impl::capabilities;

/// The event-stream handle: what the UI holds after [`connect`]. Cheap to
/// clone. Its verbs ask the backend for a history page, a message send /
/// edit / delete, the typing indicator, or a status pick; [`incoming`]
/// yields every frame the backend pushes and [`connection_status`] reports
/// the connection. The UI only calls these — it never builds a frame or
/// connects itself.
/// The event-stream handle the UI holds: either flavor, picked at app
/// start from `instance_info.chat_backend` (see `connect`). Its verbs
/// dispatch on the flavor; the UI never sees which.
#[derive(Clone, Copy)]
pub enum Gateway {
    Cassette(server::UseSocket<WsServerMsg, WsClientMsg>),
    IdeaChat(crate::backend::idea_chat::live::GatewayHandle),
}

impl Gateway {
    pub fn request_history(self: Gateway, channel_id: i64, before_id: Option<i64>) {
        match self {
            Gateway::Cassette(handle) => { handle.send(WsClientMsg::History { channel_id, before_id }); }
            Gateway::IdeaChat(handle) => { handle.request_history(channel_id, before_id); }
        }
    }
    pub fn send_message(
        self: Gateway,
        channel_id: i64,
        content: String,
        reply_to_message_id: Option<i64>,
        attachments: Vec<AttachmentInput>,
        sticker_id: Option<i64>,
    ) {
        match self {
            Gateway::Cassette(handle) => {
                handle.send(WsClientMsg::Send {
                channel_id,
                content,
                reply_to_message_id,
                attachments,
                sticker_id,
            });
            }
            Gateway::IdeaChat(handle) => handle.send_message(channel_id, content, reply_to_message_id, attachments, sticker_id),
        }
    }
    pub fn edit_message(self: Gateway, message_id: i64, content: String) {
        match self {
            Gateway::Cassette(handle) => {
                handle.send(WsClientMsg::EditMessage { message_id, content });
            }
            Gateway::IdeaChat(handle) => handle.edit_message(message_id, content),
        }
    }
    pub fn delete_message(self: Gateway, message_id: i64) {
        match self {
            Gateway::Cassette(handle) => { handle.send(WsClientMsg::DeleteMessage { message_id }); },
            Gateway::IdeaChat(handle) => { handle.delete_message(message_id); },
        }
    }
    pub fn announce_typing(self: Gateway, channel_id: i64) {
        match self {
            Gateway::Cassette(handle) => { handle.send(WsClientMsg::Typing { channel_id }); },
            Gateway::IdeaChat(handle) => { handle.announce_typing(channel_id); },
        }
    }
    pub fn set_status(self: Gateway, status: String) {
        match self {
            Gateway::Cassette(handle) => { handle.send(WsClientMsg::SetStatus { status }); },
            Gateway::IdeaChat(handle) => { handle.set_status(status); },
        }
    }
    pub fn incoming(self: Gateway) -> Signal<Option<WsServerMsg>> {
        match self {
            Gateway::Cassette(handle) => handle.incoming(),
            Gateway::IdeaChat(handle) => handle.incoming(),
        }
    }
    pub fn connection_status(self: Gateway) -> SocketStatus {
        match self {
            Gateway::Cassette(handle) => handle.status(),
            Gateway::IdeaChat(handle) => handle.connection_status(),
        }
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
mod stickers_impl;
mod gif_impl;
mod search_impl;
mod members_impl;
mod roles_impl;
mod permissions_impl;
mod voice_impl;
mod calls;
mod call_media;
mod video_feeds;
mod themes_impl;
mod instance_impl;

// The SECOND backend's areas, talked to over the idea-chat wire.
mod idea_chat;

// --- the accessors (the single place the UI gets the backend) ---------------

/// Seed the idea-chat client's schema config (the open question in wire doc
/// §7: the `x-srv-schema` header is sent only when a value has been
/// configured).
pub fn install_idea_chat_config() {
    idea_chat::client::install_srv_schema();
}

/// Open the realtime stream: the signed session token authenticates the
/// connect (an empty token gets no working socket).
pub fn connect(token: String) -> Gateway {
    if chat_idea_chat() {
            IdeaChatBackend::connect(token)
        } else {
            CassetteBackend::connect(token)
        }
}

// --- the server and channel accessors ---------------------------------------

pub async fn list_custom_themes() -> Result<Vec<CustomTheme>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::list_custom_themes().await
        } else {
            CassetteBackend::list_custom_themes().await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn save_custom_theme(name: String, base: String, colors: Vec<String>) -> Result<CustomTheme, String> {
    if chat_idea_chat() {
            IdeaChatBackend::save_custom_theme(name, base, colors).await
        } else {
            CassetteBackend::save_custom_theme(name, base, colors).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_custom_theme(from: String, to: String) -> Result<CustomTheme, String> {
    if chat_idea_chat() {
            IdeaChatBackend::rename_custom_theme(from, to).await
        } else {
            CassetteBackend::rename_custom_theme(from, to).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_custom_theme(name: String) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::delete_custom_theme(name).await
        } else {
            CassetteBackend::delete_custom_theme(name).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_servers() -> Result<Vec<ServerSummary>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::list_servers().await
        } else {
            CassetteBackend::list_servers().await
        }.map_err(|e| crate::errors::friendly(&e))
}

// --- the invite accessors ---------------------------------------------------

pub async fn create_invite(
    server_id: i64,
    expires_at: Option<i64>,
    max_uses: Option<i64>,
) -> Result<Invite, String> {
    if chat_idea_chat() {
            IdeaChatBackend::create_invite(server_id, expires_at, max_uses).await
        } else {
            CassetteBackend::create_invite(server_id, expires_at, max_uses).await
        }
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn redeem_invite(code: String) -> Result<RedeemResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::redeem_invite(code).await
        } else {
            CassetteBackend::redeem_invite(code).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_server(name: String) -> Result<ServerSummary, String> {
    if chat_idea_chat() {
            IdeaChatBackend::create_server(name).await
        } else {
            CassetteBackend::create_server(name).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_channels(server_id: i64) -> Result<Vec<ChannelSummary>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::list_channels(server_id).await
        } else {
            CassetteBackend::list_channels(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_channel(
    server_id: i64,
    name: String,
    kind: String,
    parent_channel_id: Option<i64>,
    private: bool,
) -> Result<ChannelResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::create_channel(server_id, name, kind, parent_channel_id, private).await
        } else {
            CassetteBackend::create_channel(server_id, name, kind, parent_channel_id, private).await
        }
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_channel(
    server_id: i64,
    channel_id: i64,
    new_name: String,
) -> Result<ChannelResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::rename_channel(server_id, channel_id, new_name).await
        } else {
            CassetteBackend::rename_channel(server_id, channel_id, new_name).await
        }
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_channel(
    server_id: i64,
    channel_id: i64,
) -> Result<ChannelResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::delete_channel(server_id, channel_id).await
        } else {
            CassetteBackend::delete_channel(server_id, channel_id).await
        }
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_server(
    server_id: i64,
    new_name: String,
) -> Result<ServerSettingsResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::rename_server(server_id, new_name).await
        } else {
            CassetteBackend::rename_server(server_id, new_name).await
        }
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_server_icon(
    server_id: i64,
    icon: Option<String>,
) -> Result<ServerSettingsResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_server_icon(server_id, icon).await
        } else {
            CassetteBackend::set_server_icon(server_id, icon).await
        }
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn transfer_server_ownership(
    server_id: i64,
    new_owner_id: i64,
) -> Result<ServerSettingsResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::transfer_server_ownership(server_id, new_owner_id).await
        } else {
            CassetteBackend::transfer_server_ownership(server_id, new_owner_id).await
        }
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn leave_server(server_id: i64) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::leave_server(server_id).await
        } else {
            CassetteBackend::leave_server(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

// --- the message accessors --------------------------------------------------
// The message accessors keep the per-area wrappers' behaviour: a rejection
// becomes friendly text (`Result<_, String>`), so the UI shows the server's
// own message verbatim.
pub async fn mark_read(channel_id: i64, message_id: i64) -> Result<MarkReadResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::mark_read(channel_id, message_id).await
        } else {
            CassetteBackend::mark_read(channel_id, message_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn unread_counts(server_id: i64) -> Result<Vec<UnreadCount>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::unread_counts(server_id).await
        } else {
            CassetteBackend::unread_counts(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn pin_message(message_id: i64) -> Result<PinResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::pin_message(message_id).await
        } else {
            CassetteBackend::pin_message(message_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn unpin_message(message_id: i64) -> Result<PinResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::unpin_message(message_id).await
        } else {
            CassetteBackend::unpin_message(message_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_pinned(channel_id: i64) -> Result<Vec<PinnedMessage>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::list_pinned(channel_id).await
        } else {
            CassetteBackend::list_pinned(channel_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn pinned_count(channel_id: i64) -> Result<i64, String> {
    if chat_idea_chat() {
            IdeaChatBackend::pinned_count(channel_id).await
        } else {
            CassetteBackend::pinned_count(channel_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn add_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::add_reaction(message_id, emoji).await
        } else {
            CassetteBackend::add_reaction(message_id, emoji).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_reaction(message_id: i64, emoji: String) -> Result<ReactionResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::remove_reaction(message_id, emoji).await
        } else {
            CassetteBackend::remove_reaction(message_id, emoji).await
        }.map_err(|e| crate::errors::friendly(&e))
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
    let list = if chat_idea_chat() {
            IdeaChatBackend::list_dms().await
        } else {
            CassetteBackend::list_dms().await
        }.map_err(|e| crate::errors::friendly(&e))?;
    remember_dm_avatars(list.clone());
    Ok(list)
}

pub async fn start_dm(user_id: i64) -> Result<Dm, String> {
    let dm = if chat_idea_chat() {
            IdeaChatBackend::start_dm(user_id).await
        } else {
            CassetteBackend::start_dm(user_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
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
    let result = if chat_idea_chat() {
            IdeaChatBackend::send_dm_message(channel_id, content, sticker_id).await
        } else {
            CassetteBackend::send_dm_message(channel_id, content, sticker_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    Ok(result.message)
}

pub async fn list_dm_messages(channel_id: i64, limit: Option<i64>) -> Result<Vec<DmMessage>, String> {
    let history = if chat_idea_chat() {
            IdeaChatBackend::list_dm_messages(channel_id, limit).await
        } else {
            CassetteBackend::list_dm_messages(channel_id, limit).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    for m in history.iter() {
        crate::auth::remember_avatar(m.author_username.as_str(), m.author_avatar.clone());
        crate::auth::remember_avatar_decoration(m.author_username.as_str(), m.author_decoration.clone());
    }
    Ok(history)
}

pub async fn create_group_dm(user_ids: Vec<i64>, name: String) -> Result<Dm, String> {
    if chat_idea_chat() {
            IdeaChatBackend::create_group_dm(user_ids, name).await
        } else {
            CassetteBackend::create_group_dm(user_ids, name).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn add_group_dm_member(dm_id: i64, user_id: i64) -> Result<Dm, String> {
    if chat_idea_chat() {
            IdeaChatBackend::add_group_dm_member(dm_id, user_id).await
        } else {
            CassetteBackend::add_group_dm_member(dm_id, user_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_group_dm_member(dm_id: i64, user_id: i64) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::remove_group_dm_member(dm_id, user_id).await
        } else {
            CassetteBackend::remove_group_dm_member(dm_id, user_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_group_dm(dm_id: i64, name: String) -> Result<Dm, String> {
    if chat_idea_chat() {
            IdeaChatBackend::rename_group_dm(dm_id, name).await
        } else {
            CassetteBackend::rename_group_dm(dm_id, name).await
        }.map_err(|e| crate::errors::friendly(&e))
}

// --- the friends and blocks accessors ---------------------------------------

pub async fn list_friends() -> Result<FriendList, String> {
    let list = if chat_idea_chat() {
            IdeaChatBackend::list_friends().await
        } else {
            CassetteBackend::list_friends().await
        }.map_err(|e| crate::errors::friendly(&e))?;
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
    if chat_idea_chat() {
            IdeaChatBackend::send_friend_request(username).await
        } else {
            CassetteBackend::send_friend_request(username).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn accept_friend_request(user_id: i64) -> Result<FriendList, String> {
    if chat_idea_chat() {
            IdeaChatBackend::accept_friend_request(user_id).await
        } else {
            CassetteBackend::accept_friend_request(user_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn decline_or_cancel(user_id: i64) -> Result<FriendList, String> {
    if chat_idea_chat() {
            IdeaChatBackend::decline_or_cancel(user_id).await
        } else {
            CassetteBackend::decline_or_cancel(user_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn remove_friend(user_id: i64) -> Result<FriendList, String> {
    if chat_idea_chat() {
            IdeaChatBackend::remove_friend(user_id).await
        } else {
            CassetteBackend::remove_friend(user_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    list_friends().await
}

pub async fn list_blocked() -> Result<Vec<BlockedUser>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::list_blocked().await
        } else {
            CassetteBackend::list_blocked().await
        }.map_err(|e| crate::errors::friendly(&e))
}

/// Block (or, with `block` false, unblock) `user_id`, then answer with the
/// refreshed blocked list so the message list can re-filter.
pub async fn set_blocked(user_id: i64, block: bool) -> Result<Vec<BlockedUser>, String> {
    if block {
        if chat_idea_chat() {
            IdeaChatBackend::block_user(user_id).await
        } else {
            CassetteBackend::block_user(user_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    } else {
        if chat_idea_chat() {
            IdeaChatBackend::unblock_user(user_id).await
        } else {
            CassetteBackend::unblock_user(user_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    }
    if chat_idea_chat() {
            IdeaChatBackend::list_blocked().await
        } else {
            CassetteBackend::list_blocked().await
        }.map_err(|e| crate::errors::friendly(&e))
}

// --- the account accessors --------------------------------------------------

pub async fn login(username: String, password: String) -> Result<LoginResult, String> {
    let result = if chat_idea_chat() {
            IdeaChatBackend::login(username, password).await
        } else {
            CassetteBackend::login(username, password).await
        }.map_err(|e| crate::errors::friendly(&e));
    if result.is_ok() {
        capabilities_impl::refresh();
    }
    result
}

pub async fn signup(username: String, password: String) -> Result<SignupResult, String> {
    let result = if chat_idea_chat() {
            IdeaChatBackend::signup(username, password).await
        } else {
            CassetteBackend::signup(username, password).await
        }.map_err(|e| crate::errors::friendly(&e));
    if result.is_ok() {
        capabilities_impl::refresh();
    }
    result
}

// The email-code half of the auth surface: only the idea-chat server has
// one, so these accessors reach the idea_chat entry points directly and the
// built-in screens never call them.

/// Create an account with an email, password, handle and display name;
/// answers the email the verification code was sent to.
pub async fn signup_email(email: String, password: String, handle: String, display_name: String) -> Result<String, String> {
    idea_chat::impl_account::signup_email(email, password, handle, display_name).await
        .map_err(|e| crate::errors::friendly(&e))
}

/// Sign in with an email and password. Answers `true` when the account is
/// signed in, `false` when the server said the email is unverified (a fresh
/// code went to it) and the form must move to the emailed-code step.
pub async fn sign_in_email(email: String, password: String) -> Result<bool, String> {
    idea_chat::impl_account::sign_in_email(email, password).await
        .map(move |answer| match answer {
            idea_chat::impl_account::SignInAnswer::SignedIn(_) => true,
            idea_chat::impl_account::SignInAnswer::VerifyEmail(_) => false,
        })
        .map_err(|e| crate::errors::friendly(&e))
}

/// Confirm the emailed code; on success the response's `session` cookie
/// signs the browser in, so the caller lands as after a normal login.
pub async fn verify_email(email: String, code: String) -> Result<LoginResult, String> {
    idea_chat::impl_account::verify_email(email, code).await
        .map_err(|e| crate::errors::friendly(&e))
}

/// Send another verification code. The server is silent for unknown or
/// already-verified addresses; success only says the call went through.
pub async fn resend_code(email: String) -> Result<(), String> {
    idea_chat::impl_account::resend_code(email).await
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn logout() -> Result<LogoutResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::logout().await
        } else {
            CassetteBackend::logout().await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn logout_everywhere() -> Result<LogoutResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::logout_everywhere().await
        } else {
            CassetteBackend::logout_everywhere().await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn change_password(current_password: String, new_password: String) -> Result<ChangePasswordResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::change_password(current_password, new_password).await
        } else {
            CassetteBackend::change_password(current_password, new_password).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn change_username(new_username: String) -> Result<ChangeUsernameResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::change_username(new_username).await
        } else {
            CassetteBackend::change_username(new_username).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn reset_password(token: String, new_password: String) -> Result<String, String> {
    if chat_idea_chat() {
            IdeaChatBackend::reset_password(token, new_password).await
        } else {
            CassetteBackend::reset_password(token, new_password).await
        }
        .map(|r| r.username)
        .map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_avatar(avatar_url: Option<String>) -> Result<AvatarResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_avatar(avatar_url).await
        } else {
            CassetteBackend::set_avatar(avatar_url).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn get_user_profile(user_id: i64, server_id: Option<i64>) -> Result<UserProfile, String> {
    if chat_idea_chat() {
            IdeaChatBackend::get_user_profile(user_id, server_id).await
        } else {
            CassetteBackend::get_user_profile(user_id, server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_profile(display_name: String, about: String) -> Result<ProfileFields, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_profile(display_name, about).await
        } else {
            CassetteBackend::set_profile(display_name, about).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_avatar_decoration(decoration: Option<String>) -> Result<Option<String>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_avatar_decoration(decoration).await
        } else {
            CassetteBackend::set_avatar_decoration(decoration).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_profile_effect(effect: Option<String>) -> Result<Option<String>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_profile_effect(effect).await
        } else {
            CassetteBackend::set_profile_effect(effect).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn notification_settings(server_id: Option<i64>) -> Result<Vec<NotificationSetting>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::notification_settings(server_id).await
        } else {
            CassetteBackend::notification_settings(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_notification_setting(setting: NotificationSetting) -> Result<NotificationSetting, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_notification_setting(setting).await
        } else {
            CassetteBackend::set_notification_setting(setting).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_server_emoji(server_id: i64) -> Result<Vec<ServerEmoji>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::list_server_emoji(server_id).await
        } else {
            CassetteBackend::list_server_emoji(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn upload_emoji(server_id: i64, name: String, data_url: String) -> Result<ServerEmoji, String> {
    if chat_idea_chat() {
            IdeaChatBackend::upload_emoji(server_id, name, data_url).await
        } else {
            CassetteBackend::upload_emoji(server_id, name, data_url).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn rename_emoji(emoji_id: i64, name: String) -> Result<ServerEmoji, String> {
    if chat_idea_chat() {
            IdeaChatBackend::rename_emoji(emoji_id, name).await
        } else {
            CassetteBackend::rename_emoji(emoji_id, name).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_emoji(emoji_id: i64) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::delete_emoji(emoji_id).await
        } else {
            CassetteBackend::delete_emoji(emoji_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn list_stickers(server_id: i64) -> Result<Vec<Sticker>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::list_stickers(server_id).await
        } else {
            CassetteBackend::list_stickers(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn upload_sticker(
    server_id: i64,
    name: String,
    description: String,
    related_emoji: String,
    data_url: String,
) -> Result<Sticker, String> {
    if chat_idea_chat() {
            IdeaChatBackend::upload_sticker(server_id, name, description, related_emoji, data_url).await
        } else {
            CassetteBackend::upload_sticker(server_id, name, description, related_emoji, data_url).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn edit_sticker(
    sticker_id: i64,
    name: String,
    description: String,
    related_emoji: String,
) -> Result<Sticker, String> {
    if chat_idea_chat() {
            IdeaChatBackend::edit_sticker(sticker_id, name, description, related_emoji).await
        } else {
            CassetteBackend::edit_sticker(sticker_id, name, description, related_emoji).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_sticker(sticker_id: i64) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::delete_sticker(sticker_id).await
        } else {
            CassetteBackend::delete_sticker(sticker_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn gif_search(query: String, offset: u32) -> Result<GifPage, String> {
    if chat_idea_chat() {
            IdeaChatBackend::gif_search(query, offset).await
        } else {
            CassetteBackend::gif_search(query, offset).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn search_messages(server_id: i64, query: String, tz_offset_minutes: i64, offset: i64) -> Result<SearchResults, String> {
    if chat_idea_chat() {
            IdeaChatBackend::search_messages(server_id, query, tz_offset_minutes, offset).await
        } else {
            CassetteBackend::search_messages(server_id, query, tz_offset_minutes, offset).await
        }.map_err(|e| crate::errors::friendly(&e))
}

// --- members, roles and permissions -----------------------------------------

pub async fn list_members(server_id: i64) -> Result<Vec<MemberInfo>, String> {
    let members = if chat_idea_chat() {
            IdeaChatBackend::list_members(server_id).await
        } else {
            CassetteBackend::list_members(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))?;
    // The roster is authoritative for avatars: register (or wipe) every row
    // so the member panels, message rows and DMs draw the real image.
    for m in members.iter() {
        crate::auth::remember_avatar(m.username.as_str(), m.avatar_url.clone());
        crate::auth::remember_avatar_decoration(m.username.as_str(), m.avatar_decoration.clone());
    }
    Ok(members)
}

pub async fn add_member(server_id: i64, user_id: i64) -> Result<MemberResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::add_member(server_id, user_id).await
        } else {
            CassetteBackend::add_member(server_id, user_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn remove_member(server_id: i64, user_id: i64) -> Result<MemberResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::remove_member(server_id, user_id).await
        } else {
            CassetteBackend::remove_member(server_id, user_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn ban_member(server_id: i64, user_id: i64, reason: Option<String>) -> Result<MemberResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::ban_member(server_id, user_id, reason).await
        } else {
            CassetteBackend::ban_member(server_id, user_id, reason).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn timeout_member(server_id: i64, user_id: i64, duration_secs: i64) -> Result<MemberResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::timeout_member(server_id, user_id, duration_secs).await
        } else {
            CassetteBackend::timeout_member(server_id, user_id, duration_secs).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn server_roles(server_id: i64) -> Result<RolesSnapshot, String> {
    if chat_idea_chat() {
            IdeaChatBackend::server_roles(server_id).await
        } else {
            CassetteBackend::server_roles(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn create_role(server_id: i64, name: String, color: String, mentionable: bool) -> Result<CustomRole, String> {
    if chat_idea_chat() {
            IdeaChatBackend::create_role(server_id, name, color, mentionable).await
        } else {
            CassetteBackend::create_role(server_id, name, color, mentionable).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn update_role(role_id: i64, name: String, color: String, mentionable: bool, permissions: Option<Permissions>) -> Result<CustomRole, String> {
    if chat_idea_chat() {
            IdeaChatBackend::update_role(role_id, name, color, mentionable, permissions).await
        } else {
            CassetteBackend::update_role(role_id, name, color, mentionable, permissions).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn move_role(role_id: i64, position: i64) -> Result<CustomRole, String> {
    if chat_idea_chat() {
            IdeaChatBackend::move_role(role_id, position).await
        } else {
            CassetteBackend::move_role(role_id, position).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn delete_role(role_id: i64) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::delete_role(role_id).await
        } else {
            CassetteBackend::delete_role(role_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

thread_local! {
    /// Who the relay last reported speaking in the joined voice channel.
    static SPEAKING: std::cell::RefCell<Vec<i64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Ask the call who is speaking in `channel_id` now; true when that changed
/// since the last ask (the caller repaints the voice rows then).
pub async fn refresh_speaking(channel_id: i64) -> bool {
    let mut now = call_media::speaking_ids(channel_id).await;
    now.sort_unstable();
    SPEAKING.with(|s| {
        let mut s = s.borrow_mut();
        if *s == now {
            false
        } else {
            *s = now;
            true
        }
    })
}

/// Forget who was speaking (after leaving the call); true when that changed.
pub fn clear_speaking() -> bool {
    SPEAKING.with(|s| {
        let mut s = s.borrow_mut();
        let had = !s.is_empty();
        s.clear();
        had
    })
}

/// Whether `user_id` was speaking at the last ask.
pub fn is_speaking(user_id: i64) -> bool {
    SPEAKING.with(|s| s.borrow().contains(&user_id))
}

/// Refresh the call's videos for `channel_id`; true when the tiles must repaint.
pub async fn refresh_video_feeds(channel_id: i64) -> bool {
    call_media::refresh_videos(channel_id).await
}

/// The live video for `user_id` in `channel_id`, if this tab has one.
pub fn video_feed(channel_id: i64, user_id: i64) -> Option<media_stream::MediaStream> {
    video_feeds::feed(channel_id, user_id)
}

/// How many <video> elements on the page are painting (robot hook).
pub fn playing_videos() -> u32 {
    video_feeds::playing_videos()
}

/// Whether the browser is still holding the call's audio back.
pub async fn call_audio_blocked(channel_id: i64) -> bool {
    call_media::audio_blocked(channel_id).await
}

/// The call's audio elements as JSON, for the robot hook.
pub fn call_audio_state(channel_id: i64) -> String {
    call_media::audio_state(channel_id)
}

/// Whether this tab is still sharing its screen in a voice channel (the
/// browser's own "Stop sharing" bar can end a share without the app).
pub async fn screen_share_active(channel_id: i64) -> bool {
    call_media::is_screen_sharing(channel_id).await
}

pub async fn assign_role(role_id: i64, user_id: i64, assigned: bool) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::assign_role(role_id, user_id, assigned).await
        } else {
            CassetteBackend::assign_role(role_id, user_id, assigned).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn my_permissions(server_id: i64) -> Result<MyPermissions, String> {
    if chat_idea_chat() {
            IdeaChatBackend::my_permissions(server_id).await
        } else {
            CassetteBackend::my_permissions(server_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn channel_overwrites(channel_id: i64) -> Result<Vec<Overwrite>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::channel_overwrites(channel_id).await
        } else {
            CassetteBackend::channel_overwrites(channel_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_channel_overwrite(channel_id: i64, target: Target, allow: Permissions, deny: Permissions) -> Result<Vec<Overwrite>, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_channel_overwrite(channel_id, target, allow, deny).await
        } else {
            CassetteBackend::set_channel_overwrite(channel_id, target, allow, deny).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_everyone_permissions(server_id: i64, permissions: Permissions) -> Result<Permissions, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_everyone_permissions(server_id, permissions).await
        } else {
            CassetteBackend::set_everyone_permissions(server_id, permissions).await
        }.map_err(|e| crate::errors::friendly(&e))
}

// --- the call accessors ------------------------------------------------------

pub async fn join_voice(channel_id: i64) -> Result<CallTicket, String> {
    if chat_idea_chat() {
            IdeaChatBackend::join_voice(channel_id).await
        } else {
            CassetteBackend::join_voice(channel_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn leave_voice(channel_id: i64) -> Result<LeaveVoiceResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::leave_voice(channel_id).await
        } else {
            CassetteBackend::leave_voice(channel_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn voice_state(channel_id: i64) -> Result<VoiceStateResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::voice_state(channel_id).await
        } else {
            CassetteBackend::voice_state(channel_id).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_voice_flags(
    channel_id: i64,
    muted: bool,
    deafened: bool,
    camera: bool,
    screen_share: bool,
) -> Result<(), String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_voice_flags(channel_id, muted, deafened, camera, screen_share).await
        } else {
            CassetteBackend::set_voice_flags(channel_id, muted, deafened, camera, screen_share).await
        }.map_err(|e| crate::errors::friendly(&e))
}

// --- per-person local volume (LOCAL ONLY, no server call) -------------------
// These never touch the trait: a remote's volume and a local mute are this
// tab's own playback state inside its live call session, so there is nothing
// to ask the backend. Keyed by RWND user id; the idea-chat call maps it to
// the relay numbers that user holds through the roster.

pub fn set_user_volume(channel_id: i64, user_id: i64, position: f32) -> Result<(), String> {
    call_media::set_user_volume(channel_id, user_id, position)
}

pub fn set_user_mute(channel_id: i64, user_id: i64, on: bool) -> Result<(), String> {
    call_media::set_user_mute(channel_id, user_id, on)
}

pub fn get_user_volume(channel_id: i64, user_id: i64) -> f32 {
    call_media::get_user_volume(channel_id, user_id)
}

pub fn get_user_muted(channel_id: i64, user_id: i64) -> bool {
    call_media::get_user_muted(channel_id, user_id)
}

/// This tab's whole local volume table, for the robot hook.
pub fn call_volume_state() -> String {
    call_media::volume_state()
}

// --- the instance-store accessors -------------------------------------------

pub async fn instance_info() -> Result<InstanceInfo, String> {
    if chat_idea_chat() {
            IdeaChatBackend::instance_info().await
        } else {
            CassetteBackend::instance_info().await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn get_instance_settings() -> Result<InstanceSettingsResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::get_instance_settings().await
        } else {
            CassetteBackend::get_instance_settings().await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn set_instance_settings(
    call_provider: String,
    chat_backend: String,
    idea_chat_url: String,
) -> Result<InstanceSettingsResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::set_instance_settings(call_provider, chat_backend, idea_chat_url).await
        } else {
            CassetteBackend::set_instance_settings(call_provider, chat_backend, idea_chat_url).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn test_call_provider(
    call_provider: String,
) -> Result<CallTestResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::test_call_provider(call_provider).await
        } else {
            CassetteBackend::test_call_provider(call_provider).await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn claim_instance_admin() -> Result<ClaimAdminResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::claim_instance_admin().await
        } else {
            CassetteBackend::claim_instance_admin().await
        }.map_err(|e| crate::errors::friendly(&e))
}

pub async fn test_chat_backend(idea_chat_url: String) -> Result<ChatTestResult, String> {
    if chat_idea_chat() {
            IdeaChatBackend::test_chat_backend(idea_chat_url).await
        } else {
            CassetteBackend::test_chat_backend(idea_chat_url).await
        }.map_err(|e| crate::errors::friendly(&e))
}