"""MCP GAP fallback: press a key (Enter) in the tab (no MCP key-press verb).
key_any.py PORT KEY"""
import asyncio
import json
import urllib.request
import websockets
import sys

PORT = int(sys.argv[1])
KEY = sys.argv[2]


async def press():
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
                return msg.get("result", {})

        if KEY == "Enter":
            await cdp("Input.dispatchKeyEvent", type="keyDown", key="Enter", code="Enter", windowsVirtualKeyCode=13)
            await cdp("Input.dispatchKeyEvent", type="keyUp", key="Enter", code="Enter", windowsVirtualKeyCode=13)
        else:
            await cdp("Input.dispatchKeyEvent", type="insertText", text=KEY)
    print("pressed")


asyncio.run(press())
