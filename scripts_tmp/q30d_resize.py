"""Resize the existing :8085 tab on the shared 9341 browser to 1600x900
(mcp gap: viewport size; Page.reload drops the metrics override)."""
import asyncio, json, urllib.request, websockets

async def run():
    tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9341/json/list", timeout=5))
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
        await cdp("Emulation.setDeviceMetricsOverride", width=1600, height=900, deviceScaleFactor=1, mobile=False)
        print("resized")
asyncio.run(run())
