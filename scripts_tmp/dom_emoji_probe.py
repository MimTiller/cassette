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
  const out = {inline: null, pills: [], picker: []};
  for (const img of document.querySelectorAll('img')) {
    if (img.alt === ':casspop:') {
      const r = img.getBoundingClientRect();
      const cs = getComputedStyle(img);
      // find the parent flow and its text siblings' ranges
      out.inline = {rect: [r.x, r.y, r.width, r.height], cs: {mt: cs.marginTop, mb: cs.marginBottom, va: cs.verticalAlignment, pos: cs.position, align: cs.alignSelf}};
      const p = img.parentElement;
      const spans = [];
      p.querySelectorAll('*').forEach(e => {
        const t = (e.textContent||'').trim();
        if (t && !e.querySelector('*')) spans.push({t, r: e.getBoundingClientRect(), cs: {fs: getComputedStyle(e).fontSize}});
      });
      out.inline.siblings = spans.map(s => ({t: s.t.slice(0,30), y: s.r.y, h: s.r.height, fs: s.cs.fs}));
    }
  }
  // reaction pills: look for the pill views with count text
  const all = Array.from(document.querySelectorAll('*'));
  out.counts = all.filter(e => (e.textContent||'').trim()==='1' && !e.querySelector('*')).length;
  return JSON.stringify(out);
})()
"""
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        print(r["result"]["result"])

asyncio.run(go())
