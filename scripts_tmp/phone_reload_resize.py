import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

async def go():
    tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9341/json/list"))
    tab = next(t for t in tabs if t["id"] == tid)
    async with websockets.connect(tab["webSocketDebuggerUrl"], max_size=None) as ws:
        n = 0
        async def cdp(method, **params):
            nonlocal n
            n += 1
            await ws.send(json.dumps({"id": n, "method": method, "params": params}))
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=20))
                if "id" in msg and msg["id"] == n:
                    return msg
        await cdp("Page.reload", ignoreCache=True)
        await asyncio.sleep(2)
        await cdp("Emulation.setDeviceMetricsOverride", width=390, height=844, deviceScaleFactor=1, mobile=False)
        print("reloaded + resized 390x844")

asyncio.run(go())
