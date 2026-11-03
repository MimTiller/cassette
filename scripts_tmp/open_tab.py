import json, urllib.request, asyncio, websockets

async def go():
    req = urllib.request.Request("http://127.0.0.1:9341/json/new?http://127.0.0.1:8085/", method="PUT")
    t = json.load(urllib.request.urlopen(req, timeout=5))
    print("new tab:", t["id"], t["url"])
    async with websockets.connect(t["webSocketDebuggerUrl"], max_size=None) as ws:
        n = 0
        async def cdp(method, **params):
            nonlocal n
            n += 1
            await ws.send(json.dumps({"id": n, "method": method, "params": params}))
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=15))
                if "id" in msg and msg["id"] == n:
                    return msg
        await cdp("Emulation.setDeviceMetricsOverride", width=1600, height=900, deviceScaleFactor=1, mobile=False)
        print("resized 1600x900")

asyncio.run(go())
