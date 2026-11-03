"""Run every e2e/test_*.py in order against a base URL; print ok/KNOWN/FAIL.

Usage: run.py [base_url] [--timeout SECS]
Default base: http://127.0.0.1:8080. Screenshots land in /tmp/cassette-e2e, outside the repo: the dev server
watches the repo, so a file written inside it reloads the app mid-test.
Each test's overall timeout comes from its top-level `TIMEOUT = N` line
(seconds), else --timeout. The suite exits non-zero on any failure that is not
marked known_bug by the test.
"""
import glob
import json
import os
import re
import subprocess
import sys
import time
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))


def close_stray_tabs():
    """The shared CDP browser must not accumulate tabs; close every page tab
    still open when the suite finishes."""
    try:
        tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9341/json/list"))
    except Exception:
        return
    for t in tabs:
        if t.get("type") == "page":
            try:
                urllib.request.urlopen(f"http://127.0.0.1:9341/json/close/{t['id']}")
            except Exception:
                pass


def main():
    argv = sys.argv[1:]
    default_timeout = 120
    if "--timeout" in argv:
        i = argv.index("--timeout")
        default_timeout = int(argv[i + 1])
        del argv[i:i + 2]
    positional = [a for a in argv if not a.startswith("--")]
    base = positional[0] if positional else "http://127.0.0.1:8080"
    out = "/tmp/cassette-e2e"
    os.makedirs(out, exist_ok=True)

    tests = sorted(glob.glob(os.path.join(HERE, "test_*.py")))
    if not tests:
        print("no tests found in e2e/")
        return 1

    grand_ok = grand_known = grand_fail = 0
    failures = []
    started = time.time()
    for path in tests:
        name = os.path.basename(path)
        m = re.search(r"^TIMEOUT\s*=\s*(\d+)", open(path).read(), re.M)
        per = int(m.group(1)) if m else default_timeout
        print(f"== {name}: running (timeout {per}s) ==", flush=True)
        t0 = time.time()
        crashed = False
        try:
            proc = subprocess.run([sys.executable, "-B", path, base, out],
                                  capture_output=True, text=True, timeout=per)
            out_text, err_text, crashed = proc.stdout, proc.stderr, proc.returncode != 0
        except subprocess.TimeoutExpired as e:
            out_text = (e.stdout or b"").decode(errors="replace") if isinstance(e.stdout, bytes) else (e.stdout or "")
            err_text = (e.stderr or b"").decode(errors="replace") if isinstance(e.stderr, bytes) else (e.stderr or "")
            crashed = True
            out_text += "\n  ** test TIMED OUT **"
        print(out_text.rstrip(), flush=True)
        if err_text.strip():
            print("  stderr:", err_text.strip()[:1500], flush=True)

        payload = None
        for line in out_text.splitlines():
            if line.startswith("RESULTS "):
                try:
                    payload = json.loads(line[len("RESULTS "):])
                except json.JSONDecodeError:
                    pass
        if payload is None:
            grand_fail += 1
            failures.append(f"{name}: no RESULTS line (crash or timeout)")
            print(f"== {name}: FAIL (no results) ==", flush=True)
            continue
        res, known = payload["results"], set(payload.get("known", []))
        ok = sum(1 for v in res.values() if v)
        kn = sum(1 for k, v in res.items() if not v and k in known)
        bad = [k for k, v in res.items() if not v and k not in known]
        grand_ok, grand_known, grand_fail = grand_ok + ok, grand_known + kn, grand_fail + len(bad)
        for k in bad:
            failures.append(f"{name}: {k}")
        aborted = "!! test aborted" in out_text or not res
        if aborted and not bad:
            grand_fail += 1
            failures.append(f"{name}: aborted before its checks finished")
        status = "FAIL" if (bad or crashed or aborted) else "ok"
        print(f"== {name}: {status} — {ok} ok, {kn} known, {len(bad)} failed in {time.time() - t0:.0f}s ==", flush=True)

    close_stray_tabs()
    print(f"\nSUMMARY: {grand_ok} checks ok, {grand_known} KNOWN bugs, {grand_fail} failed "
          f"across {len(tests)} tests in {time.time() - started:.0f}s")
    for f in failures:
        print("FAILED:", f)
    return 1 if grand_fail else 0


if __name__ == "__main__":
    sys.exit(main())
