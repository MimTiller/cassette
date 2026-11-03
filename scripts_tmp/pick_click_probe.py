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
        # Click the composer emoji button (bottom bar, right side).
        for m in ("mouseMoved", "mousePressed", "mouseReleased"):
            await cdp("Input.dispatchMouseEvent", type=m, x=1291, y=876, buttons="left" if m == "mousePressed" else None)
        await asyncio.sleep(1.5)
        expr = """
(() => {
  const all = Array.from(document.querySelectorAll('*'));
  const labels = all.filter(e => { const t=(e.textContent||'').trim(); return t==='THIS SERVER' && !e.querySelector('*'); });
  const imgs = Array.from(document.querySelectorAll('img')).map(i => { const b=i.getBoundingClientRect(); return {alt:i.alt, x:Math.round(b.x), y:Math.round(b.y), w:Math.round(b.width), h:Math.round(b.height), src:(i.src||'EMPTY').slice(-46), ok: i.complete && i.naturalWidth>0, ce: i.complete}; });
  return JSON.stringify({labels: labels.map(e => {const b=e.getBoundingClientRect(); return [Math.round(b.x),Math.round(b.y),Math.round(b.width),Math.round(b.height)];}), imgs: imgs.filter(i => i.x>900 || (i.alt||'').includes('casspop'))});
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(r["result"].get("result", "?"))
        # close the picker
        await cdp("Input.dispatchMouseEvent", type="mousePressed", x=200, y=200, buttons="left")
        await cdp("Input.dispatchMouseEvent", type="mouseReleased", x=200, y=200, buttons=None)

asyncio.run(go())
