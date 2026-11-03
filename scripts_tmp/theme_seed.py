import asyncio, json, urllib.request, websockets, sys

PORT = 9341
KEY = 'discord-clone:idealyst.discord.theme'

async def go(action, value):
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
                if "id" in msg and msg["id"] == n:
                    return msg

        if action == "delete":
            expr = f"delete localStorage[{sys!r}]"
            expr = "delete localStorage['" + KEY + "']"
        else:
            expr = "localStorage['" + KEY + "'] = '" + value + "'"
        r = await cdp("Runtime.evaluate", expression=expr)
        print("seed:", r.get("result", {}).get("result", {}).get("value", "?"))
        await cdp("Page.reload", ignoreCache=False)
        await asyncio.sleep(2)
        await cdp("Emulation.setDeviceMetricsOverride", width=1600, height=900, deviceScaleFactor=1, mobile=False)
        print("reloaded+resized")

if __name__ == "__main__":
    asyncio.run(go(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else ""))
