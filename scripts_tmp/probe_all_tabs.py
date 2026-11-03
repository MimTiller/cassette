import asyncio, json, urllib.request, websockets

async def probe_tab(tab):
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
  const imgs = Array.from(document.querySelectorAll('img')).map(i => { const b=i.getBoundingClientRect(); return {alt:i.alt, x:Math.round(b.x), y:Math.round(b.y), w:Math.round(b.width), src:(i.src||'').slice(-46), ok: i.complete && i.naturalWidth>0}; });
  return JSON.stringify({labels: labels.length, pickerImgs: imgs.filter(i => i.x>900), casspop: imgs.filter(i => (i.alt||'').includes('casspop'))});
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        return r.get("result", {}).get("result", "?")

async def go():
    tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9341/json/list"))
    for t in tabs:
        if "127.0.0.1:8085" in t.get("url", ""):
            print(t["id"], await probe_tab(t))

asyncio.run(go())
