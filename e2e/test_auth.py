"""Auth smoke test: sign up a fresh account, sign out, sign back in.

Usage: test_auth.py [base_url] [out_dir]
Writes `RESULTS {json}` on the last line: {"results": {...}, "known": [...]}.
"""
import asyncio
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdp import Session, MissingElement, WaitTimeout, CDPTimeout, CDPError

TIMEOUT = 220

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8086"
SHOTS = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "out")


async def main():
    tag = int(time.time()) % 100000
    user, pw = f"auth{tag}", f"auth-pass-{tag}"
    s = Session(BASE, SHOTS)
    await s.open()
    try:
        await s.fresh_page()
        s.check("login screen shows on a fresh page",
                await s.visible("Welcome back!"))
        await s.shot("auth1-login.png")

        # 1. sign up with a fresh random account
        await s.signup(user, pw)
        s.check("sign-up signs the fresh account in",
                await s.visible("USER SETTINGS", xmax=s.W * 0.3))
        s.check("the user panel shows the new account",
                await s.visible(user, ymin=s.H * 0.8))
        await s.shot("auth2-signed-up.png")

        # 2. sign out via the User Settings sidebar's Log Out row
        await s.open_user_settings()
        s.check("User Settings offers Log Out",
                await s.visible("Log Out", xmax=s.W * 0.3))
        await s.log_out()
        signed_out = (await s.visible("Welcome back!")
                      and not await s.visible("USER SETTINGS", xmax=s.W * 0.3))
        s.check("sign-out lands back on the login screen", signed_out)
        await s.shot("auth3-signed-out.png")

        # 3. sign in again with the same credentials
        await s.login(user, pw)
        signed_in = await s.visible("USER SETTINGS", xmax=s.W * 0.3)
        s.check("sign-in again restores the signed-in shell", signed_in)
        await s.shot("auth4-signed-in-again.png")

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
