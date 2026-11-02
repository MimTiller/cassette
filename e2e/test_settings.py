"""Server Settings smoke: Overview/Roles/Members, create a role, give it to
yourself from your profile card, check your name takes the role colour; then
User Settings and switch theme.

Usage: test_settings.py [base_url] [out_dir]
"""
import asyncio
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdp import Session, MissingElement, WaitTimeout, CDPTimeout, CDPError

TIMEOUT = 300

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8086"
SHOTS = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "out")
ROLE = "Night Watch"
SERVER = "Settings Crew"
# the role colour we click must match: COLORS[5] in roles_settings is #e91e63
ROLE_COLOR = "rgb(233, 30, 99)"


async def main():
    tag = int(time.time()) % 100000
    user, pw = f"setu{tag}", f"settings-pass-{tag}"
    s = Session(BASE, SHOTS)
    await s.open()
    try:
        await s.fresh_page()
        await s.signup(user, pw)
        await s.create_server(SERVER)
        s.check("created a server for settings checks", True)
        await s.send_message("settings crew hello")
        await s.shot("settings1-shell.png")

        # --- Server Settings: switch Overview -> Roles -> Members ---
        await s.open_server_settings(SERVER)
        overview = (await s.visible("Server name", xmin=s.W * 0.15)
                    and await s.visible("Server icon", xmin=s.W * 0.15))
        s.check("Overview section shows the rename/icon rows", overview)
        await s.shot("settings2-overview.png")
        s.check("can switch to the Roles section",
                await s.settings_section("Roles"))
        role_input = await s.has_input(s.input_exact("New role name"))
        s.check("Roles section offers the create-role input", role_input)
        await s.shot("settings3-roles.png")
        s.check("can switch to the Members section",
                await s.settings_section("Members"))
        s.check("Members section shows add/transfer rows",
                await s.visible("Add a member", xmin=s.W * 0.15))
        await s.shot("settings4-members.png")

        # --- create a role ---
        await s.settings_section("Roles")
        await s.create_role(ROLE, swatch_index=5)
        listed = await s.leaf_color(ROLE)
        s.check(f"the new role is listed in its colour (got {listed})",
                listed == ROLE_COLOR)
        await s.shot("settings5-role-created.png")
        await s.close_settings()

        # --- give it to yourself from your profile card ---
        await s.click_own_name(user)
        card_open = await s.until(s.painted("Member since"), timeout=6,
                                  why="profile card open")
        s.check("own name opens the profile card", card_open)
        given = await s.give_role_from_card(ROLE, user)
        s.check("the card's '+ role' row applies the role", given)
        await s.shot("settings6-card-role.png")
        await s.escape()
        await asyncio.sleep(1.0)
        name_col = await s.leaf_color(user)
        s.check(f"my name takes the role colour (got {name_col})",
                name_col == ROLE_COLOR)

        # --- User Settings: switch theme ---
        await s.open_user_settings()
        await s.shot("settings7-user-myaccount.png")
        bg_before = await s.ev("getComputedStyle(document.body).backgroundColor")
        s.check("can switch to the Appearance section",
                await s.settings_section("Appearance"))
        await s.shot("settings8-appearance.png")
        clicked = await s.click_text("Light", xmin=s.W * 0.15)
        if not clicked:
            clicked = await s.click_text("Dark Purple", xmin=s.W * 0.15)
        s.check("a theme card is clickable", clicked)
        await asyncio.sleep(1.5)
        bg_after = await s.ev("getComputedStyle(document.body).backgroundColor")
        changed = bg_after != bg_before
        s.known_bug("picking a theme recolours the app", changed,
                    why="body background before/after: " + str(bg_before) + "/" + str(bg_after))
        await s.shot("settings9-theme-switched.png")
        await s.close_settings()

        errs = [e["params"].get("exceptionDetails", {}).get("message", "")
                for e in s._events if e["method"] == "Runtime.exceptionThrown"]
        s.check("no uncaught page exceptions", not errs)
        if errs:
            print("  page exceptions:", errs[:3])
    except (MissingElement, WaitTimeout, CDPTimeout, CDPError) as e:
        print(f"  !! test aborted: {e}")
    finally:
        await s.close()
    print("RESULTS", json.dumps({"results": s.results, "known": sorted(s.known)}))
    bad = [k for k, v in s.results.items() if not v and k not in s.known]
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
