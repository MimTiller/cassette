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
  const res = {inline: [], pill: [], picker: false};
  for (const img of document.querySelectorAll('img')) {
    if ((img.alt||'').includes('casspop')) {
      const b = img.getBoundingClientRect();
      const p = img.parentElement;
      const sibs = [];
      for (const c of p.children) {
        const bb = c.getBoundingClientRect();
        sibs.push({tag: c.tagName, y: bb.y, h: bb.height, t: (c.textContent||'').trim().slice(0,20)});
      }
      res.inline.push({img: [Math.round(b.x), Math.round(b.y), Math.round(b.width), Math.round(b.height)], sibs});
    }
  }
  const all = Array.from(document.querySelectorAll('*'));
  res.picker = all.some(e => (e.textContent||'').trim()==='THIS SERVER' && !e.querySelector('*'));
  return JSON.stringify(res);
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(r["result"]["result"])

asyncio.run(go())
