import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

EXPR = """
(() => {
  const out = [];
  const targets = { 'ONLINE': null, 'Avery North (you)': null, 'MIXTAPES': null, 'CRATE': null };
  for (const e of document.querySelectorAll('span')) {
    const t = (e.textContent||'').trim();
    for (const k in targets) {
      if (t.includes(k) && targets[k] === null) {
        const r = e.getBoundingClientRect();
        if (r.width > 0) targets[k] = [Math.round(r.left), Math.round(r.top), Math.round(r.right), Math.round(r.width)];
      }
    }
  }
  // find divs with ONLINE text whose width > 200 (panel header row)
  for (const e of document.querySelectorAll('div')) {
    const t = (e.textContent||'').trim();
    if (t.startsWith('ONLINE') && e.getBoundingClientRect().width > 100) {
      const r = e.getBoundingClientRect();
      targets['ONLINE'] = targets['ONLINE'] || [Math.round(r.left), Math.round(r.top), Math.round(r.right), Math.round(r.width)];
    }
  }
  return JSON.stringify(targets);
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
        for k, val in v.items():
            print(f"  {k}: {val}")

asyncio.run(go())
