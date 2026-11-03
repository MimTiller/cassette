# Split the working-tree diff into item1 (avatar shape) and item2 (presence) patches.
import subprocess, re

out = subprocess.run(["git", "diff", "HEAD", "--", "discord-clone"], capture_output=True, text=True, cwd="/root/cassette-v100")
diff = out.stdout

# Split into per-file chunks
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

def is_presence(chunk):
    body = [l for l in chunk if l and l[0] in "+-" and not l.startswith(("+++", "---"))]
    text = "".join(body).lower()
    path = "".join(l for l in chunk if l.startswith("+++ "))
    if "avatar_chip.rs" in path:
        return False
    if "avatar_chip.rs" in path:
        # helper-shape hunks are item1; presence-dot machinery is item2
        return any(k in text for k in ["presence_dot", "presence_status_role", "presence constants", "status menu", " hollow"]) or any(k in "".join(body) for k in ["PRESENCE_"])
    return any(k in text for k in ["presence", "dot", "opacity", "status_str", "status_role"])

p1, p2 = [], []
for chunk in files:
    header = [l for l in chunk if l.startswith(("diff --git", "index ", "+++", "---", "@@")) or l.startswith("    ") is False and l and l[0] not in "+- "]
    # keep everything as-is but route whole file-chunks; for mixed files split hunks
    # split into preamble + hunks
    pre = []
    hunks = []
    i = 0
    while i < len(chunk) and not chunk[i].startswith("@@"):
        pre.append(chunk[i]); i += 1
    curh = None
    for l in chunk[i:]:
        if l.startswith("@@"):
            if curh: hunks.append(curh)
            curh = [l]
        else:
            curh.append(l)
    if curh: hunks.append(curh)
    h1 = [h for h in hunks if not is_presence(h)]
    h2 = [h for h in hunks if is_presence(h)]
    if h1: p1.append(("".join(pre), h1))
    if h2: p2.append(("".join(pre), h2))

def write(patch, name):
    txt = "".join(pre + "".join(h) for pre, hs in patch for h in hs)
    open(f"/root/cassette-v100/scripts_tmp/{name}", "w").write(txt)
    # count hunks
    return txt.count("@@")

n1 = write(p1, "q30g_item1.patch")
n2 = write(p2, "q30g_item2.patch")
print(f"item1 hunks={n1} item2 hunks={n2}")
