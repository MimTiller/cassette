import json, urllib.request, websockets, asyncio

tid = open("/root/cassette-v100/scripts_tmp/phone_tab_id.txt").read().strip()

# For rail/server tiles and sidebar rows: find elements and their rects by text
EXPR = """
(() => {
  const fr = (e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)]; };
  const out = {};
  // server rail tiles: imgs with alt = server names, and their parent
  for (const e of document.querySelectorAll('img')) {
    const alt = e.getAttribute('alt') || '';
    const r = e.getBoundingClientRect();
    if (alt.includes('Cassette Demo') && r.left < 100 && r.width > 0) {
      out['rail_tile_img'] = fr(e);
      let p = e.parentElement;
      let hops = 0;
      while (p && hops < 3) { const pr = p.getBoundingClientRect(); if (Math.abs(pr.width - 48) < 1) { out['rail_tile_48'] = fr(p); break; } p = p.parentElement; hops++; }
    }
  }
  // sidebar header row containing 'Cassette Demo 88785'
  for (const e of document.querySelectorAll('span')) {
    const t = (e.textContent || '').trim();
    if (t.startsWith('Cassette Demo') && !out['hdr_label']) out['hdr_label'] = fr(e);
  }
  for (const k of ['weekly-picks', 'liner-notes', 'MIXTAPES', 'CRATE']) {
    for (const e of document.querySelectorAll('span')) {
      const t = (e.textContent || '').trim();
      if (t.includes(k)) { out['row_' + k] = fr(e); break; }
    }
  }
  // user panel: 'Avery North' in bottom half, and the icon buttons (18px divs)
  for (const e of document.querySelectorAll('span')) {
    const t = (e.textContent || '').trim();
    if (t.startsWith('Avery') && e.getBoundingClientRect().top > 400) out['user_name'] = fr(e);
  }
  const btn = [];
  for (const e of document.querySelectorAll('div')) { const r = e.getBoundingClientRect(); if (Math.abs(r.width-18)<1 && Math.abs(r.height-18)<1 && r.left>0) btn.push(fr(e)); }
  out['user_icon_btns'] = btn.slice(0, 5);
  const scw = document.documentElement ? document.documentElement.scrollWidth : 0;
  return JSON.stringify({ scw: scw, vw: window.innerWidth, els: out });
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
            print(f"  {k}: {val}")

asyncio.run(go())
