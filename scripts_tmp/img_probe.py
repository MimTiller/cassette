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
  const imgs = Array.from(document.querySelectorAll('img')).map(i => ({src:(i.src||'').slice(-40), alt:i.alt, w:i.width, h:i.height, nw:i.naturalWidth, nh:i.naturalHeight, ok:i.complete && i.naturalWidth>0}));
  const cs = img => { const c=getComputedStyle(img); return {mt:c.marginTop,mb:c.marginBottom,al:c.alignSelf,va:c.verticalAlign}; };
  return JSON.stringify({imgs, css: Array.from(document.querySelectorAll('img')).map(cs)});
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(r["result"]["result"])

asyncio.run(go())
