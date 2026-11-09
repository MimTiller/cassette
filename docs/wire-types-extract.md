# Wire types extracted from discord-server/src

Extracted verbatim from `/root/cassette-v100/discord-server/src`. Doc comments stripped; `#[derive(...)]` / `#[serde(...)]` attribute lines preserved exactly. No source files were modified.

## server_settings
### WsServerMsg
`src/server_settings/gateway.rs`
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WsServerMsg {
    Message {
        message_id: i64,
        channel_id: i64,
        #[serde(default)]
        server_id: i64,
        author_id: i64,
        author_username: String,
        #[serde(default)]
        author_avatar: Option<String>,
        #[serde(default)]
        author_decoration: Option<String>,
        content: String,
        created_at: i64,
        edited_at: Option<i64>,
        new: bool,
        #[serde(default)]
        mentioned_user_ids: Vec<i64>,
        #[serde(default)]
        reply_to_message_id: Option<i64>,
        #[serde(default)]
        reactions: Vec<(String, u32, bool)>,
        #[serde(default)]
        attachments: Vec<MessageAttachment>,
        #[serde(default)]
        embeds: Vec<crate::link_previews::Embed>,
        #[serde(default)]
        sticker: Option<StickerCopy>,
        #[serde(default)]
        seq: u64,
    },
    MessageEmbeds {
        message_id: i64,
        channel_id: i64,
        embeds: Vec<crate::link_previews::Embed>,
        #[serde(default)]
        seq: u64,
    },
    EmojiChanged {
        server_id: i64,
        #[serde(default)]
        seq: u64,
    },
    StickersChanged {
        server_id: i64,
        #[serde(default)]
        seq: u64,
    },
    History {
        channel_id: i64,
        has_more: bool,
        #[serde(default)]
        seq: u64,
    },
    DmMessage {
        message_id: i64,
        dm_id: i64,
        author_id: i64,
        author_username: String,
        #[serde(default)]
        author_avatar: Option<String>,
        #[serde(default)]
        author_decoration: Option<String>,
        content: String,
        created_at: i64,
        #[serde(default)]
        sticker: Option<StickerCopy>,
        #[serde(default)]
        seq: u64,
    },
    DmChanged {
        dm_id: i64,
        #[serde(default)]
        seq: u64,
    },
    Presence {
        user_id: i64,
        status: String,
        #[serde(default)]
        seq: u64,
    },
    StatusChoice {
        status: String,
        #[serde(default)]
        seq: u64,
    },
    Typing {
        channel_id: i64,
        user_id: i64,
        #[serde(default)]
        seq: u64,
    },
    MessageEdited {
        message_id: i64,
        channel_id: i64,
        content: String,
        edited_at: i64,
        #[serde(default)]
        mentioned_user_ids: Vec<i64>,
        #[serde(default)]
        seq: u64,
    },
    RolesChanged {
        server_id: i64,
        #[serde(default)]
        seq: u64,
    },
    ServerSettingsChanged {
        server_id: i64,
        #[serde(default)]
        seq: u64,
    },
    MessageDeleted {
        message_id: i64,
        channel_id: i64,
        #[serde(default)]
        seq: u64,
    },
    MemberChanged {
        server_id: i64,
        user_id: i64,
        joined: bool,
        #[serde(default)]
        seq: u64,
    },
    ProfileChanged {
        user_id: i64,
        username: String,
        display_name: Option<String>,
        #[serde(default)]
        avatar_decoration: Option<String>,
        #[serde(default)]
        seq: u64,
    },
    ReactionAdd {
        message_id: i64,
        channel_id: i64,
        user_id: i64,
        emoji: String,
        count: u32,
        #[serde(default)]
        seq: u64,
    },
    ReactionRemove {
        message_id: i64,
        channel_id: i64,
        user_id: i64,
        emoji: String,
        count: u32,
        #[serde(default)]
        seq: u64,
    },
    FriendChanged {
        friend_id: i64,
        action: String,
        #[serde(default)]
        seq: u64,
    },
    Error {
        reason: String,
        #[serde(default)]
        seq: u64,
    },
}
```

### WsClientMsg
`src/server_settings/gateway.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsClientMsg {
    Send {
        channel_id: i64,
        content: String,
        #[serde(default)]
        reply_to_message_id: Option<i64>,
        #[serde(default)]
        attachments: Vec<AttachmentInput>,
        #[serde(default)]
        sticker_id: Option<i64>,
    },
    History { channel_id: i64, before_id: Option<i64> },
    Typing { channel_id: i64 },
    EditMessage { message_id: i64, content: String },
    DeleteMessage { message_id: i64 },
    SetStatus { status: String },
}
```

### AttachmentInput
`src/server_settings/gateway.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub filename: String,
    pub mime: String,
    pub data: String,
}
```

### ChannelResult
`src/server_settings/channels.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChannelResult {
    pub channel_id: i64,
    pub server_id: i64,
    pub name: Option<String>,
}
```

### MemberInfo
`src/server_settings/members.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemberInfo {
    pub user_id: i64,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub role: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub avatar_decoration: Option<String>,
    #[serde(default)]
    pub presence: Option<String>,
}
```

### MemberResult
`src/server_settings/members.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MemberResult {
    pub server_id: i64,
    pub user_id: i64,
    pub role: String,
}
```

### ServerSettingsResult
`src/server_settings/mod.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ServerSettingsResult {
    pub server_id: i64,
    pub name: String,
    pub icon_url: Option<String>,
    pub owner_id: i64,
}
```

### PinResult
`src/server_settings/pins.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PinResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub pinned: bool,
    pub pinned_count: i64,
}
```

### PinnedMessage
`src/server_settings/pins.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PinnedMessage {
    pub message_id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    #[serde(default)]
    pub mentioned_user_ids: Vec<i64>,
    pub pinned_by: i64,
}
```

### ReactionResult
`src/server_settings/reactions.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReactionResult {
    pub message_id: i64,
    pub channel_id: i64,
    pub emoji: String,
    pub count: u32,
    pub added: bool,
}
```

### MarkReadResult
`src/server_settings/reads.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MarkReadResult {
    pub channel_id: i64,
    pub message_id: i64,
}
```

### UnreadCount
`src/server_settings/reads.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct UnreadCount {
    pub channel_id: i64,
    pub count: i64,
    #[serde(default)]
    pub unread_mentions: i64,
}
```

## servers
### ServerSummary
`src/servers.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerSummary {
    pub id: i64,
    pub name: String,
    pub icon_url: Option<String>,
}
```

### ChannelSummary
`src/servers.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSummary {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub position: i64,
    pub parent_channel_id: Option<i64>,
    #[serde(default)]
    pub private: bool,
}
```

## dms

*(no wire types are defined in this module; dms.rs uses types defined in other modules)*

## invites
### Invite
`src/invites.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Invite {
    pub id: i64,
    pub server_id: i64,
    pub code: String,
    pub created_by: i64,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub max_uses: Option<i64>,
    pub uses: i64,
}
```

### RedeemResult
`src/invites.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RedeemResult {
    pub server_id: i64,
    pub user_id: i64,
    pub already_member: bool,
    pub role: String,
}
```

## auth
### LoginResult
`src/auth.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LoginResult {
    pub user_id: i64,
    pub token: String,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub blocked_ids: Vec<i64>,
}
```

### SignupResult
`src/auth.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SignupResult {
    pub user_id: i64,
    pub username: String,
}
```

### LogoutResult
`src/auth.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LogoutResult {}
```

### ChangePasswordResult
`src/auth.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChangePasswordResult {}
```

### ChangeUsernameResult
`src/auth.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChangeUsernameResult {
    pub username: String,
}
```

### AvatarResult
`src/auth.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AvatarResult {
    pub user_id: i64,
    #[serde(default)]
    pub avatar_url: Option<String>,
}
```

## profiles
### ProfileFields
`src/profiles.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileFields {
    pub display_name: Option<String>,
    pub about: String,
}
```

### UserProfile
`src/profiles.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserProfile {
    pub user_id: i64,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub about: String,
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub avatar_decoration: Option<String>,
    #[serde(default)]
    pub profile_effect: Option<String>,
    pub created_at: i64,
    pub server_role: Option<String>,
    pub joined_server_at: Option<i64>,
    pub mutual_servers: Vec<(i64, String)>,
    pub relationship: String,
    pub blocked: bool,
}
```

## friends
### FriendList
`src/friends.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FriendList {
    pub incoming: Vec<FriendUser>,
    pub outgoing: Vec<FriendUser>,
    pub accepted: Vec<FriendUser>,
}
```

### FriendUser
`src/friends.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FriendUser {
    pub user_id: i64,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub avatar_decoration: Option<String>,
}
```

### SendFriendRequestResult
`src/friends.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SendFriendRequestResult {
    pub user_id: i64,
    pub status: String,
}
```

## blocks
### BlockedUser
`src/blocks.rs`
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct BlockedUser {
    pub user_id: i64,
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}
```

## search
### SearchResults
`src/search.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SearchResults {
    pub total: i64,
    pub hits: Vec<SearchHit>,
    pub query: Query,
}
```

## notification_settings
### NotificationSetting
`src/notification_settings.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationSetting {
    pub scope: Scope,
    pub scope_id: i64,
    pub level: Option<Level>,
    pub muted_until: Option<i64>,
}
```

### Scope
`src/notification_settings.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Server,
    Channel,
}
```

### Level
`src/notification_settings.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Level {
    All,
    Mentions,
    Nothing,
}
```

## server_emoji
### ServerEmoji
`src/server_emoji.rs`
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEmoji {
    pub id: i64,
    pub server_id: i64,
    pub name: String,
    pub image: String,
}
```

## stickers
### Sticker
`src/stickers.rs`
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sticker {
    pub id: i64,
    pub server_id: i64,
    pub name: String,
    pub description: String,
    pub related_emoji: String,
    pub image: String,
}
```

## gifs
### GifPage
`src/gifs.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GifPage {
    pub gifs: Vec<Gif>,
    pub next_offset: Option<u32>,
}
```

## server_roles
### RolesSnapshot
`src/server_roles.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RolesSnapshot {
    pub server_id: i64,
    pub roles: Vec<CustomRole>,
    pub assignments: Vec<(i64, i64)>,
    #[serde(default)]
    pub everyone_permissions: crate::permissions::Permissions,
}
```

### CustomRole
`src/server_roles.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomRole {
    pub id: i64,
    pub server_id: i64,
    pub name: String,
    pub color: String,
    pub mentionable: bool,
    pub position: i64,
    #[serde(default)]
    pub permissions: crate::permissions::Permissions,
}
```

## permissions
### Permissions
`src/permissions.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Permissions(pub u64);
```

### Target
`src/permissions.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Everyone,
    Role(i64),
    Member(i64),
}
```

### MyPermissions
`src/permissions.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MyPermissions {
    pub server_id: i64,
    pub server: Permissions,
    pub channels: Vec<(i64, Permissions)>,
    pub rank: Rank,
}
```

### Overwrite
`src/permissions.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overwrite {
    pub target: Target,
    pub allow: Permissions,
    pub deny: Permissions,
}
```

## password_resets
### ResetPasswordResult
`src/password_resets.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResetPasswordResult {
    pub username: String,
}
```


**Total types: 42 · total LOC extracted: 470**
