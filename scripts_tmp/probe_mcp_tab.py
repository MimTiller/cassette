import asyncio, json, websockets

# The dev session's own headless tab (the one the idealyst MCP bridge drives).
WS = "ws://127.0.0.1:35123/devtools/page/"

async def go(expr):
    # discover the tab id from the /json/list on that port
    import urllib.request
    tabs = json.load(urllib.request.urlopen("http://127.0.0.1:35123/json/list"))
    tab = next(t for t in tabs if "8085" in t.get("url", ""))
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
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(r["result"].get("result", "?"))

expr = """
(() => {
  const all = Array.from(document.querySelectorAll('*'));
  const labels = all.filter(e => { const t=(e.textContent||'').trim(); return t==='THIS SERVER' && !e.querySelector('*'); });
  const imgs = Array.from(document.querySelectorAll('img')).map(i => { const b=i.getBoundingClientRect(); return {alt:i.alt, x:Math.round(b.x), y:Math.round(b.y), w:Math.round(b.width), src:(i.src||'EMPTY').slice(-46), ok: i.complete && i.naturalWidth>0}; });
  return JSON.stringify({labels: labels.map(e => {const b=e.getBoundingClientRect(); return [Math.round(b.x),Math.round(b.y),Math.round(b.width),Math.round(b.height)];}), imgs: imgs.filter(i => (i.alt||'').includes('casspop')), totalImgs: imgs.length});
})()
"""
asyncio.run(go(expr))
