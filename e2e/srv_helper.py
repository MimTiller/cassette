"""Tiny /_srv RPC helper shared by the e2e tests: seed accounts and server
data directly (the UI checks still assert on what the browser paints)."""
import json
import urllib.request


def srv(base, fn, inp, token=None):
    body = [] if inp is None else [inp]
    req = urllib.request.Request(
        f"{base}/_srv/{fn}", data=json.dumps(body).encode(), method="POST",
        headers={"content-type": "application/json"})
    if token:
        req.add_header("Authorization", "Bearer " + token)
    out = json.load(urllib.request.urlopen(req, timeout=30))
    if isinstance(out, dict) and ("Err" in out or "Failed" in out):
        raise RuntimeError(f"{fn}: {out.get('Err') or out.get('Failed')}")
    return out.get("Ok", out) if isinstance(out, dict) else out


def signup_login(base, name, pw):
    srv(base, "signup", {"username": name, "password": pw})
    return srv(base, "login", {"username": name, "password": pw})
