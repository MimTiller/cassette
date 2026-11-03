import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

EXPR = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const hits = {};
  for (const e of document.querySelectorAll('span')) {
    const t = (e.textContent || '').trim();
    if (t === 'ESC' || t === '\u2715' || t === '✕') { hits[t] = fr(e); }
  }
  // the Theme option row widths
  const scw = document.documentElement ? document.documentElement.scrollWidth : 0;
  // find any element overflowing the viewport right side (top half)
  const over = [];
  for (const e of document.querySelectorAll('*')) {
    const r = e.getBoundingClientRect();
    if (r.width > 5 && r.right > window.innerWidth + 1 && r.top > -1 && r.top < 450) {
      over.push([Math.round(r.left), Math.round(r.right), Math.round(r.top), e.tagName, ((e.textContent||'').trim()).slice(0, 20)]);
    }
  }
  return JSON.stringify({ scw: scw, hits: hits, over: over.slice(0, 25) });
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
        for k, val in v["hits"].items():
            print(f"  {k!r}: {val}")
        print("overflowing (top half):")
        for o in v["over"]:
            print(f"  x {o[0]}..{o[1]} y {o[2]} :: {o[3]} {o[4]!r}")

asyncio.run(go())
