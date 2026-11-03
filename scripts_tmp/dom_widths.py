import asyncio, json, urllib.request, websockets

PORT = 9341
JS = """
(() => {
  const out = [];
  function walk(el, d) {
    if (d > 24) return;
    for (const c of el.children) {
      const r = c.getBoundingClientRect();
      if (r.width > 100 && r.width < 1400 && r.x > 300 && r.x < 500) {
        const cls = (c.getAttribute('class')||'') + (c.tagName==='IMG'?' IMG':'');
        out.push([Math.round(r.x), Math.round(r.width), cls, d]);
      }
      walk(c, d+1);
    }
  }
  const root = document.getElementById('idealyst-root') || document.body.firstElementChild;
  if (root) walk(root, 0);
  const imgs = [];
  document.querySelectorAll('img').forEach(i => { const r = i.getBoundingClientRect(); imgs.push([Math.round(r.x), Math.round(r.width), Math.round(r.height)]); });
  return JSON.stringify({blocks: out.slice(0, 300), imgs: imgs});
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
        val = r["result"]["result"].get("value")
        data = json.loads(val)
        print("imgs:", data["imgs"])
        # widest per depth
        for b in sorted(data["blocks"], key=lambda x: -x[1])[:25]:
            print(b)

asyncio.run(go())
