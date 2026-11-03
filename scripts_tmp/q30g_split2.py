# Split the q30g diff (HEAD 777eb1b vs stash tree 12577e6) into item1/item2 patches.
import subprocess

diff = subprocess.run(["git", "diff", "777eb1b", "12577e6", "--", "discord-clone"],
                     capture_output=True, text=True, cwd="/root/cassette-v100").stdout

files = []
cur = None
for line in diff.splitlines(keepends=True):
    if line.startswith("diff --git"):
        if cur:
            files.append(cur)
        cur = [line]
    else:
        if cur is None:
            continue
        cur.append(line)
if cur:
    files.append(cur)

def hunk_is_presence(hunk, path):
    body = [l for l in hunk if l and l[0] in "+-" and not l.startswith(("+++", "---"))]
    text = "".join(body).lower()
    if "avatar_chip.rs" in path:
        return False  # shared helper refactor ships with item 1
    if "collections::HashMap" in text or "objectfit" in text:
        return True   # import hunks that feed the presence wiring
    return any(k in text for k in ["presence", "opacity", "status_str", "status_role",
                                   "dot_style", "{ dot }", "inner_chip"])

p1, p2 = [], []
for chunk in files:
    pre = []
    hunks = []
    i = 0
    while i < len(chunk) and not chunk[i].startswith("@@"):
        pre.append(chunk[i]); i += 1
    path = "".join(l for l in pre if l.startswith("+++ "))
    curh = None
    for l in chunk[i:]:
        if l.startswith("@@"):
            if curh: hunks.append(curh)
            curh = [l]
        else:
            curh.append(l)
    if curh: hunks.append(curh)
    h1 = [h for h in hunks if not hunk_is_presence(h, path)]
    h2 = [h for h in hunks if hunk_is_presence(h, path)]
    if h1: p1.append(("".join(pre), h1))
    if h2: p2.append(("".join(pre), h2))

def write(patch, name):
    txt = "".join(pre + "".join(h) for pre, hs in patch for h in hs)
    open(f"/root/cassette-v100/scripts_tmp/{name}", "w").write(txt)
    return txt.count("@@")

print("item1 hunks=", write(p1, "q30g_item1.patch"), "item2 hunks=", write(p2, "q30g_item2.patch"))
