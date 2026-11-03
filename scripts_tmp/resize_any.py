"""MCP GAP fallback: set a CDP tab's viewport. Usage: resize_any.py PORT W H"""
import asyncio
import json
import urllib.request
import websockets
import sys

PORT = int(sys.argv[1])


async def resize(w, h):
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list"))
    targets = [t for t in tabs if "127.0.0.1:8085" in t.get("url", "")]
    print(f"{len(targets)} tabs")
    for tab in targets:
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

            await cdp("Emulation.setDeviceMetricsOverride",
                      width=w, height=h, deviceScaleFactor=1, mobile=False)
    print(f"resized to {w}x{h}")


asyncio.run(resize(int(sys.argv[2]), int(sys.argv[3])))
