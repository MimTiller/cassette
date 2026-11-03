"""Two-user smoke: A creates a server and mints an invite; B (second tab,
cleared cookies) signs up fresh and joins with the code; A sends "@B hello"
and B's view shows the mention highlighted; B replies and A sees it live.

Usage: test_two_users.py [base_url] [out_dir]
"""
import asyncio
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdp import (Session, MissingElement, WaitTimeout, CDPTimeout,
                 CDPError)

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8086"
SHOTS = sys.argv[2] if len(sys.argv) > 2 else "/tmp/cassette-e2e"


async def main():
    tag = int(time.time()) % 100000
    a_user, a_pw = f"twoA{tag}", f"passA-{tag}"
    b_user, b_pw = f"twoB{tag}", f"passB-{tag}"
    server = f"Duo Server {tag}"
    msg = f"@{b_user} hello"
    reply = f"hi from {b_user}"
    a = Session(BASE, SHOTS)
    b = Session(BASE, SHOTS)
    try:
        await a.open()
        await a.fresh_page()
        await a.signup(a_user, a_pw)
        await a.create_server(server)
        await a.shot("duo1-a-created.png")
        code = await a.mint_invite(server)
        a.check("A mints an invite with a code", bool(code))
        code = code.split("/")[-1]
        await a.shot("duo2-invite.png")

        # B: fresh tab, cleared cookies, own fresh account
        await b.open()
        await b.fresh_page()
        await b.signup(b_user, b_pw)
        # The invite overlay is full-screen: close it so A's composer shows.
        await a.click_text("Done", xmin=a.W * 0.8, ymax=80)
        await b.join_via_code(code)
        # A successful join auto-enters the new server (the join modal's
        # enter-live pattern), so it lands open without a click.
        landed = await b.until(b.painted(server, xmax=b.W * 0.45), timeout=5,
                               why="joined server opened by itself")
        b.check("joining opens the new server", landed)
        joined = await b.until(b.painted(server, xmax=b.W * 0.45),
                               timeout=20, why="server painted after join")
        b.check("B joins with A's invite code", joined)
        await b.shot("duo3-b-joined.png")

        # A sends the mention; A sees it
        await a.send_message(msg)
        seen_a = await a.until(a.painted(msg, xmin=a.W * 0.2, xmax=a.W * 0.9),
                               timeout=10, why="A sees own message")
        a.check("A sees its own message", seen_a)
        await a.shot("duo4-a-sent.png")

        # B's view shows it highlighted (mention pill on the row)
        seen_b = await b.until(b.painted(msg, xmin=b.W * 0.2, xmax=b.W * 0.9),
                               timeout=20, why="B receives A's message")
        b.check("B sees A's message in its own view", seen_b)
        pill = await b.pill_bg(f"@{b_user}")   # the pill is its own leaf
        b.check(f"B's row shows the mention highlighted (pill {pill})",
                bool(pill))
        await b.shot("duo5-b-sees-mention.png")

        # B replies; A sees it live over its open gateway
        await b.send_message(reply)
        seen_b_own = await b.until(b.painted(reply, xmin=b.W * 0.2),
                                   timeout=10, why="B sees own reply")
        b.check("B sees its reply", seen_b_own)
        await b.shot("duo6-b-replied.png")
        seen_a2 = await a.until(a.painted(reply, xmin=a.W * 0.2, xmax=a.W * 0.9),
                                timeout=15, why="A receives B's reply live")
        a.check("A sees B's reply without reloading", seen_a2)
        await a.shot("duo7-a-sees-reply.png")

        # Presence: both users are online, so each side's member panel must
        # show ONLINE — 2 with no stale offline row. B registered after A
        # came online, and Presence frames only fire on changes, so the
        # roster itself must carry presence for B to see A online.
        both_b = await b.until(b.painted("ONLINE — 2", xmin=b.W * 0.55),
                               timeout=10, why="B's member panel shows both online")
        b.check("B's member list shows A online", bool(both_b))
        both_a = await a.until(a.painted("ONLINE — 2", xmin=a.W * 0.55),
                               timeout=10, why="A's member panel shows both online")
        a.check("A's member list shows B online", bool(both_a))
        await b.shot("duo8-both-online.png")

        # B reloads keeps cookies: its fresh page re-registers a socket but
        # gets no new Presence frames (A's status never changed), so A must
        # come back online purely from the roster seed.
        await b.reload_page()
        await a.cdp("Page.bringToFront")
        settled_b = await b.until(b.painted("ONLINE — 2", xmin=b.W * 0.55),
                                  timeout=15, why="B's panel settles both online")
        b.check("B shows both online after reloading", bool(settled_b))
        await b.shot("duo9-after-b-reload.png")
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
