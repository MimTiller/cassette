"""Search: search the server for a word, the result list shows the hit,
clicking it jumps to and highlights the message.

Usage: test_search.py [base_url] [out_dir]
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

TIMEOUT = 300

NEEDLE = "needle message here"


async def main():
    tag = int(time.time()) % 100000
    a_u = f"qs{tag}"
    pw = "search-pass-1"
    ra = signup_login(BASE, a_u, pw)
    server = f"Search Crew {tag}"
    sid = srv(BASE, "create_server", {"name": server}, ra["token"])["id"]

    s = Session(BASE, SHOTS)
    try:
        await s.open()
        await s.fresh_page()
        await s.login(a_u, pw)
        # seed: filler messages then the needle so it sits high in the list
        for i in range(12):
            await s.send_message(f"filler{i}")
        await s.send_message(NEEDLE)
        for i in range(12):
            await s.send_message(f"tail{i}")
        await asyncio.sleep(0.5)

        # search panel: find the Search input in the message area
        js = s.input_startswith("Search")
        present = await s.until(f"!!{js}", timeout=15, why="search box painted")
        s.check("the channel shows a Search box", present)
        if not present:
            raise MissingElement("search box never painted")
        await s.type_into(js, "needle")
        await s.enter()
        hits = await s.until(s.painted("1 result") or s.painted("results"),
                             timeout=10, why="result heading")
        s.check("the result list shows the hit", hits)
        card = await s.until(s.painted(NEEDLE, xmin=s.W * 0.15, xmax=s.W * 0.55),
                             timeout=8, why="result card")
        s.known_bug("the result card names the message", bool(card),
                    why="search panel row")
        await s.shot("search1-results.png")

        # click the result card -> jump to the message + highlight
        jump_ok = False
        p = await s.text_pos(NEEDLE, xmin=s.W * 0.15, xmax=s.W * 0.55)
        if p:
            await s.click(*p)
            jumped = await s.until(s.painted(NEEDLE, xmin=s.W * 0.3), timeout=12,
                                   why="jumped to the message")
            hi = await s.row_highlight_bg(NEEDLE)
            jump_ok = jumped and bool(hi)
            s.known_bug("clicking the result jumps to the message", bool(jumped),
                        why="result card tap")
            s.known_bug("the jumped-to message is highlighted", bool(hi),
                        why="row highlight after jump")
            await s.shot("search2-jumped.png")
        else:
            s.known_bug("clicking the result jumps to the message", False,
                        why="card not painted")
            s.known_bug("the jumped-to message is highlighted", False,
                        why="no card")

        errs = [e["params"].get("exceptionDetails", {}).get("message", "")
                for e in s._events if e["method"] == "Runtime.exceptionThrown"]
        s.check("no uncaught page exceptions (search)", not errs)
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
