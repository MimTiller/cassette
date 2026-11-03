import asyncio, json, urllib.request, websockets

PORT = 9341
JS = """
(() => {
  const out = {};
  out.vw = window.innerWidth;
  out.dpr = window.devicePixelRatio;
  const pres = [];
  document.querySelectorAll('pre').forEach(e => { const r = e.getBoundingClientRect(); pres.push([Math.round(r.x), Math.round(r.width)]); });
  out.pre = pres;
  const cmp = document.querySelector('textarea, [class*=composer], input');
  const imgs = [];
  document.querySelectorAll('img').forEach(i => { const r = i.getBoundingClientRect(); if (r.width>100) imgs.push([Math.round(r.x), Math.round(r.width), Math.round(r.height), i.alt]); });
  out.imgs = imgs;
  const spans = [];
  document.querySelectorAll('span').forEach(s => { const t = (s.textContent||'').trim(); if (t === 'who wants the 4th pressing?') { const r = s.getBoundingClientRect(); spans.push([Math.round(r.x), Math.round(r.width)]); } });
  out.target = spans;
  return JSON.stringify(out);
})()
"""

async def go():
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list"))
    tab = next(t for t in tabs if "127.0.0.1:8085" in t.get("url", ""))
    async with websockets.connect(tab["webSocketDebuggerUrl"], max_size=None) as ws:
        n = 0

        async def cdp(method, **params):
            nonlocal n
            n += 1
            await ws.send(json.dumps({"id": n, "method": method, "params": params}))
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=20))
                if "id" in msg and msg["id"] == n:
                    return msg

        r = await cdp("Runtime.evaluate", expression=JS, returnByValue=True)
        print(r["result"]["result"].get("value"))

asyncio.run(go())
