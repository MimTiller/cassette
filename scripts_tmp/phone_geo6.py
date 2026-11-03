import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# All divs 12..40px with their rects and parent sizes: rail circles, settings rows, etc.
EXPR = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const out = [];
  for (const e of document.querySelectorAll('div')) {
    const r = e.getBoundingClientRect();
    if (r.width >= 1 && r.height >= 1) continue;
  }
  // rail: circles
  for (const e of document.querySelectorAll('div')) {
    const r = e.getBoundingClientRect();
    if (Math.abs(r.width - 48) < 1 && Math.abs(r.height - 48) < 1) out.push({ tag: 'circle48', fr: fr(e) });
  }
  const scw = document.documentElement ? document.documentElement.scrollWidth : 0;
  return JSON.stringify({ scw: scw, els: out.slice(0, 12) });
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
            print(f"  {o['tag']}: {o['fr']}")

asyncio.run(go())
