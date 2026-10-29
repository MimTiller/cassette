//! The rail's modal dialogs: join a server, create a server, and the live
//! channel create / rename / delete, invite and server
//! settings panels. Split out of `rail.rs`; reads the shared dialog state
//! through `ShellState`.

use super::*;

pub(super) struct ModalsParts {
    pub(super) join_el: Element,
    pub(super) create_el: Element,
    pub(super) cc_el: Element,
    pub(super) rc_el: Element,
    pub(super) dc_el: Element,
    pub(super) invite_el: Element,
    pub(super) ss_el: Element,
}

pub(super) fn build(s: &ShellState) -> ModalsParts {
    let ShellState { active_live_channel, active_live_server, cc_busy, cc_err, cc_kind, cc_name, cc_open, create_busy, create_error, create_name, create_open, dc_busy, dc_cid, dc_err, dc_name, dc_open, gateway, invite_created, invite_err, invite_expiry, invite_mode, join_busy, join_code, join_error, join_open, join_result, live_channels, live_kick_confirm, live_member_error, live_members, live_servers, live_unread, live_unread_rev, rc_busy, rc_cid, rc_err, rc_name, rc_open, ss_add_user_id, ss_busy, ss_err, ss_icon, ss_name, ss_open, ss_owner, .. } = s.clone();
    // The `#[component]` `name = expr` call form is only parsed inside `ui!`,
    // so the modal is wrapped in a `view` (the wrapper is a positioned
    // ancestor for the full-screen overlay, like the Profile/notif regions).
    let join_el = ui! {
        view {
            JoinServerModal(
                open = join_open,
                code = join_code,
                result = join_result,
                error = join_error,
                busy = join_busy,
                servers = live_servers,
            )
        }
    };

    // --- "Create a Server" modal ------------------------------------------
    // Trigger is the rail's green PLUS circle ("Add a Server"). `on_create`
    // mints a REAL backend server via the
    // `crate::servers::create_server` RPC (the backend seeds the owner's
    // membership row plus a default `#general` text channel, so the fresh
    // server is immediately usable), then re-fetches `live_servers` so the new
    // server appears in the rail's live section (the same refresh the Join
    // modal uses) and auto-enters it (selects it + pulls its channels, the
    // enter-live pattern below). `Signal` is `Copy`, so the captured `set`
    // stays `Fn`.
    let create_on_create: Rc<dyn Fn(String)> = Rc::new({
        let live_servers = live_servers.clone();
        let active_live_server = active_live_server.clone();
        let active_live_channel = active_live_channel.clone();
        let live_channels = live_channels.clone();
        let live_members = live_members.clone();
        let live_member_error = live_member_error.clone();
        let live_kick_confirm = live_kick_confirm.clone();
        let live_unread = live_unread.clone();
        let live_unread_rev = live_unread_rev.clone();
        let create_open = create_open;
        let create_error = create_error;
        let create_busy = create_busy;
        move |name: String| {
            let name_owned = name.clone();
            // Clone the `Rc` in the `Fn` body (not inside the `async move`):
            // the `async move` block would otherwise try to move the captured
            // `Rc` out of the `Fn` closure (E0507) — the enter-live handler
            // uses the same `let unread_buf = unread_buf.clone()` idiom.
            let buf = live_unread.clone();
            let c_err = create_error;
            let c_busy = create_busy;
            runtime_core::driver::spawn_async(async move {
                // Clear any prior rejection and mark in-flight; the modal's
                // Create button shows "Creating…" while this runs (the busy
                // guard also stops a fast double-tap from minting two).
                c_err.set(String::new());
                c_busy.set(true);
                match crate::servers::create_server(&name_owned).await {
                    Ok(summary) => {
                        // The backend persisted the server + default channel;
                        // re-fetch the live server list (the same RPC the rail's
                        // live section uses) so the new server appears without a
                        // reload.
                        if let Ok(list) = crate::servers::list_servers().await {
                            live_servers.set(list);
                        }
                        // Auto-enter the new server, mirroring the enter-live
                        // handler: select it, reset any stale channel/moderation
                        // state, and pull its channels + roster + unread counts.
                        active_live_server.set(Some(summary.id));
                        active_live_channel.set(None);
                        live_kick_confirm.set(None);
                        live_member_error.set(String::new());
                        live_members.set(Vec::new());
                        let sid = summary.id;
                        if let Ok(channels) = crate::servers::list_channels(sid).await {
                            // Auto-select the first text channel (same as the
                            // enter-live tile handler): the main panes key on
                            // `active_live_channel`, so a freshly created server
                            // would otherwise sit on a stale / mock main pane
                            // until a channel row is tapped.
                            let first = channels
                                .iter()
                                .find(|c| c.kind == "text")
                                .map(|c| c.id);
                            live_channels.set(channels);
                            if let Some(id) = first {
                                active_live_channel.set(Some(id));
                                // Persist the landing spot for this account so
                                // a refresh re-enters the just-created server's
                                // first channel.
                                crate::auth::save_last_view_channel(summary.id, id);
                                // See the enter-live tile handler's identical
                                // fix: auto-selecting doesn't fetch history on
                                // its own, so send it explicitly or the fresh
                                // server sits on an empty pane until a channel
                                // row is manually re-tapped.
                                gateway.send(
                                    discord_server::server_settings::WsClientMsg::History {
                                        channel_id: id,
                                        before_id: None,
                                    },
                                );
                            }
                        }
                        if let Ok(members) = crate::moderation::list_members(sid).await {
                            crate::mentions::members_loaded(sid, &members);
                            live_members.set(members);
                        }
                        if let Ok(counts) = crate::servers::unread_counts(sid).await {
                            let rev = live_unread_rev;
                            buf.borrow_mut()
                                .extend(counts.into_iter().map(|c| {
                                    (c.channel_id, (c.count, c.unread_mentions))
                                }));
                            rev.set(rev.get() + 1);
                        }
                        // Success: the server exists and we're auto-entering
                        // it, so close the modal. (On the error path below the
                        // modal stays open so the inline message is visible and
                        // the name field can be corrected and resubmitted.)
                        create_open.set(false);
                    }
                    Err(msg) => {
                        // Surface the backend's own rejection inline in the
                        // modal (e.g. an empty name -> "server name must not
                        // be empty"). The modal stays open; the name field is
                        // still editable so the user can correct it and hit
                        // Create again. We do NOT close here — closing on
                        // failure would hide the very message we just set.
                        eprintln!("create_server failed: {msg}");
                        c_err.set(msg);
                    }
                }
                c_busy.set(false);
            });
        }
    });
    // The create modal's secondary "Have an invite already? Join a Server"
    // link hands the Join flow to this caller: per-open resets (code, result,
    // error, busy) then open the join overlay.
    let create_on_join: Rc<dyn Fn()> = Rc::new({
        let join_code = join_code.clone();
        let join_result = join_result.clone();
        let join_error = join_error.clone();
        let join_busy = join_busy.clone();
        let join_open = join_open.clone();
        move || {
            join_code.set(String::new());
            join_result.set(None);
            join_error.set(String::new());
            join_busy.set(false);
            join_open.set(true);
        }
    });
    let create_modal_el: Element =
        create_server_modal(create_open, create_name, create_error, create_busy, create_on_create, create_on_join);
    let create_el = ui! {
        view {
            { create_modal_el }
        }
    };

    let cc_el = ui! {
        view {
            CreateChannelModal(
                open = cc_open,
                server_id = active_live_server,
                name = cc_name,
                kind = cc_kind,
                busy = cc_busy,
                error = cc_err,
                channels = live_channels,
            )
        }
    };
    let rc_el = ui! {
        view {
            RenameChannelModal(
                open = rc_open,
                server_id = active_live_server,
                channel_id = rc_cid,
                name = rc_name,
                busy = rc_busy,
                error = rc_err,
                channels = live_channels,
            )
        }
    };
    let dc_el = ui! {
        view {
            DeleteChannelModal(
                open = dc_open,
                server_id = active_live_server,
                channel_id = dc_cid,
                channel_name = dc_name,
                busy = dc_busy,
                error = dc_err,
                channels = live_channels,
            )
        }
    };
    let invite_el = ui! {
        view {
            InviteModal(
                open = invite_mode,
                server_id = active_live_server,
                expiry = invite_expiry,
                created = invite_created,
                created_err = invite_err,
            )
        }
    };
    let ss_el = ui! {
        view {
            ServerSettingsPanel(
                open = ss_open,
                server_id = active_live_server,
                name = ss_name,
                icon = ss_icon,
                new_owner_id = ss_owner,
                busy = ss_busy,
                error = ss_err,
                servers = live_servers,
                members = live_members,
                add_member_user_id = ss_add_user_id,
            )
        }
    };
    ModalsParts { join_el, create_el, cc_el, rc_el, dc_el, invite_el, ss_el }
}
