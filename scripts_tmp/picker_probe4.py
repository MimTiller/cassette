import asyncio, json, urllib.request, websockets

async def go():
    tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9341/json/list"))
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
        expr = """
(() => {
  const all = Array.from(document.querySelectorAll('*'));
  const labels = all.filter(e => { const t=(e.textContent||'').trim(); return t==='THIS SERVER' && !e.querySelector('*'); });
  return JSON.stringify({labels: labels.map(e => {const b=e.getBoundingClientRect(); return [Math.round(b.x),Math.round(b.y),Math.round(b.width),Math.round(b.height),b.width, (getComputedStyle(e).fontSize)];}), n: labels.length});
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(r["result"].get("result","?"))

asyncio.run(go())
