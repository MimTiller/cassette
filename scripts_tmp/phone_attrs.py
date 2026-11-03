import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# List the attributes on nodes whose text is a server-header label, and dump
# all distinct attribute names, so we can find how test_id is exposed.
EXPR = """
(() => {
  const attrs = new Set();
  for (const e of document.querySelectorAll('*')) {
    for (const a of e.attributes) attrs.add(a.name);
    if (attrs.size > 40) break;
  }
  const hdr = [...document.querySelectorAll('div')].find(e => (e.textContent||'').trim().startsWith('Cassette Demo') && e.getBoundingClientRect().width > 200 && e.getBoundingClientRect().top < 60);
  const h = hdr ? [...hdr.attributes].map(a => a.name + '=' + a.value.slice(0,60)) : [];
  return JSON.stringify({attrs: [...attrs], hdr: h});
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
        print("attrs:", v["attrs"])
        print("hdr:", v["hdr"])

asyncio.run(go())
