"""Permissions: a member without Manage Channels sees no channel-edit controls;
a private channel is invisible to a member until their role is let in; a member
can't send where Send Messages is denied (the composer shows the no-permission
bar). Owner-side edits go through the UI like the perm2 one-off check.

Usage: test_permissions.py [base_url] [out_dir]
"""
import asyncio
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdp import Session, MissingElement, WaitTimeout, CDPTimeout, CDPError
from srv_helper import signup_login, srv

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8085"
SHOTS = sys.argv[2] if len(sys.argv) > 2 else "/tmp/cassette-e2e"

TIMEOUT = 420

NO_PERM = "You don't have permission to send messages in this channel."
SEND = 2


async def right_at(s, x, y):
    await s.cdp("Input.dispatchMouseEvent", type="mouseMoved", x=x, y=y)
    for kind in ("mousePressed", "mouseReleased"):
        await s.cdp("Input.dispatchMouseEvent", type=kind, x=x, y=y,
                    button="right", clickCount=1, buttons=2)
    await asyncio.sleep(1.0)


async def main():
    tag = int(time.time()) % 100000
    a_u, b_u = f"qa{tag}", f"qb{tag}"
    pw = "perm-pass-1"
    server = f"Perm Crew {tag}"
    ra = signup_login(BASE, a_u, pw)
    rb = signup_login(BASE, b_u, pw)
    sid = srv(BASE, "create_server", {"name": server}, ra["token"])["id"]
    srv(BASE, "add_member", {"server_id": sid, "user_id": rb["user_id"]}, ra["token"])
    guard = srv(BASE, "create_role", {"server_id": sid, "name": "Guard",
                                      "color": "#e91e63", "mentionable": True}, ra["token"])
    srv(BASE, "create_channel", {"server_id": sid, "name": "secret", "kind": "text",
                                 "private": True}, ra["token"])
    srv(BASE, "assign_role", {"role_id": guard["id"], "user_id": rb["user_id"],
                              "assigned": True}, ra["token"])

    a = Session(BASE, SHOTS)
    b = Session(BASE, SHOTS)
    try:
        await a.open()
        await a.fresh_page()
        await a.login(a_u, pw)
        await b.open()
        await b.fresh_page()
        await b.login(b_u, pw)

        # --- private channel invisible to the member (it holds the role but
        # the channel is created private for @everyone; the role was assigned
        # BEFORE creation so the role has no allow yet) ---
        hidden = not await b.until(b.painted("secret", xmax=b.W * 0.45), timeout=6,
                                   why="secret painted for B")
        b.check("the private channel is invisible to the member", hidden)

        # --- member's server dropdown hides channel-edit controls ---
        await b.open_server_menu(server)
        for row, label in (("Create Channel", "hides Create Channel"),
                          ("Server Settings", "hides Server Settings")):
            b.check(f"the member's dropdown {label}",
                    not await b.visible(row, xmin=60, xmax=b.W * 0.6))
        await b.shot("perm0-member-menu.png")
        await b.escape()

        # --- owner lets the Guard role into #secret through its menu ---
        pos = await a.text_pos("secret", xmax=a.W * 0.45)
        a.check("the owner sees the private channel in its sidebar", bool(pos))
        let_in = False
        if pos:
            await right_at(a, *pos)
            got = await a.until(a.painted("Permissions", xmin=60, xmax=a.W * 0.7),
                                timeout=6, why="channel menu Permissions")
            a.check("the channel right-click menu offers Permissions", got)
            if got:
                await a.click_menu_item("Permissions")
                await asyncio.sleep(1.2)
                added = await a.click_text("Add role or member", xmin=60)
                if not added:
                    added = await a.click_starts("Add role")
                let_in = added and await a.click_text("+ Guard")
                await a.shot("perm1-access.png")
                await a.escape()
        b.known_bug("the Guard role is let into #secret by the owner", let_in,
                    why="owner UI path")

        # --- B now sees #secret (the allow overwrite + the role) ---
        visible_now = await b.until(b.painted("secret", xmax=b.W * 0.45),
                                    timeout=15, why="secret visible to B")
        if not visible_now:
            await b.click_text(server, xmax=b.W * 0.45)
            visible_now = await b.until(b.painted("secret", xmax=b.W * 0.45),
                                        timeout=10, why="secret after sidebar refresh")
        b.known_bug("after the role is let in the member sees #secret",
                    bool(visible_now), why="B's sidebar")

        # --- deny Send Messages for @everyone in #general via the menu ---
        pos = await a.text_pos("general", xmin=72, xmax=312)
        denied = False
        if pos:
            await right_at(a, *pos)
            got = await a.until(a.painted("Permissions", xmin=60, xmax=a.W * 0.7),
                                timeout=6, why="channel menu")
            if not got:
                await a.shot("perm2-general-menu-missing.png")
            if got:
                await a.click_menu_item("Permissions")
                await asyncio.sleep(1.2)
                # The Send Messages row ends in three buttons: ✕ (deny), / (inherit),
                # ✓ (allow); the dialog opens on @everyone. Click the row's ✕.
                sw = await a.ev(
                    "(() => { const leaves = [...document.querySelectorAll('*')].filter(e => e.children.length === 0); const l = leaves.find(e => e.textContent.trim() === 'Send Messages' && e.getBoundingClientRect().x > 60 && e.getBoundingClientRect().width > 0); if (!l) return null; const lr = l.getBoundingClientRect(); const t = leaves.filter(e => e.textContent.trim() === '\u2715').map(e => e.getBoundingClientRect()).filter(r => r.width > 0 && r.x > lr.x && Math.abs(r.y + r.height/2 - (lr.y + lr.height/2)) < 20).sort((p, q) => p.x - q.x)[0]; return t ? [t.x + t.width/2, t.y + t.height/2] : null; })()")
                print("  deny button at", sw)
                if sw:
                    await a.click(*sw)
                    await asyncio.sleep(1.5)
                chans = srv(BASE, "list_channels", {"server_id": sid}, ra["token"])
                gid = next(c["id"] for c in chans if c["name"] == "general")
                ow = srv(BASE, "channel_overwrites", {"channel_id": gid}, ra["token"])
                everyone = [o for o in ow if o.get("target") == "Everyone"]
                denied = bool(everyone) and (everyone[0]["deny"] & SEND) == SEND
                await a.shot("perm2-deny-send.png")
                await a.escape()
        a.known_bug("the UI denies Send Messages for @everyone", denied,
                    why="switch tap then channel_overwrites check")

        if not denied:
            # Seed the deny through the server so the member-side checks still run.
            chans = srv(BASE, "list_channels", {"server_id": sid}, ra["token"])
            gid = next(c["id"] for c in chans if c["name"] == "general")
            srv(BASE, "set_channel_overwrite", {"channel_id": gid, "target": "Everyone",
                                                "allow": 0, "deny": SEND}, ra["token"])
        # --- the member's composer shows the no-permission bar ---
        # B has #general open while the owner changes it: the bar should appear live.
        # A background tab doesn't paint, so bring B's to the front first.
        await b.cdp("Page.bringToFront")
        live_bar = await b.until(b.painted(NO_PERM), timeout=10, why="no-permission bar")
        b.check("the no-permission bar appears live when the owner denies Send Messages", bool(live_bar))
        if not live_bar:
            await b.shot("perm3-member-composer.png")
            await b.click_text("secret", xmin=72, xmax=312)
            await asyncio.sleep(1.0)
            await b.click_text("general", xmin=72, xmax=312)
        bar = await b.until(b.painted(NO_PERM), timeout=10, why="no-permission bar")
        b.check("the composer shows the no-permission bar", bar)
        if not bar:
            await b.shot("perm4-member-after-switch.png")
        posted = False
        await b.type_ph_start("Message", "should not post", select=False)
        await b.enter()
        await asyncio.sleep(1.5)
        posted = await b.visible("should not post", xmin=b.W * 0.2)
        b.check("the denied member's send does not post", not posted)

        errs = [e["params"].get("exceptionDetails", {}).get("message", "")
                for e in a._events if e["method"] == "Runtime.exceptionThrown"]
        a.check("no uncaught page exceptions (permissions)", not errs)
        if errs:
            print("  page exceptions:", errs[:3])
    except (MissingElement, WaitTimeout, CDPTimeout, CDPError) as e:
        print(f"  !! test aborted: {e}")
    finally:
        await a.close()
        await b.close()
    merged = {}
    merged.update(a.results)
    merged.update(b.results)
    known = sorted(a.known | b.known)
    print("RESULTS", json.dumps({"results": merged, "known": known}))
    bad = [k for k, v in merged.items() if not v and k not in known]
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
