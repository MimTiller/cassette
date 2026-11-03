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
  const t = all.filter(e => (e.textContent||'').trim()==='THIS SERVER' && !e.querySelector('*'));
  const r = t.map(e => { const b = e.getBoundingClientRect(); return [b.x,b.y,b.width,b.height]; });
  const imgs = Array.from(document.querySelectorAll('img')).filter(i => (i.alt||'') === ':casspop:').map(i => { const b=i.getBoundingClientRect(); return [b.x,b.y,b.width,b.height]; });
  return JSON.stringify({labelCount: t.length, labelRects: r, casspopRects: imgs});
})()
"""
        res = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(res["result"]["result"])

asyncio.run(go())
