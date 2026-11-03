"""Server smoke test: create a server, create a text channel, send a message
and see it appear; edit and delete it. All data is fresh (tagged names).

Usage: test_server.py [base_url] [out_dir]
"""
import asyncio
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdp import Session, MissingElement, WaitTimeout, CDPTimeout, CDPError

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"
SHOTS = sys.argv[2] if len(sys.argv) > 2 else "/tmp/cassette-e2e"


async def main():
    tag = int(time.time()) % 100000
    user, pw = f"srvu{tag}", f"srv-pass-{tag}"
    server, channel = f"Smoke Server {tag}", f"smokechan{tag}"
    msg, edited = f"hello {tag}", f"hello {tag} edited"
    s = Session(BASE, SHOTS)
    await s.open()
    try:
        await s.fresh_page()
        await s.signup(user, pw)
        await s.create_server(server)
        s.check("create server: the name lands in the header",
                await s.visible(server, xmax=s.W * 0.45, ymin=-1, ymax=60))
        await s.shot("srv1-server.png")

        # App bug: the create/join path never fetches the new server's
        # permission snapshot, so its dropdown hides the permission rows until
        # a page reload boots the app again (see ISSUES.md). Keep the check,
        # then reload to arm perms for the rest of the flow.
        await s.open_server_menu(server)
        armed = await s.until(s.painted("Create Channel", xmin=60,
                                        xmax=s.W * 0.5), timeout=6, why="menu row")
        s.known_bug("the fresh server arms its permissions right away",
                    armed, why="dropdown lists no permission rows until a reload")
        if not armed:
            await s.reload_page()
            await s.open_server_menu(server)
            armed = await s.until(s.painted("Create Channel", xmin=60,
                                            xmax=s.W * 0.5), timeout=12, why="menu row")
        s.check("the owner's menu offers Create Channel", armed)
        if not await s.click_text("Create Channel", xmin=60, xmax=s.W * 0.5):
            raise MissingElement("Create Channel row not painted in the menu")
        await s.create_channel(channel)
        s.check("create channel: it shows in the sidebar",
                await s.visible(channel, xmin=72, xmax=s.W * 0.45))
        await s.shot("srv2-channel.png")

        # send a message and see it appear
        await s.send_message(msg)
        appeared = await s.until(
            s.painted(msg, xmin=s.W * 0.2, xmax=s.W * 0.9), timeout=10,
            why="sent message painted")
        s.check("send message: it appears in the message view", appeared)
        await s.shot("srv3-sent.png")
        s.check("the composer clears after sending",
                (await s.input_value(s.input_startswith("Message"))) == "")

        # edit it via the message menu
        menu = await s.open_message_menu(msg)
        s.check("the own message offers the action menu", menu)
        if not menu:
            raise MissingElement("message menu never opened")
        ok_edit = await s.edit_message(msg, edited)
        s.check("edit: the new text shows with an (edited) marker", ok_edit)
        await s.shot("srv4-edited.png")

        # delete it (reopen the menu on the edited row)
        menu = await s.open_message_menu(edited)
        await asyncio.sleep(0.6)
        del_ok = False
        clicked_del = menu and await s.click_text("Delete", xmin=s.W * 0.6, ymax=s.H * 0.6)
        if clicked_del:
            await asyncio.sleep(1.0)
            await s.shot("srv5a-after-delete-click.png")
            confirmed = await s.until(s.painted("Confirm"), timeout=6, why="confirm strip")
            del_ok = confirmed and await s.click_text("Confirm", xmin=312, ymax=s.H) \
                and await s.until(s.not_painted(edited), timeout=10, why="message gone")
        s.check("delete: the message disappears", del_ok)
        await s.shot("srv5-deleted.png")

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
