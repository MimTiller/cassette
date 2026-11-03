"""MCP GAP fallback: right-click (context menu) on a point to open the
message overflow menu / channel right-click menu. rclick_row.py PORT X Y"""
import asyncio
import json
import urllib.request
import websockets
import sys

PORT = int(sys.argv[1])
X = int(sys.argv[2])
Y = int(sys.argv[3])


async def main():
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
                if msg.get("id") != n:
                    continue
                if "error" in msg:
                    print("ERR:", msg["error"])
                    return
                return msg.get("result", {})

        await cdp("Input.dispatchMouseEvent", type="mousePressed", x=X, y=Y, button="right")
        await cdp("Input.dispatchMouseEvent", type="mouseReleased", x=X, y=Y, button="right")
    print("right-clicked")


asyncio.run(main())
