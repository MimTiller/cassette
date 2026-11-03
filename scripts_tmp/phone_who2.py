import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# Settings nav rows: text + rects for every visible label in the settings screen
EXPRS = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const out = [];
  const seen = new Set();
  for (const e of document.querySelectorAll('span')) {
    const t = (e.textContent || '').trim();
    if (t.length < 2 || t.length > 45 || seen.has(t)) continue;
    const r = e.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    if (r.right > window.innerWidth + 1 || r.left < -1) continue;
    // settings section rows typically appear in the top half with short names
    if (r.top < 500) { seen.add(t); out.push({ txt: t, l: Math.round(r.left), t2: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) }); }
  }
  return JSON.stringify(out.slice(0, 50));
})()
"""

async def go():
    tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9341/json/list"))
    tab = next(t for t in tabs if t["id"] == tid)
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
        r = await cdp("Runtime.evaluate", expression=EXPRS, returnByValue=True)
        v = json.loads(r["result"]["result"]["value"])
        print("vw:", 390)
        for o in v:
            print(f"  {o['txt']!r} x={o['l']} y={o['t2']} w={o['w']} h={o['h']}")
        scw = await cdp("Runtime.evaluate", expression="document.documentElement.scrollWidth", returnByValue=True)
        print("scrollWidth:", scw["result"]["result"])

asyncio.run(go())
