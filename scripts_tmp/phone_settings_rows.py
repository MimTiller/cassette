import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# dump id/role/aria-label/test attributes for spans with settings labels
EXPR = """
(() => {
  const want = ['Account','Appearance','Notifications','Keybinds','Voice','Privacy','Server','Log Out','My Account'];
  const hits = [];
  for (const e of document.querySelectorAll('span')) {
    const t = (e.textContent||'').trim();
    if (want.some(w => t === w || t.startsWith(w))) {
      const r = e.getBoundingClientRect();
      const attrs = {};
      for (const a of e.attributes) attrs[a.name] = (a.value||'').slice(0,50);
      hits.push({txt: t, fr: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)], attrs: attrs});
    }
  }
  return JSON.stringify(hits.slice(0, 25));
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
        for h in json.loads(r["result"]["result"]["value"]):
            print(f"  {h['txt']!r} fr={h['fr']} attrs={h['attrs']}")

asyncio.run(go())
