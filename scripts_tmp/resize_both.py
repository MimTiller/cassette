import asyncio, json, urllib.request, websockets

PORT = 9341

async def go():
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list"))
    cands = [t for t in tabs if "127.0.0.1:8085" in t.get("url", "")]
    for tab in cands:
        async with websockets.connect(tab["webSocketDebuggerUrl"], max_size=None) as ws:
            n = 0

            async def cdp(method, **params):
                nonlocal n
                n += 1
                await ws.send(json.dumps({"id": n, "method": method, "params": params}))
                while True:
                    msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=15))
                    if "id" in msg:
                        if msg.get("id") == n:
                            return msg

            await cdp("Emulation.setDeviceMetricsOverride", width=1600, height=900, deviceScaleFactor=1, mobile=False)
            print(tab["id"], "resized")

asyncio.run(go())
