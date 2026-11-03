"""Log out a specific account (logout_everywhere needs THAT account's bearer)."""
import json
import urllib.request

BASE = "http://127.0.0.1:8085"
PW = "polish-pass-1"


def call(fn, args, token=None):
    req = urllib.request.Request(
        f"{BASE}/_srv/{fn}",
        data=json.dumps(args).encode(),
        method="POST",
        headers={"Content-Type": "application/json"},
    )
    if token:
        req.add_header("Authorization", "Bearer " + token)
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)


if __name__ == "__main__":
    who = __import__("sys").argv[1]
    r = call("login", [{"username": who, "password": PW}])
    tok = r["Ok"]["token"]
    out = call("logout_everywhere", [], tok)
    print(f"logout for {who}:", out)
