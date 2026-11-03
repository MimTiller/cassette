"""Q30g avatar/presence step — scratchpad notes.

BEFORE screens (Dark): member-list-before-dark.png, friends-before-dark.png taken.
MCP app: selector 'Discord Clone:web' pid 3610884 project_root /root/cassette-v100/discord-clone.
Login creds: polA88785 / polish-pass-1 (dev sidecar at :8085). Rail DM toggle test_id: rail-direct-messages.

Sites (all under discord-clone/src):
- components/avatar_chip.rs: shared chip, callers pass (size, radius) — the shape knob to centralize.
- app/member_panels.rs: live member rows; avatar_chip(..., 32.0, 16.0); presence dot 11px+2px ring rounded(6) at right:0,bottom:0; ring=BG_SIDEBAR; offline opacity 0.5 (MEMBER_OFFLINE_OPACITY) already there.
- app/user_panel.rs: own strip avatar 32px rounded(16), dot 11px ring=USER_BAR, status.color() from data.rs (SUCCESS/WARNING/DANGER/TEXT_FAINT).
- live_dm_view.rs: DM list rows: hand-rolled chip 34px rounded(10) x3 (image / group icon / initial) at :319-374; find-result rows avatar_chip(...,24.0,12.0) at :190. NO presence dots.
- friends_view.rs: friend_avatar_chip hand-rolled 34px rounded(10) image + avatar_chip(...,34.0,10.0) fallbacks; NO presence dots.
- app/user_popups.rs: profile card 80px chip in 88px frame; NO presence dot.
- group_dm.rs: avatar_chip(..., 28.0, 13.0) x2.
- settings_suite.rs: avatar_chip(..., 28.0, 14.0).
- app/message_view.rs: hand-rolled via styles::Avatar40() (radius 20 circle already).
- components/reply_reference_strip.rs: avatar_chip(...,16.0,8.0).
- app/composer.rs: @mention autocomplete rows = text only (no avatars).
- app/member_list.rs: members_switch -> live_member_panel.

Presence model: app.rs live_presence HashMap<i64,String> ('online'/'away'/'idle'/'dnd' + 'offline' on last frame); data.rs Status wire: online/idle/dnd/invisible. gateway_effects.rs folds Presence frames.

Plan:
1. theme.rs: PRESENCE_TEXT role + 'presence-text' token; add to ROLES (30->31) and all three palettes; update arrays sizes + test stays green.
2. avatar_chip.rs: drop `radius` param -> always size/2 (circle). Callers pass size only.
3. presence_chip.rs: shared helper presence_chip(dot_role, ring_role, over_dimmed) dot 10 + 2px ring at bottom-right (right/bottom = -2 => 10px ring gap, tangent).
4. Callers: dot in DM list (via live_presence + DM participants' user_ids + own CURRENT_USER_ID fallback), friends rows, profile card, member list, user panel. Keep group-icon chip square.
5. Verify: cargo check (native+wasm), cargo test, restart8085.sh, MCP screenshots dark+light, commit 2 (one per numbered item).

Scripts only for MCP gaps: viewport resize (CDP), theme seed (localStorage), login re-entry. Everything else MCP.
"""
