import asyncio, json, urllib.request, websockets, sys

PORT = 9341

async def go():
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list"))
    for t in tabs:
        print(t["id"], t.get("url"), t.get("title"))
    cands = [t for t in tabs if "127.0.0.1:8085" in t.get("url", "")]
    print("cands:", len(cands))
    for tab in cands:
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

            m = await cdp("Page.getDeviceMetricsOverride")
            print(tab["id"], "metrics:", m["result"]["result"])
            r = await cdp("Runtime.evaluate", expression="window.devicePixelRatio + ' ' + window.innerWidth + ' ' + window.innerHeight")
            print(tab["id"], "inner:", r["result"]["result"]["value"])
            await cdp("Page.setDeviceMetricsOverride", width=1600, height=900, deviceScaleFactor=1, mobile=False)
            m2 = await cdp("Page.getDeviceMetricsOverride")
            print(tab["id"], "metrics now:", m2["result"]["result"])

asyncio.run(go())
