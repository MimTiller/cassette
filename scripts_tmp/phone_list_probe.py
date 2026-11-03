import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# For every visible span/div whose text is short, print its rect and the rect
# of its nearest ancestor that is a flex container (the row).
EXPR = """
(() => {
  const out = [];
  const seen = new Set();
  for (const e of document.querySelectorAll('span')) {
    const t = (e.textContent||'').trim();
    if (t.length < 3 || t.length > 40 || seen.has(t)) continue;
    seen.add(t);
    const r = e.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    if (r.right > window.innerWidth + 1 || r.left < -1 || r.bottom > window.innerHeight + 1 || r.top < -1) continue;
    out.push({ txt: t, l: Math.round(r.left), rt: Math.round(r.right), t2: Math.round(r.top), b: Math.round(r.bottom) });
  }
  return JSON.stringify(out.slice(0, 60));
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
        for o in json.loads(r["result"]["result"]["value"]):
            print(f"  {o['txt']!r} x{o['l']}..{o['rt']} y{o['t2']}..{o['b']}")
asyncio.run(go())
