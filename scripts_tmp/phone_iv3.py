import json, urllib.request, websockets, asyncio, sys

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# List every [data-iv] element: its attribute value and rect, plus body scrollWidth
EXPR = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const ivs = [...document.querySelectorAll('[data-iv]')].map(e => ({ iv: e.getAttribute('data-iv'), fr: fr(e) })).filter(o => o.fr[2] > 0 && o.fr[3] > 0);
  const byid = {};
  for (const o of ivs) { if (o.iv && o.iv.trim()) { byid[o.iv] = o.fr; } }
  const scw = document.documentElement ? document.documentElement.scrollWidth : 0;
  return JSON.stringify({ vw: window.innerWidth, scw: scw, nonempty: ivs.filter(o=>o.iv&&o.iv.trim()).length, sample: ivs.slice(0, 5), ids: byid });
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
        print("vw:", v["vw"], "scrollWidth:", v["scw"], "nonempty data-iv:", v["nonempty"])
        for k, val in v["ids"].items():
            print(f"  {k}: left={val[0]} top={val[1]} w={val[2]} h={val[3]}")
        # any nonempty data-iv values
        vals = v.get("all", [])
asyncio.run(go())
