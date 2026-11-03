"""MCP GAP fallback: drive the :8085 tab's login via CDP (Page.navigate reload).
Reason: after a login click the app's UI socket reconnects but MCP lost the
relay; reloading the page re-registers the app with the dev session."""
import asyncio, json, urllib.request, websockets, sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 9341

async def run():
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list", timeout=5))
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
                return msg.get("result", {})
        await cdp("Runtime.evaluate", expression="window.location.reload()", returnByValue=True)
        print("reloaded")
asyncio.run(run())
