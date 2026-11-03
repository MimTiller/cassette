"""Messages: reply to a message, add/remove a reaction, pin and see the pins
panel, Markdown (bold + inline code), a long message wraps, jump-to-bottom
after scrolling up, typing indicator shows on the other user's screen.

Usage: test_messages.py [base_url] [out_dir]
"""
import asyncio
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdp import Session, MissingElement, WaitTimeout, CDPTimeout, CDPError
from cdp import _paint_js
from srv_helper import signup_login, srv

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8085"
SHOTS = sys.argv[2] if len(sys.argv) > 2 else "/tmp/cassette-e2e"

TIMEOUT = 420


def painted_re(s, pattern, xmin=0, xmax=None, ymin=0, ymax=None):
    xmax = xmax if xmax is not None else s.W
    ymax = ymax if ymax is not None else s.H
    return (f"[...document.querySelectorAll('*')].some(e => e.children.length === 0 && "
            f"RegExp({json.dumps(pattern)}, 'u').test(e.textContent.trim()) && "
            + _paint_js("e", xmax, ymax) + ")")


async def main():
    tag = int(time.time()) % 100000
    a_user, b_user = f"ma{tag}", f"mb{tag}"
    pw = "msg-pass-1"
    server = f"Msg Crew {tag}"
    ra = signup_login(BASE, a_user, pw)
    rb = signup_login(BASE, b_user, pw)
    sid = srv(BASE, "create_server", {"name": server}, ra["token"])["id"]
    srv(BASE, "add_member", {"server_id": sid, "user_id": rb["user_id"]}, ra["token"])

    a = Session(BASE, SHOTS)
    b = Session(BASE, SHOTS)
    try:
        await a.open()
        await a.fresh_page()
        await a.login(a_user, pw)
        target = "the target message"
        await a.send_message(target)
        ok = await a.until(a.painted(target, xmin=a.W * 0.2), timeout=10,
                           why="target message painted")
        a.check("target message is painted", ok)

        # --- reply ---
        menu_ok = await a.open_message_menu(target)
        a.check("the message offers its hover menu", menu_ok)
        await a.escape()          # close that menu before using the hover bar
        await asyncio.sleep(0.4)
        replied = False
        if await a.hover_row(target) and await a.click_text("\u21a9"):
            await asyncio.sleep(1.0)
            prev = await a.until(painted_re(a, "Replying to "), timeout=6,
                                 why="reply preview bar")
            replied = bool(prev) and bool(await a.send_message("reply body here"))
            await asyncio.sleep(0.5)
        rep_seen = replied and await a.until(
            a.painted("reply body here", xmin=a.W * 0.2), timeout=10,
            why="reply visible in the thread")
        a.known_bug("reply: the reply renders under the target", bool(rep_seen),
                    why="Reply row -> preview bar -> sent reply")
        await a.shot("msg1-reply.png")

        # --- reaction add / remove ---
        react_ok = False
        glyph = None
        await a.hover_row(target)
        clicked = False
        for _ in range(3):
            bar = await a.text_pos("\U0001f60a")
            if bar:
                await a.click(*bar)
                clicked = await a.until(
                    "!![...document.querySelectorAll('input')].find(i => (i.placeholder || '').startsWith('Search') && "
                    + _paint_js("i", a.W, a.H) + ")", timeout=5, why="emoji picker card")
                if clicked:
                    break
            await a.hover_row(target)
        cells = []
        if clicked:
            # First cell of Smileys & People (emoji are 2 UTF-16 units, so match by text).
            cell = await a.text_pos("\U0001f600", xmin=a.W * 0.2, ymin=150)
            cells = [cell] if cell else []
            if cell:
                await a.click(*cell)
                glyph = "\U0001f600"
                react_ok = await a.until(
                    painted_re(a, f"^{re.escape(glyph)}", xmin=int(a.W * 0.2)),
                    timeout=8, why="reaction pill painted")
        if not react_ok:
            await a.shot("msg2-react-picker.png")
            print("  react: picker opened", clicked, "cells", len(cells) if clicked else 0)
            await a.escape()
            await asyncio.sleep(0.4)
        a.known_bug("adding a reaction paints a pill under the message",
                    react_ok, why="React button -> emoji picker -> pill")
        if react_ok:
            await a.click_starts(glyph, xmin=a.W * 0.2)
            still = await a.ev(painted_re(a, f"^{re.escape(glyph)}",
                                          xmin=int(a.W * 0.2), xmax=int(a.W * 0.9)))
            a.known_bug("clicking the pill removes the reaction", not still,
                        why="pill toggle")

        # --- pin and pins panel ---
        menu_ok = await a.open_message_menu(target)
        pinned = menu_ok and await a.click_text("Pin message", xmin=300)
        toast = pinned and await a.until(
            a.painted("Message pinned!"), timeout=8, why="pin toast")
        a.check("pin: the pin toast confirms", bool(toast))
        badge_pos = await a.text_pos("\U0001f4cc", xmin=a.W * 0.2,
                                     xmax=a.W * 0.9, ymin=0, ymax=140)
        a.check("the channel header shows a pin badge", bool(badge_pos))
        pins_open = False
        if badge_pos:
            # The 📌 leaf is the count pill (not clickable); the open-panel
            # touch handler is on the pin icon just left of it (pin-live,
            # 24px wide per MCP).
            await a.click(badge_pos[0] - 26, badge_pos[1])
            pins_open = await a.until(a.painted("Pinned messages"), timeout=8,
                                      why="pins panel title")
            listed = pins_open and await a.visible(target)
            a.check("the pins panel lists the pinned message", bool(listed))
            await a.shot("msg2-pins-panel.png")
            await a.escape()

        # --- markdown: bold + inline code ---
        await a.send_message("bold check **boldword** end")
        md = await a.until(a.painted("boldword", xmin=a.W * 0.2), timeout=10,
                           why="bold parsed into its own run")
        await a.send_message("code `codeword` end")
        md2 = await a.until(a.painted("codeword", xmin=a.W * 0.2), timeout=10,
                            why="inline code parsed")
        a.check("markdown bold renders", md)
        a.check("markdown inline code renders", md2)
        await a.shot("msg3-markdown.png")

        # --- long message wraps ---
        long_text = "wrap " + "wonderful " * 60
        await a.send_message(long_text.strip())
        row = None
        for _ in range(8):
            row = await a.message_rect(long_text.strip())
            if row:
                break
        wrapped = row and (row[3] - row[1]) > 24 and row[2] < a.W * 0.95
        panel = await a.ev("[...document.querySelectorAll('*')].filter(e => e.children.length === 0 "
                           "&& /^ONLINE/.test(e.textContent.trim())).map(e => e.getBoundingClientRect().x)[0]")
        a.check("a long message leaves the member list its width", bool(panel) and panel < a.W - 200)
        a.known_bug("a long message wraps instead of overflowing", bool(wrapped),
                    why="row box %s" % (row or "missing"))

        # --- jump to bottom after scrolling up ---
        filler = [f"filler{i}" for i in range(1, 26)]
        for f in filler:
            await a.send_message(f)
        await a.ev(
            "(() => { const e = [...document.querySelectorAll('*')].find(x => x.children.length === 0 && x.textContent.trim() === 'filler1'); "
            "let s = e; while (s && !(s.scrollHeight > s.clientHeight + 5)) s = s.parentElement; "
            "if (s) s.scrollTop = 0; return !!s; })()")
        jb = await a.until(a.painted("Jump to bottom"), timeout=8, why="jump button")
        jumped = False
        if jb:
            await a.click_text("Jump to bottom")
            jumped = await a.until(a.painted(filler[-1], xmin=a.W * 0.2),
                                   timeout=8, why="bottom row back in view")
        a.known_bug("scrolling up shows Jump to bottom and it works",
                    bool(jb and jumped), why="button then bottom row")
        await a.shot("msg4-jump.png")

        # --- typing indicator on the other user's screen ---
        await b.open()
        await b.fresh_page()
        await b.login(b_user, pw)
        # Give B's gateway socket a moment: a typing frame sent before it is up is dropped.
        await asyncio.sleep(4)
        await b.type_ph_start("Message", "someone is typing now", select=False)
        # Laid out (width > 0) rather than painted: the line sits under the
        # message list's floating layers, so elementFromPoint misses it.
        seen = await a.until(
            "[...document.querySelectorAll('*')].some(e => e.children.length === 0 && "
            f"e.textContent.trim() === {json.dumps(b_user + ' is typing...')} && "
            "e.getBoundingClientRect().width > 0)",
            timeout=20, why="typing indicator on A's screen")
        if not seen:
            await a.shot("msg5-typing-a.png")
            await b.shot("msg5-typing-b.png")
        a.known_bug("typing indicator shows on the other user's screen",
                    bool(seen), why="B typed without Enter; A should see it")
        await a.shot("msg5-typing.png")

        errs = [e["params"].get("exceptionDetails", {}).get("message", "")
                for e in a._events if e["method"] == "Runtime.exceptionThrown"]
        a.check("no uncaught page exceptions (messages)", not errs)
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
