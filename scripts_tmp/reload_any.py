"""MCP GAP fallback: reload the dev tab (Page.reload via CDP). reload_any.py PORT"""
import asyncio
import json
import urllib.request
import websockets
import sys

PORT = int(sys.argv[1])


async def reload():
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list"))
    tab = next(t for t in tabs if "127.0.0.1:8085" in t.get("url", ""))
    async with websockets.connect(tab["webSocketDebuggerUrl"], max_size=None) as ws:
        n = 0

        async def cdp(method, **params):
            nonlocal n
            n += 1
            await ws.send(json.dumps({"id": n, "method": method, "params": params}))
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=15))
                if "method" in msg and "id" not in msg:
                    continue
                if "error" in msg:
                    print("ERR:", msg["error"])
                    return
                return msg.get("result", {})

        await cdp("Page.reload", ignoreCache=False)
    print("reloaded")


asyncio.run(reload())
