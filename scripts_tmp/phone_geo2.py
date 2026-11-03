import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

EXPR = """
(() => {
  const q = (s) => document.querySelector('[data-idealyst-test-id=' + s + ']');
  const fr = (el) => { const r = el.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const out = {};
  for (const id of ['server-header', 'rail-direct-messages', 'message-search', 'pin-live', 'invite-live', 'toggle-members', 'composer', 'members-scrim']) {
    const el = q('"' + id + '"');
    if (el) out[id] = fr(el);
  }
  let scw = document.documentElement ? document.documentElement.scrollWidth : 0;
  return JSON.stringify({ vw: window.innerWidth, scw: scw, els: out });
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
        print("vw:", v["vw"], "scrollWidth:", v["scw"])
        for k, val in v["els"].items():
            print(f"  {k}: left={val[0]} top={val[1]} w={val[2]} h={val[3]}")

asyncio.run(go())
