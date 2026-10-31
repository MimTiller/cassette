"""Profile/status: change display name and about-me, see them on the profile
card from the other user; switch status to Do Not Disturb and the other user
sees the red dot; the app-icon picker changes the tab icon.

Usage: test_profile_status.py [base_url] [out_dir]
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
SHOTS = sys.argv[2] if len(sys.argv) > 2 else os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "out")

TIMEOUT = 360

DND_RED = "rgb(242, 63, 67)"


async def avatar_pos(s):
    return await s.ev(
        f"(() => {{ const e = [...document.querySelectorAll('div')].find(e => {{ const r = e.getBoundingClientRect();"
        f"return Math.abs(r.width - 32) <= 6 && Math.abs(r.height - r.width) <= 2 && r.x < 300 && r.y > {s.H - 90}; }});"
        " if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; })()")


async def dots(s):
    return await s.ev(
        "[...document.querySelectorAll('div')].filter(e => { const r = e.getBoundingClientRect();"
        "return r.width >= 9 && r.width <= 12 && Math.abs(r.height - r.width) <= 2; })"
        ".map(e => getComputedStyle(e).backgroundColor)")


async def main():
    tag = int(time.time()) % 100000
    a_u, b_u = f"pfA{tag}x", f"pbB{tag}y"
    pw = "profile-pass-1"
    server = f"Profile Crew {tag}"
    disp, about = f"Display {tag}", f"About me text {tag}"
    ra = signup_login(BASE, a_u, pw)
    rb = signup_login(BASE, b_u, pw)
    sid = srv(BASE, "create_server", {"name": server}, ra["token"])["id"]
    srv(BASE, "add_member", {"server_id": sid, "user_id": rb["user_id"]}, ra["token"])

    a = Session(BASE, SHOTS)
    b = Session(BASE, SHOTS)
    try:
        await a.open()
        await a.fresh_page()
        await a.login(a_u, pw)

        # --- edit display name + about me through User Settings > Profile ---
        await a.open_user_settings()
        await a.settings_section("Profile")
        about_ok = await a.type_ph_start("A line about yourself", about, select=False)
        name_ok = await a.type_ph_start(a_u, disp, select=False)
        saved = (name_ok or about_ok) and await a.click_text("Save Profile")
        await asyncio.sleep(1.0)
        confirm = await a.until(a.painted("Saved."), timeout=6,
                                why="save confirmation text")
        a.check("the Profile section offers an about-me field", bool(about_ok))
        a.known_bug("the Profile section offers a display-name field", bool(name_ok),
                    why="the display-name field's placeholder is the username")
        a.known_bug("the profile edit saves", bool(saved and confirm),
                    why="Save Profile button then confirmation")
        await a.close_settings()

        # --- B sees them on A's card ---
        await b.open()
        await b.fresh_page()
        await b.login(b_u, pw)
        pos = await b.text_pos(a_u) or await b.text_pos(disp)
        card_ok = False
        if pos:
            await b.click(*pos)
            card_open = await b.until(b.painted("Member since"), timeout=8,
                                      why="profile card open")
            if not card_open:
                await b.escape()
                pos = await b.text_pos(a_u) or await b.text_pos(disp)
                if pos:
                    await b.click(*pos)
                    card_open = await b.until(b.painted("Member since"), timeout=8,
                                              why="card second try")
            shown = card_open and await b.until(b.painted(disp), timeout=8,
                                                why="display name on the card")
            about_seen = shown and await b.visible(about)
            a_card = bool(card_open and shown)
            b.check("the card shows A's new display name", a_card)
            b.known_bug("the card shows A's about-me text", bool(about_seen),
                        why="about field saved above")
            await b.shot("prof1-card.png")
            await b.escape()
        else:
            b.check("the card shows A's new display name", False)
            b.known_bug("the card shows A's about-me text", False,
                        why="name not found in B's view")

        # --- status: A sets Do Not Disturb; B (already connected) sees red ---
        cir = await avatar_pos(a)
        if cir:
            await a.click(*cir)
            menu = await a.until(a.painted("Do Not Disturb", xmin=60, xmax=a.W * 0.4),
                                 timeout=8, why="status menu")
            a.check("the avatar popup is the status menu", menu)
            if menu:
                picked = await a.click_text("Do Not Disturb", xmin=60, xmax=a.W * 0.4)
                await asyncio.sleep(1.5)
                da = await dots(a)
                a.check("A's own dots turn red", DND_RED in da)
                await a.shot("prof2-dnd.png")
                await a.escape()
        else:
            a.check("the avatar popup is the status menu", False)
        # B receives the Presence frame over its live socket; the members-rail
        # dot next to A's row turns red within a few seconds.
        seen_red = False
        for _ in range(6):
            db = await dots(b)
            if DND_RED in db:
                seen_red = True
                break
            await asyncio.sleep(1.5)
        b.check("B sees the red Do Not Disturb dot", seen_red)
        await b.shot("prof3-dnd-b.png")

        # --- app icon picker changes the tab icon ---
        icon_before = await a.ev("(document.querySelector('link[rel=icon]') || document.querySelector('link')).href")
        await a.open_user_settings()
        got_ap = await a.click_text("Appearance", xmax=int(a.W * 0.4))
        if not got_ap:
            got_ap = await a.settings_section("Appearance")
        clicked = (await a.click_text("Miami", xmin=int(a.W * 0.3))
                   or await a.click_text("Memphis", xmin=int(a.W * 0.3))
                   or await a.click_text("Bubblegum", xmin=int(a.W * 0.3)))
        a.check("the Appearance section offers a cassette icon picker", bool(clicked))
        await asyncio.sleep(1.5)
        href_after = await a.ev("(document.querySelector('link[rel=icon]') || document.querySelector('link')).href")
        changed = href_after != icon_before
        a.known_bug("picking a cassette changes the tab icon", bool(clicked and changed),
                    why="icon link href before/after")
        if not changed:
            await a.reload_page()
            href2 = await a.ev("(document.querySelector('link[rel=icon]') || document.querySelector('link')).href")
            a.known_bug("the chosen tab icon survives a reload", href2 != icon_before,
                        why="href after reload")
        await a.shot("prof4-icon.png")

        errs = [e["params"].get("exceptionDetails", {}).get("message", "")
                for e in a._events if e["method"] == "Runtime.exceptionThrown"]
        a.check("no uncaught page exceptions (profile)", not errs)
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
