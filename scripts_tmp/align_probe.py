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
  const out = {casspop: [], pills: []};
  for (const img of document.querySelectorAll('img')) {
    if ((img.alt||'').includes('casspop')) {
      const b = img.getBoundingClientRect();
      const cs = getComputedStyle(img);
      out.casspop.push({y: b.y, h: b.height, align: cs.alignSelf, va: cs.verticalAlignment});
      // sibling leaf text in same parent
      const p = img.parentElement;
      for (const c of p.children) {
        if (c === img) continue;
        const bb = c.getBoundingClientRect();
        out.casspop.push({sib: (c.textContent||'').trim().slice(0,20), sibY: bb.y, sibH: bb.height});
      }
    }
  }
  return JSON.stringify(out);
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(r["result"]["result"][:3000])

asyncio.run(go())
