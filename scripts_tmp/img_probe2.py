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
  const imgs = Array.from(document.querySelectorAll('img'));
  const info = imgs.map(i => { const b = i.getBoundingClientRect(); return {alt: i.alt, x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.width), h: Math.round(b.height), src: (i.src||'').slice(-46), ok: i.complete && i.naturalWidth>0}; });
  const thisServer = Array.from(document.querySelectorAll('*')).filter(e => { const t=(e.textContent||''); return t.includes('THIS SERVER') && t.length < 20; }).map(e => {const b=e.getBoundingClientRect(); return [Math.round(b.x),Math.round(b.y)];});
  return JSON.stringify({totalImgs: imgs.length, info, thisServer: thisServer.slice(0,5)});
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(json.dumps(r.get("result", r))[:4000])

asyncio.run(go())
