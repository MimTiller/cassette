import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# Measure tap boxes for: rail buttons, user-panel icon buttons, composer buttons
EXPR = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const out = [];
  const seen = new Set();
  // all small interactive-looking divs in the chat header band and bottom strip
  for (const e of document.querySelectorAll('div')) {
    const r = e.getBoundingClientRect();
    const w = Math.round(r.width), h = Math.round(r.height);
    if (w >= 12 && w <= 30 && h >= 12 && h <= 30) {
      const k = w + 'x' + h + ':' + Math.round(r.left) + ':' + Math.round(r.top);
      if (!seen.has(k)) { seen.add(k); out.push({ w: w, h: h, l: Math.round(r.left), t: Math.round(r.top) }); }
    }
  }
  out.sort((a,b)=>a.t-b.t);
  return JSON.stringify({ scw: (document.documentElement||document.body).scrollWidth, els: out.slice(0, 30) });
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
        r = await cdp("Runtime.evaluate", expression=EXPR, returnByValue=True)
        v = json.loads(r["result"]["result"]["value"])
        print("scrollWidth:", v["scw"])
        for o in v["els"]:
            print(f"  {o['w']}x{o['h']} at x={o['l']} y={o['t']}")

asyncio.run(go())
