import asyncio, json, urllib.request, websockets

PORT = 9341

async def go():
    tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/json/list"))
    tab = next(t for t in tabs if "127.0.0.1:8085" in t.get("url", ""))
    async with websockets.connect(tab["webSocketDebuggerUrl"], max_size=None) as ws:
        n = 0

        async def cdp(method, **params):
            nonlocal n
            n += 1
            await ws.send(json.dumps({"id": n, "method": method, "params": params}))
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=15))
                if msg.get("id") == n:
                    return msg

        expr = """
        (() => {
          const out = [];
          const els = document.querySelectorAll('*');
          for (const el of els) {
            const st = el.getAttribute('style') || '';
            if (st.includes('border-radius:6px')) {
              const r = el.getBoundingClientRect();
              out.push([Math.round(r.width), Math.round(r.height), el.getAttribute('style')]);
            }
          }
          return JSON.stringify(out.slice(0,10));
        })()
        """
        r = await cdp("Runtime.evaluate", expression=expr, returnByValue=True)
        val = r.get("result", {}).get("result", {}).get("value")
        print(val)

asyncio.run(go())
