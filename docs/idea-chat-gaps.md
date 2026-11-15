# crate::backend → idea-chat mapping (Phase 2)

Reference: `~/idea-chat` master `1504241`; the app pins the vocab crates at
`6437901` — `chat-model`, `chat-api`, `chat-server` and `idea-state` are
byte-identical between the two (only `docs/e2ee-protocol.md` grew), and `crates/hearth`
is commented out of the build ("Idea Chat replaces it"), so the chat-api surface
is what ships. Verdicts are against the **source**, not the wire study; where the
study's table (§4) disagrees, the source wins and the fix is noted inline.

Legend: **impl** = wired in `src/backend/idea_chat/` · **state** = served from the
reducer snapshot (no RPC needed) · **refuse** = answers the shared "not supported
on this server" sentence · **missing** = idea-chat has no route/frame for it
(question for Nicho, or client-side work for later).

## Accounts / session — AccountApi

| method | idea-chat route | notes |
|---|---|---|
| login | `sign_in` | email+password; RWND's username rides the email slot |
| signup | `sign_up` | email+password+handle+display; the verification-code half is WIP server-side (Q: no "send me the code" RPC for a client that just asked) |
| logout | `sign_out` | |
| logout_everywhere | `sign_out_everywhere` | |
| change_password | `change_password` | answers a fresh `Session` |
| change_username | `set_handle` | **wire-doc fix**: the study (§4, §7-Q4) said RWND-only; pinned rev's auth.rs has `set_handle` — doc corrected |
| reset_password | `reset_password` | RWND's opaque token rides the code arg |
| set_avatar | `set_avatar` + upload triplet | client sniffs inline bytes (`emoji_upload::sniff_mime`) and PUTs the presigned slot |
| get_user_profile | — **state** | no profile-read RPC for arbitrary users; assembled from the reducer's visible `User` rows |
| set_profile | `update_profile` | display name + bio |
| set_avatar_decoration | — **missing** | no decoration anywhere in the model |
| set_profile_effect | — **missing** | ditto |

## Spaces as servers / channels — ServersApi + InvitesApi

| method | route | notes |
|---|---|---|
| list_servers | — **state** | no list RPC; `Snapshot`/`SpaceUpserted` carry them in `space_order` |
| create_server | `create_space` | |
| list_channels | — **state** | `State.channels` filtered by `Channel.space`, in position order |
| create_channel | `create_channel` | RWND's `private` flag dropped: visibility here is by role (`view`/`post`) |
| rename_channel | `rename_channel` | |
| delete_channel | `delete_channel` | |
| rename_server | `rename_space` | |
| set_server_icon | `set_space_icon` | upload triplet |
| transfer_server_ownership | `transfer_space` | |
| leave_server | `leave_space` | |
| create_invite | `create_invite` | RWND's epoch-seconds expiry becomes whole hours from now |
| redeem_invite | `invite_preview` + `join_space` | the preview-then-join pair |
| (idea-chat-only) | `order_channels`, `create_category`/`rename_category`/`delete_category`, `set_archived`, `create_thread` | no RWND screen asks for these yet |

## Messages — MessagesApi

| method | route | notes |
|---|---|---|
| (list/page) | `history` + `message` | paging via `Window` (Latest/Before/After/Around), not limit+offset; live.rs `request_history` feeds `Message { new: false }` frames |
| send | `send` | gateway `Send` moved to the API; attachments via the triplet; sticker sends refuse |
| edit | `edit` | the fresh body lands back as `MessageEdited` |
| delete | `delete` | |
| add_reaction / remove_reaction | `react` (add= bool) | the fresh total rides `ReactionChanged` |
| unread_counts | — **state** (client-side because E2EE) | the reducer's `reads` map vs each channel's `last_message`; no unread RPC in chat-api — **wire-doc fix**: the study called this RWND-only; the state supports it faithfully (0/1 + mention tally, not an exact row count) |
| mark_read | `mark_read` | `Read` event folds with no frame |
| pin_message / unpin_message / list_pinned / pinned_count | — **missing** | no pinned flag on a message (Q6) |

## DMs — DmsApi

| method | route | notes |
|---|---|---|
| list_dms | — **state** | `Direct`/`Group` channels with no space |
| start_dm | `open_direct` | |
| send_dm_message | `send` | a DM is a channel |
| list_dm_messages | `history` | |
| create_group_dm | `create_group` | |
| add_group_dm_member | `add_to_group` | |
| remove_group_dm_member | `leave_group` | only the caller's own removal — no verb drops an arbitrary member from someone else's group |
| rename_group_dm | `update_channel` | name; topic/view/post restated unchanged |
| (idea-chat-only) | `close_conversation` | |

## Members / presence / roster — MembersApi + EventStreamApi

| method | route | notes |
|---|---|---|
| list_members (live roster) | — **state** | `Space.members` × `User` rows × presence map; `MemberJoined`/`MemberLeft`/`SpaceUpserted` events keep it fresh; presence doubles as socket lifecycle (first socket online, last close offline) |
| add_member | — **missing** (as a direct verb) | joining is invite redemption, which joins the caller, not a named person; the invite flow covers the user journey honestly |
| remove_member | `remove_member` | |
| ban_member / timeout_member | — **missing** | no moderation state (Q6) |
| presence | `set_status` (API) + `PresenceChanged` (live) | chat-model `Presence` is Online/Idle/Busy/Offline — **wire-doc fix**: the study said no DND/Invisible; pinned rev has `Busy` = do-not-disturb, so dnd rides it; invisible rides offline (policy note in live.rs) |
| typing dots | `Up::Action Typing` + `Event::Typing` | clean |
| connect (live) | `live_ticket` + `/_srv/_ws/live` | reducer fold, duplicate/gap handling in live.rs |

## Social — SocialApi

| method | route | notes |
|---|---|---|
| list_friends / list_blocked | — **state** | `State.relations`; four relation states split into RWND's three lists |
| send_friend_request | `add_friend` (by handle) | mutual→Friend, one-way→Outgoing |
| accept_friend_request | `accept_friend` | |
| decline_or_cancel / remove_friend | `remove_relation` | one verb for all three |
| block_user | `block` | |
| unblock_user | — **missing** | no verb clears a `Blocked` relation |

## Everything else — no counterpart on idea-chat today

NotificationsApi (muted exists as `set_muted`/`MutedChanged` but not per-setting),
EmojiApi, StickersApi, GifApi, RolesApi (chat-model has the three fixed ranks
Owner/Admin/Member — `set_role` exists but the named-role system has no model),
PermissionsApi, ThemesApi, CallApi (voice: chat-model speaks `VoiceJoined/
VoiceUpdated/VoiceLeft`, `RingUpdated`, `KeyRequested`/`KeyDelivered` — E2EE
call keys; the app's call provider has nothing to bridge to yet, and the old
provider is out; join_call/leave_call/update_call exist on chat-api now —
**wire-doc fix**: the study predated them), InstanceApi (chat-api has no
instance-status/claim route — that was hearth, now out of the build;
**missing**), SearchApi → `search(scope, query)` **impl** (client-side search
because E2EE: the server cannot read ciphertext, so RWND's query parsing moves
client-side and the raw text rides the RPC).

## Open questions for Nicho (carried from §7, current against master)

1. Roles: three fixed ranks vs RWND's named-role bitsets — grow or degrade?
2. Signup's verification code: how does a client that just signed up get its code?
3. E2EE devices/`EnrollToken`/`KeyDelivered`: is the call-key machinery for RWND?
4. Pins, bans/timeouts, notification settings, custom emoji, GIFs, channel
   permission overwrites, themes, avatar decorations: deliberately out of scope,
   or Phase-2 additions?
5. `x-srv-schema`: how does a foreign client obtain the hash? Omitting is what
   we do today (header sent only when configured).
6. Unblock: no verb clears `Blocked` — is a block a relation with no removal path?
7. Instance claim/status: hearth had it, chat-api doesn't — does the shipping
   app have an instance-admin surface at all?
