"""Shared CDP browser helpers for the e2e suite (see run.py for how tests use it).

Every test drives Chrome through the DevTools protocol (CDP endpoint
http://127.0.0.1:9341). One Session owns one browser tab: open() attaches it,
close() detaches it, and tests call close() from a finally block so the shared
browser never accumulates tabs. Unlike the old one-off scripts in
/root/discord-ref/tools:

- cdp() bounds every round-trip with asyncio.wait_for and raises naming the
  method that timed out (a wedged call used to hang a script forever).
- wait_for() polls a JS condition instead of fixed sleeps.
- text lookups and clicks are paint-tested with elementFromPoint: the app
  builds its ~30 overlays/modals once and collapses them, so labels like
  "Create" exist in the DOM many times with live rects, and a rect-only
  filter picks a hidden button and clicks air.

Element finding is by VISIBLE text, input placeholders and position; test_id
values from the Rust code are not rendered into the DOM.
"""
import asyncio
import os
import base64
import json
import urllib.request
import websockets

CDP_HTTP = "http://127.0.0.1:9341"
CDP_TIMEOUT = 15.0
CLICK_WAIT = 0.35


class CDPTimeout(Exception):
    pass


class CDPError(Exception):
    pass


class WaitTimeout(Exception):
    pass


class MissingElement(Exception):
    pass


def _paint_js(elem, W, H, xmin=-1.0, ymin=-1.0):
    """JS predicate: elem's centre is on-screen and nothing opaque paints over
    it. Collapsed overlays sit off-viewport (their rects land in negative or
    beyond-edge coordinates), and open modals have opaque backgrounds — a
    transparent empty container over the point does not hide the text."""
    return (f"(() => {{ const r = {elem}.getBoundingClientRect(); "
            f"if (!(r.width > 0 && r.height > 0 && r.x > {xmin} && r.y > {ymin})) return false; "
            f"const x = r.x + r.width / 2, y = r.y + r.height / 2; "
            f"if (!(x >= 0 && x < {W} && y >= 0 && y < {H})) return false; "
            f"if (getComputedStyle({elem}).visibility === 'hidden') return false; "
            f"const hit = document.elementFromPoint(x, y); "
            f"if (!hit) return false; "
            f"if (hit === {elem} || {elem}.contains(hit) || hit.contains({elem})) return true; "
            f"const hs = getComputedStyle(hit); "
            f"return hs.visibility === 'visible' && hs.backgroundColor === 'rgba(0, 0, 0, 0)' && "
            f"parseFloat(hs.opacity) > 0 && hit.textContent.trim() === ''; }})()")


class Session:
    def __init__(self, base, shots, W=1600, H=900):
        self.base, self.shots, self.W, self.H = base, shots, W, H
        self.tab_id = None
        self._ws = None
        self._n = 0
        self._events = []
        self.results = {}
        self.known = set()

    async def open(self):
        tab = json.load(urllib.request.urlopen(
            urllib.request.Request(f"{CDP_HTTP}/json/new?about:blank", method="PUT")))
        self.tab_id = tab["id"]
        self._ws = await websockets.connect(tab["webSocketDebuggerUrl"], max_size=None)
        await self.cdp("Emulation.setDeviceMetricsOverride",
                       width=self.W, height=self.H, deviceScaleFactor=1, mobile=False)
        await self.cdp("Network.clearBrowserCookies")

    async def close(self):
        """Close the tab even if the caller crashed (call from a finally)."""
        try:
            if self._ws:
                await self._ws.close()
        except Exception:
            pass
        if self.tab_id:
            try:
                urllib.request.urlopen(f"{CDP_HTTP}/json/close/{self.tab_id}")
            except Exception:
                pass

    async def cdp(self, method, **params):
        self._n += 1
        my = self._n
        await self._ws.send(json.dumps({"id": my, "method": method, "params": params}))
        while True:
            try:
                msg = json.loads(await asyncio.wait_for(self._ws.recv(), timeout=CDP_TIMEOUT))
            except asyncio.TimeoutError:
                raise CDPTimeout(f"CDP call {method} got no reply within {CDP_TIMEOUT}s")
            if "method" in msg:
                self._events.append(msg)
                continue
            if msg.get("id") != my:
                continue
            if "error" in msg:
                raise CDPError(f"CDP {method} failed: {msg['error']}")
            return msg.get("result", {})

    async def ev(self, js):
        r = await self.cdp("Runtime.evaluate", expression=js,
                           returnByValue=True, awaitPromise=True)
        res = r.get("result", {})
        if "exceptionDetails" in res:
            d = res["exceptionDetails"]
            msg = d.get("text") or d.get("exception", {}).get("description") or d.get("message") or "?"
            raise CDPError(f"JS threw: {msg}")
        if res.get("type") == "undefined":
            return None
        return res.get("value")

    async def click(self, x, y):
        x = min(max(x, 1.0), self.W - 2.0)
        y = min(max(y, 1.0), self.H - 2.0)
        await self.cdp("Input.dispatchMouseEvent", type="mouseMoved", x=x, y=y)
        for kind in ("mousePressed", "mouseReleased"):
            await self.cdp("Input.dispatchMouseEvent", type=kind, x=x, y=y,
                           button="left", clickCount=1, buttons=1)
        await asyncio.sleep(CLICK_WAIT)

    async def key(self, k, code=None, vk=None):
        code = code or k
        if vk is None:
            vk = ord(k[0]) if len(k) == 1 else 0
        down = {"type": "keyDown", "key": k, "code": code,
                "windowsVirtualKeyCode": vk}
        if k == "Enter":
            down["text"] = "\r"
        await self.cdp("Input.dispatchKeyEvent", **down)
        await self.cdp("Input.dispatchKeyEvent", type="keyUp", key=k,
                       code=code, windowsVirtualKeyCode=vk)
        await asyncio.sleep(0.15)

    async def enter(self):
        await self.key("Enter", vk=13)

    async def escape(self):
        await self.key("Escape", vk=27)

    async def center(self, js_el):
        """Viewport-painted centre of the element this JS names, else None."""
        return await self.ev(
            f"(() => {{ const e = {js_el}; if (!e) return null; const r = "
            f"e.getBoundingClientRect(); if (!(r.width > 0 && r.x >= 0 && r.y >= 0 && r.x < {self.W} && r.y < {self.H})) return null; "
            f"return [r.x + r.width / 2, r.y + r.height / 2]; }})()")

    def by_text(self, t, xmin=-1.0, xmax=None, ymin=-1.0, ymax=None):
        """JS expr: leaf element with exactly this text inside the box."""
        xmax = int(xmax) if xmax is not None else self.W
        ymax = int(ymax) if ymax is not None else self.H
        return (f"[...document.querySelectorAll('*')].reverse()"
                f".find(e => {{ const r = e.getBoundingClientRect(); return "
                f"e.children.length === 0 && e.textContent.trim() === {json.dumps(t)} && r.width > 0 "
                f"&& r.x > {xmin} && r.x < {xmax} && r.y > {ymin} && r.y < {ymax}; }})")

    async def visible(self, t, xmin=-1.0, xmax=None, ymin=-1.0, ymax=None):
        """True when t is painted in the box (an open panel), not a hidden one."""
        xmax = int(xmax) if xmax is not None else self.W
        ymax = int(ymax) if ymax is not None else self.H
        return await self.ev(
            f"[...document.querySelectorAll('*')].reverse().some(e => e.textContent.trim() === {json.dumps(t)} && "
            + _paint_js("e", xmax, ymax, xmin, ymin) + ")")

    async def text_pos(self, t, xmin=-1.0, xmax=None, ymin=-1.0, ymax=None):
        xmax = int(xmax) if xmax is not None else self.W
        ymax = int(ymax) if ymax is not None else self.H
        return await self.ev(
            f"(() => {{ const e = [...document.querySelectorAll('*')].reverse().find(e => e.textContent.trim() === {json.dumps(t)} && "
            + _paint_js("e", xmax, ymax, xmin, ymin) + f"); if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; }})()")

    async def text_pos_regex(self, pattern, xmin=-1.0, xmax=None, ymin=-1.0, ymax=None):
        """Viewport-painted centre of the leaf whose text matches this regex."""
        xmax = int(xmax) if xmax is not None else self.W
        ymax = int(ymax) if ymax is not None else self.H
        return await self.ev(
            f"(() => {{ const e = [...document.querySelectorAll('*')].reverse().find(e => e.children.length === 0 && RegExp({json.dumps(pattern)}, 'u').test(e.textContent.trim()) && "
            + _paint_js("e", xmax, ymax, xmin, ymin) + f"); if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; }})()")

    async def click_text(self, t, xmin=-1.0, xmax=None, ymin=-1.0, ymax=None, tries=6):
        for i in range(tries):
            p = await self.text_pos(t, xmin, xmax, ymin, ymax)
            if p:
                await self.click(*p)
                return True
            if i < tries - 1:
                await asyncio.sleep(0.8)
        return False

    async def click_starts(self, prefix, xmin=-1.0, xmax=None, ymin=-1.0, ymax=None):
        """Click the painted leaf whose text starts with prefix (wrapped rows)."""
        xmax = xmax if xmax is not None else self.W
        ymax = ymax if ymax is not None else self.H
        p = await self.ev(
            f"(() => {{ const e = [...document.querySelectorAll('*')].reverse().find(e => e.children.length === 0 && e.textContent.trim().startsWith({json.dumps(prefix)}) && "
            + _paint_js("e", xmax, ymax, xmin, ymin) + f"); if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; }})()")
        if not p:
            return False
        await self.click(*p)
        return True

    def painted(self, t, xmin=-1.0, xmax=None, ymin=-1.0, ymax=None):
        """JS condition string for visible(t), for wait_for."""
        xmax = xmax if xmax is not None else self.W
        ymax = ymax if ymax is not None else self.H
        return (f"[...document.querySelectorAll('*')].reverse().some(e => e.textContent.trim() === {json.dumps(t)} && "
                + _paint_js("e", xmax, ymax, xmin, ymin) + ")")

    def not_painted(self, t):
        return "!" + self.painted(t)

    def input_exact(self, placeholder):
        """JS expr: painted input whose placeholder is exactly this."""
        return (f"[...document.querySelectorAll('input')].find(i => (i.placeholder || '') === {json.dumps(placeholder)} && "
                + _paint_js("i", self.W, self.H) + ")")

    def input_startswith(self, prefix):
        return (f"[...document.querySelectorAll('input')].find(i => (i.placeholder || '').startsWith({json.dumps(prefix)}) && "
                + _paint_js("i", self.W, self.H) + ")")

    async def has_input(self, js_input):
        return await self.ev(f"!!{js_input}")

    async def input_value(self, js_input):
        return await self.ev(f"(() => {{ const i = {js_input}; return i ? i.value : null; }})()")

    async def type_into(self, js_input, text, select=True):
        p = await self.center(js_input)
        if not p:
            return False
        await self.click(*p)
        if select:
            await self.ev(f"{js_input}.select()")
        await self.cdp("Input.insertText", text=text)
        await asyncio.sleep(0.3)
        return True

    async def type_ph(self, placeholder, text, wait=8):
        js = self.input_exact(placeholder)
        if not await self.until(f"!!{js}", timeout=wait, why=f"input '{placeholder}'"):
            return False
        return await self.type_into(js, text)

    async def type_ph_start(self, prefix, text, select=True, wait=8):
        js = self.input_startswith(prefix)
        if not await self.until(f"!!{js}", timeout=wait, why=f"input '{prefix}…'"):
            return False
        return await self.type_into(js, text, select)

    async def wait_for(self, js_condition, timeout=30, step=0.5, why=""):
        end = asyncio.get_running_loop().time() + timeout
        while True:
            v = await self.ev(js_condition)
            if v:
                return v
            if asyncio.get_running_loop().time() > end:
                raise WaitTimeout(f"condition never became true within {timeout}s ({why or js_condition[:90]})")
            await asyncio.sleep(step)

    async def until(self, js_condition, timeout=30, step=0.5, why=""):
        """wait_for as a boolean: True if it happened, False if it timed out."""
        try:
            await self.wait_for(js_condition, timeout, step, why)
            return True
        except WaitTimeout:
            return False

    async def shot(self, name):
        # A background tab in the shared browser never answers captureScreenshot.
        try:
            await self.cdp("Page.bringToFront")
        except Exception:
            pass
        self._n += 1
        my = self._n
        await self._ws.send(json.dumps({"id": my, "method": "Page.captureScreenshot",
                                        "params": {"format": "png"}}))
        data = None
        while True:
            try:
                msg = json.loads(await asyncio.wait_for(self._ws.recv(), timeout=40.0))
            except asyncio.TimeoutError:
                raise CDPTimeout("Page.captureScreenshot got no reply within 40s")
            if "method" in msg:
                self._events.append(msg)
                continue
            if msg.get("id") != my:
                continue
            if "error" in msg:
                raise CDPError(f"CDP Page.captureScreenshot failed: {msg['error']}")
            data = msg.get("result", {})
            break
        os.makedirs(self.shots, exist_ok=True)
        path = f"{self.shots}/{name}"
        with open(path, "wb") as f:
            f.write(base64.b64decode(data["data"]))
        return path

    async def fresh_page(self):
        """Empty cookie/storage state: a clean login screen (cleared cookies).

        The WASM bundle boots at its own pace, so wait until the screen paints.
        """
        await self.cdp("Network.clearBrowserCookies")
        await self.cdp("Network.clearBrowserCache")
        await self.ev("localStorage.clear(); sessionStorage.clear()")
        await self.cdp("Page.navigate", url=self.base + "/")
        await asyncio.sleep(2)
        await self.ev("localStorage.clear(); sessionStorage.clear(); location.reload()")
        await self.wait_for(self.painted("Welcome back!"), timeout=60, step=1,
                            why="login screen painted")

    def composer_live(self):
        return f"!!{self.input_startswith('Message')}"

    def app_live(self):
        """Logged-in shell is live: the rail's presence line or the settings sidebar paints."""
        return f"({self.painted('Online')} || {self.painted('USER SETTINGS', xmax=int(self.W * 0.2))})"

    # ---- auth flows ----------------------------------------------------------

    async def signup(self, user, pw):
        """Sign up through the login screen; wait until the app shell is live."""
        await self.wait_for(self.painted("Sign Up", ymin=200, ymax=420), timeout=30, step=1,
                            why="Sign Up tab painted")
        if not await self.click_text("Sign Up", ymin=200, ymax=420):
            raise MissingElement("Sign Up tab not painted on the login screen")
        for ph in ("username", "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022", "type it again"):
            if not await self.type_ph(ph, user if ph == "username" else pw):
                raise MissingElement(f"signup field '{ph}' not painted")
        if not await self.click_text("Create Account", ymin=400):
            raise MissingElement("Create Account button not painted")
        await self.wait_for(self.app_live(), timeout=60, step=1,
                            why="app shell live after sign-up")

    async def login(self, user, pw):
        await self.type_ph("username", user)
        await self.type_ph("\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022", pw)
        if not await self.click_text("Log In", ymin=450):
            raise MissingElement("Log In submit button not painted")
        await self.wait_for(self.app_live(), timeout=60, step=1,
                            why="app shell live after log-in")

    async def open_user_settings(self):
        """User panel avatar popup -> Edit Profile -> the User Settings screen."""
        pos = await self.ev(f"(() => {{ const e = [...document.querySelectorAll('div')].find(e => {{ const r = e.getBoundingClientRect(); return Math.abs(r.width - 32) <= 6 && Math.abs(r.height - r.width) <= 2 && r.x < 300 && r.y > {self.H - 90}; }}); if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; }})()")
        if not pos:
            raise MissingElement("user panel avatar circle not found")
        await self.click(*pos)
        await asyncio.sleep(1.2)
        if not await self.click_text("Edit Profile", xmax=self.W * 0.35):
            raise MissingElement("'Edit Profile' popup row not painted")
        await self.wait_for(self.painted("USER SETTINGS", xmax=self.W * 0.2),
                            timeout=15, why="user settings sidebar")

    async def log_out(self):
        """Sign out via the User Settings sidebar row, back to the login screen.

        Assumes User Settings is already open (does not re-open it).
        """
        if not await self.click_text("Log Out", xmax=self.W * 0.2):
            raise MissingElement("Log Out row not painted in the settings sidebar")
        await self.wait_for(self.painted("Welcome back!"), timeout=40, step=1,
                            why="login screen after sign-out")

    # ---- rail / servers / channels -------------------------------------------

    async def open_create_server_modal(self):
        """Click the rail's green plus (rail-column svgs, bottom up) until it
        opens. The rail moves as servers are added, so probe every icon."""
        svgs = await self.ev("[...document.querySelectorAll('svg')].map(s => s.getBoundingClientRect()).filter(r => r.width > 0 && r.x < 80 && r.y > 0).map(r => [r.x + r.width/2, r.y + r.height/2])")
        for x, y in sorted((p for p in svgs), key=lambda a: -a[1]):
            await self.click(x, y)
            if await self.until(self.painted("Add a Server"), timeout=6,
                                step=0.7, why="create modal"):
                return True
            await self.escape()
        return False

    async def create_server(self, name):
        if not await self.open_create_server_modal():
            raise MissingElement("could not open the create-server modal")
        if not await self.type_ph("Server name", name):
            raise MissingElement("Server name field not painted")
        if not await self.click_text("Create", ymin=self.H * 0.3):
            raise MissingElement("Create button not painted in the modal")
        await self.wait_for(self.painted(name), timeout=30, why=f"server '{name}' painted")

    async def open_server_menu(self, name):
        """Ensure the server's dropdown menu is open (Escape closes one that
        was already up, so clicking the header afterwards cannot toggle it off).
        """
        await self.escape()
        await asyncio.sleep(0.4)
        for _ in range(3):
            if not await self.click_text(name, xmax=self.W * 0.45):
                raise MissingElement(f"server header '{name}' not painted")
            await asyncio.sleep(1.2)
            if await self.visible("Mute Server", xmin=60, xmax=self.W * 0.5):
                return
            await self.escape()
            await asyncio.sleep(0.4)
        raise MissingElement("server dropdown never opened")

    async def reload_page(self, wait_shell=60):
        """Reload the tab (keeps cookies): re-runs the app boot, which re-enters
        the saved last server and fetches its permissions."""
        await self.cdp("Page.navigate", url=self.base + "/")
        await self.wait_for(self.app_live(), timeout=wait_shell, step=1,
                            why="shell live after reload")
        await asyncio.sleep(2)

    async def click_menu_item(self, label):
        return await self.click_text(label, xmin=60, xmax=self.W * 0.5)

    async def create_channel(self, name):
        """In an open New Channel modal: name it and press Create."""
        if not await self.type_ph("Channel name", name):
            raise MissingElement("Channel name field not painted")
        if not await self.click_text("Create", ymin=self.H * 0.3):
            raise MissingElement("Create button not painted in the channel modal")
        await self.wait_for(self.painted(name), timeout=30, why=f"channel '{name}' painted")

    async def mint_invite(self, name=None):
        """Open the invite overlay and return the minted invite link.

        Opens it from the channel header's user-plus icon. (The server menu's
        Invite People row was unreliable here; `name` is kept for callers.)
        """
        opened = False
        if not opened:
            icons = await self.ev("[...document.querySelectorAll('svg')].map(s => s.getBoundingClientRect()).filter(r => r.width > 0 && r.y < 60 && r.x > 300 && r.x < 1400).map(r => [r.x + r.width/2, r.y + r.height/2])")
            # Header icons left to right: pins, invite (user-plus), members.
            for x, y in sorted((p for p in icons), key=lambda a: a[0]):
                await self.click(x, y)
                if await self.until(self.painted("Create invite"), timeout=6,
                                    step=0.7, why="invite overlay"):
                    opened = True
                    break
                await self.escape()
        if not opened:
            raise MissingElement("invite overlay never opened")
        # The modal opens in its Create arm: click Create, then the card
        # ("INVITE CODE" header + a read-only input holding the bare code)
        # paints. No full URL text appears anywhere.
        # The live overlay's button reads "Create invite" (overlays.rs); the
        # older modal said "Create".
        if not (await self.click_text("Create invite", tries=3) or await self.click_text("Create", tries=3)):
            await self.shot("invite-modal-no-create.png")
            raise MissingElement("Create button not painted in the invite modal")
        await self.wait_for(self.painted("INVITE CODE"), timeout=20,
                            why="invite card painted")
        code = await self.wait_for(
            "(() => { const i = [...document.querySelectorAll('input')].find(i => (i.placeholder || '') === '' && /^[A-Za-z0-9]{4,16}$/.test(i.value) && " + _paint_js("i", self.W, self.H) + "); return i ? i.value : null; })()",
            timeout=10, why="invite code input")
        if not code:
            raise MissingElement("invite code not painted")
        return code

    async def join_via_code(self, code):
        """Join an existing server: create modal -> join link -> redeem code."""
        if not await self.open_create_server_modal():
            raise MissingElement("could not open the create-server modal to reach Join")
        if not await self.click_starts("Have an invite already", ymax=self.H * 0.9):
            raise MissingElement("the create modal's join link is not painted")
        if not await self.until(self.painted("Join a Server"), timeout=10,
                                why="Join a Server modal (title)"):
            raise MissingElement("Join a Server modal never opened")
        if not await self.type_ph("Invite code", code):
            raise MissingElement("Invite code field not painted")
        if not await self.click_text("Join"):
            raise MissingElement("Join button not painted")
        # The modal answers "Joined! Your role is: member" with a Done button.
        joined = await self.until(
            "[...document.querySelectorAll('*')].some(e => e.children.length === 0 && "
            "e.textContent.trim().startsWith('Joined!') && e.getBoundingClientRect().width > 0)",
            timeout=30, step=1, why="Joined! confirmation")
        if not joined:
            await self.shot("join-not-confirmed.png")
            raise MissingElement("the join was never confirmed")
        await self.click_text("Done", xmin=self.W * 0.2)
        await asyncio.sleep(1.0)

    # ---- messages -------------------------------------------------------------

    async def send_message(self, text):
        """Type into the composer and submit with Enter."""
        if not await self.type_ph_start("Message", text, select=False):
            raise MissingElement("composer input not painted")
        await self.enter()
        await asyncio.sleep(1.0)
        return True

    async def message_rect(self, body):
        """Box of the message row painting this exact body leaf (centre col)."""
        return await self.ev(
            "(() => { const e = [...document.querySelectorAll('*')].find(x => x.children.length === 0 && x.textContent.trim() === %s && (() => { const r = x.getBoundingClientRect(); return r.width > 0 && r.x > 300 && r.x < 1400 && r.y > 40; })()); if (!e) return null; const r = e.getBoundingClientRect(); return [r.x, r.y, r.right, r.bottom]; })()" % json.dumps(body))

    async def hover_row(self, body):
        r = await self.message_rect(body)
        if not r:
            return False
        await self.cdp("Input.dispatchMouseEvent", type="mouseMoved",
                       x=(r[0] + r[2]) / 2, y=r[1] + 8)
        await asyncio.sleep(1.0)
        return True

    async def open_message_menu(self, body):
        """Hover the row, click the More ('⋯') pill until the menu paints."""
        if not await self.hover_row(body):
            return False
        if await self.visible("Copy text"):
            return True
        r = await self.message_rect(body)
        if not r:
            return False
        more = await self.text_pos("\u22ef", xmin=r[0] - 260, xmax=self.W)
        if not more:
            return False
        await self.click(*more)
        return await self.until(self.painted("Copy text"), timeout=5, why="message menu")

    async def edit_message(self, old, new):
        """From an open message menu: Edit -> type new text -> Save.

        True when the new content (and the (edited) marker) paint.
        """
        if not await self.click_text("Edit", ymin=0):
            return False
        if not await self.until(f"!!{self.input_exact('Edit message')}",
                                timeout=8, why="edit input"):
            return False
        if not await self.type_into(self.input_exact("Edit message"), new):
            return False
        if not await self.click_text("Save", xmin=300):
            return False
        return await self.until(self.painted(new) + f" && {self.painted('(edited)')}",
                                timeout=10, why="edited content + marker")

    async def delete_message(self, old):
        """From an open message menu: Delete -> Confirm the strip.

        True once the message text is no longer painted.
        """
        if not await self.click_text("Delete", xmin=300):
            return False
        if not await self.until(self.painted("Delete this message?"),
                                timeout=8, why="confirm strip"):
            return False
        if not await self.click_text("Confirm", xmin=300):
            return False
        return await self.until(self.not_painted(old), timeout=10, why="message gone")

    async def leaf_color(self, t):
        """Computed color of the last painted leaf with exactly this text."""
        return await self.ev(
            f"(() => {{ const l = [...document.querySelectorAll('*')].filter(e => e.children.length === 0 && e.textContent.trim() === {json.dumps(t)} && "
            + _paint_js("e", self.W, self.H) + f"); const e = l.at(-1); if (!e) return null; return getComputedStyle(e).color; }})()")

    async def pill_bg(self, t):
        """Computed background colour of the nearest painted ancestor with one.

        A mention/role pill answers with a colour; bare text answers null.
        """
        return await self.ev(
            f"(() => {{ const l = [...document.querySelectorAll('*')].filter(e => e.children.length === 0 && e.textContent.trim() === {json.dumps(t)} && "
            + _paint_js("e", self.W, self.H) + f"); let e = l.at(-1); if (!e) return null; let p = e.parentElement; while (p) {{ const bg = getComputedStyle(p).backgroundColor; if (bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') return bg; p = p.parentElement; }} return null; }})()")

    async def row_highlight_bg(self, body):
        """Background of the message row's mention rule (2px left border).

        Null means the row carries no mention highlight.
        """
        return await self.ev(
            "(() => { const e = [...document.querySelectorAll('*')].find(x => x.children.length === 0 && x.textContent.trim() === %s && (() => { const r = x.getBoundingClientRect(); return r.width > 0 && r.x > 300 && r.x < 1400; })()); let n = e; while (n) { const s = getComputedStyle(n); if (s.borderLeftWidth === '2px' && s.borderLeftStyle !== 'none') return s.backgroundColor; n = n.parentElement; } return null; })()" % json.dumps(body))

    # ---- people ----------------------------------------------------------------

    async def click_own_name(self, author):
        """Click the author's name in the member list (or a message row)."""
        if not await self.click_text(author):
            raise MissingElement(f"no painted '{author}' to click")

    # ---- settings -----------------------------------------------------------

    async def open_server_settings(self, name):
        """Open the server dropdown and press Server Settings."""
        await self.open_server_menu(name)
        if not await self.click_menu_item("Server Settings"):
            raise MissingElement("Server Settings not in the server menu")
        await self.wait_for(self.painted("Overview", xmax=self.W * 0.2),
                            timeout=15, why="server settings sidebar")

    async def settings_section(self, label, xmax_frac=0.3):
        """Click a sidebar row; verify the section's own panel paints.

        Both settings dialogs (server and user) put their sidebar rows left
        of x = 260: click there, then look for the label re-painting in the
        main content panel to the right of it.
        """
        if not await self.click_text(label, xmax=260):
            return False
        return await self.until(self.painted(label, xmin=260,
                                             xmax=int(self.W * 0.9)),
                                timeout=8, why=f"section '{label}' panel")

    async def create_role(self, name, swatch_index=5):
        """In the open Roles section: type the name, pick a colour swatch,
        press Create."""
        if not await self.type_ph("New role name", name):
            raise MissingElement("New role name input not painted")
        sw = await self.ev(
            "(() => { const i = [...document.querySelectorAll('input')].find(i => (i.placeholder || '') === 'New role name' && "
            + _paint_js("i", self.W, self.H) + "); if (!i) return null; const ir = i.getBoundingClientRect(); "
            "const c = [...document.querySelectorAll('div')].filter(d => { const r = d.getBoundingClientRect(); return Math.abs(r.width - 22) < 2 && Math.abs(r.height - 22) < 2 && r.y > ir.top - 10 && r.y < ir.bottom + 80 && r.x > ir.left - 40; }); "
            f"c.sort((a, b) => a.getBoundingClientRect().x - b.getBoundingClientRect().x); "
            f"const r = c[{swatch_index}] && c[{swatch_index}].getBoundingClientRect(); return r ? [r.x + 11, r.y + 11] : null; }})()")
        if sw:
            await self.click(*sw)
        else:
            print("  (no swatch row found for role colour)")
        if not await self.click_text("Create", ymin=self.H * 0.3):
            raise MissingElement("role Create button not painted")
        await asyncio.sleep(1.5)

    async def give_role_from_card(self, role, author):
        """Click the author's name (message view) to open the profile card,
        then press the '+ role' row; returns whether the card confirms."""
        if not await self.click_text(f"+ {role}"):
            return False
        return await self.until(self.painted(f"{role}  \u00d7"), timeout=8,
                                why="the card's role chip (name with x)")

    async def close_settings(self):
        await self.escape()
        await asyncio.sleep(0.8)

    # ---- check bookkeeping ----------------------------------------------------

    def check(self, label, ok):
        """Record a check; run.py reads the printed RESULTS line."""
        self.results[label] = bool(ok)
        print(f"  [{'ok' if ok else 'FAIL'}] {label}", flush=True)

    def known_bug(self, label, ok, why=""):
        """Record an expected app-side failure: run.py prints KNOWN, not FAIL,
        and does not count it against the suite. Add the reason to ISSUES.md.
        """
        self.results[label] = bool(ok)
        if not ok:
            self.known.add(label)
        print(f"  [{'ok' if ok else 'KNOWN'}] {label}" + (f" — {why}" if why and not ok else ""), flush=True)
