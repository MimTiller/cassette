"""CDP helper for the :8085 dev tab (MCP gaps: viewport size, reload, navigate,
localStorage seed). Usage: tab.py resize W H | reload | reload-all | navigate URL |
theme KEY | close-mine <id...>
"""
import asyncio
import json
import urllib.request
import websockets
import sys


def dev_port():
    import glob
    import urllib.request
    cands = []
    try:
        for f in glob.glob("/root/cassette-v100/discord-clone/.idealyst/*/DevToolsActivePort") + glob.glob("/root/cassette-v100/discord-clone/target/idealyst-web-*/DevToolsActivePort"):
            first = open(f).read().split("\n")[0].strip()
            if first.isdigit():
                cands.append(int(first))
    except Exception:
        pass
    cands.append(35123)
    cands.append(9341)
    for port in cands:
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/json/list", timeout=2)
            return port
        except Exception:
            continue
    return 9341


async def cdp_on(ws_url, method, **params):
    async with websockets.connect(ws_url, max_size=None) as ws:
        await ws.send(json.dumps({"id": 1, "method": method, "params": params}))
        while True:
            msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=20))
            if "method" in msg and "id" not in msg:
                continue
            return msg.get("result", msg.get("error", {}))


async def run(cmd):
    port = dev_port()
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json/list"))
    mine = [t for t in tabs if "127.0.0.1:8085" in t.get("url", "")]
    if cmd == "resize":
        w, h = int(sys.argv[2]), int(sys.argv[3])
        for t in mine:
            r = await cdp_on(t["webSocketDebuggerUrl"], "Emulation.setDeviceMetricsOverride",
                             width=w, height=h, deviceScaleFactor=1, mobile=False)
            print(f"resized {t['id'][:8]}: {r.get('error', 'ok')}")
    elif cmd == "reload" or cmd == "reload-all":
        targets = mine if cmd == "reload-all" else mine[:1]
        for t in targets:
            await cdp_on(t["webSocketDebuggerUrl"], "Page.reload", ignoreCache=False)
            print(f"reloaded {t['id'][:8]}")
    elif cmd == "navigate":
        await cdp_on(mine[0]["webSocketDebuggerUrl"], "Page.navigate", url=sys.argv[2])
        print("navigated (first tab)")
    elif cmd == "theme":
        key = sys.argv[2]
        expr = (
            f"localStorage['discord-clone:idealyst.discord.theme'] = '{key}'"
            if key != "delete"
            else "delete localStorage['discord-clone:idealyst.discord.theme']"
        )
        r = await cdp_on(mine[0]["webSocketDebuggerUrl"], "Runtime.evaluate", expression=expr)
        print(json.dumps(r)[:200])
        await cdp_on(mine[0]["webSocketDebuggerUrl"], "Page.reload", ignoreCache=False)
        print("reloaded with theme")
    elif cmd == "close":
        # Only close tab ids passed on the command line (tabs this script opened).
        for t in mine:
            if t["id"] in sys.argv[2:]:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/json/close/{t['id']}")
                print(f"closed {t['id'][:8]}")

asyncio.run(run(sys.argv[1]))
