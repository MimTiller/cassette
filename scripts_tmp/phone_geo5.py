import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# Dump every non-empty data-iv attribute and the rect of elements whose
# text starts with key labels, so we can confirm what's laid out.
EXPR = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  // test_id probe: look for the attribute idealyst uses
  const e1 = document.querySelector('[data-iv]');
  const iv = e1 ? e1.getAttribute('data-iv') : null;
  return JSON.stringify({ scw: document.documentElement.scrollWidth, sample: iv });
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
        print(r["result"]["result"].get("value"))

asyncio.run(go())
