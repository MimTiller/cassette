"""MCP GAP fallback: the dev session's browser lost its tab (another agent
closed it). Re-open the 8085 tab and resize to 1600x900 so the app
re-registers with the idealyst relay."""
import asyncio, json, urllib.request, websockets

async def run():
    tab = json.load(urllib.request.urlopen(
        urllib.request.Request("http://127.0.0.1:9341/json/new?http://127.0.0.1:8085/", method="PUT"),
        timeout=10))
    print("tab id:", tab["id"])
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
        r = await cdp("Emulation.setDeviceMetricsOverride", width=1600, height=900, deviceScaleFactor=1, mobile=False)
        print("metrics:", r)
asyncio.run(run())
