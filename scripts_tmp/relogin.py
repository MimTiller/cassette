import asyncio, json, urllib.request, websockets, sys

PORT = 9341

async def go():
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
                if msg.get("id") == n:
                    return msg

        await cdp("Page.reload", ignoreCache=False)
        await asyncio.sleep(3)
        await cdp("Page.setDeviceMetricsOverride", width=1600, height=900, deviceScaleFactor=1, mobile=False)
        print("reloaded + resized")

asyncio.run(go())
