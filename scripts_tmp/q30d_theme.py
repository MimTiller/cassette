"""Set theme in localStorage and reload (dark='__del')."""
import asyncio, json, urllib.request, websockets, sys

KEY = sys.argv[1]
PORT = 9341

async def run():
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list", timeout=5))
    tab = next((t for t in tabs if "127.0.0.1:8085" in t.get("url", "")), None)
    if tab is None:
        print("NO 8085 TAB:", [t.get("url") for t in tabs])
        return
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
        expr = (f"delete localStorage['discord-clone:idealyst.discord.theme']; true"
                if KEY == "dark" else
                f"localStorage['discord-clone:idealyst.discord.theme']='light'; true")
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print("set:", r.get("result"))
        await cdp("Page.reload", ignoreCache=False)
        print("reloaded")
asyncio.run(run())
