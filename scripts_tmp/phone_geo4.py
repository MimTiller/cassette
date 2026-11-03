import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

IDS = ['server-header', 'rail-direct-messages', 'rail-create-server', 'message-search',
       'pin-live', 'invite-live', 'toggle-members', 'composer', 'members-scrim',
       'mobile-back', 'live-category', 'composer-emoji', 'composer-attach']

# Dump the first 12 [data-iv] elements' id/class/rect so we can see the mapping,
# then rect of every element whose text starts with ONLINE / 'Cassette Demo' / MIXTAPES
EXPR = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const ivs = [...document.querySelectorAll('[data-iv]')].slice(0, 14).map(e => ({ id: e.id, cls: (e.className||'').slice(0,40), fr: fr(e) }));
  const texts = {};
  for (const k of ['ONLINE', 'OFFLINE', 'MIXTAPES', 'CRATE', 'general', 'Avery North', 'weekly']) {
    for (const e of document.querySelectorAll('span')) {
      const t = (e.textContent||'').trim();
      if (t.includes(k)) { texts[k] = fr(e); break; }
    }
  }
  const scw = document.documentElement ? document.documentElement.scrollWidth : 0;
  return JSON.stringify({ vw: window.innerWidth, scw: scw, ivs: ivs, texts: texts });
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
        for o in v["ivs"]:
            print(f"  iv: id={o['id']} cls={o['cls']} fr={o['fr']}")
        for k, val in v["texts"].items():
            print(f"  text {k!r}: fr={val}")

asyncio.run(go())
