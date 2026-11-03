"""Split custom_emoji.rs diff: hunk1(imports)+hunk2(emoji_view) = item 1; hunk3(picker) = item 3."""
import re
lines = open('/tmp/ce.diff').read().lines if False else open('/tmp/ce.diff').readlines()
header = []
hunks = []
cur = None
for ln in lines:
    if ln.startswith('diff ') or ln.startswith('--- ') or ln.startswith('+++ '):
        header.append(ln)
    elif ln.startswith('@@'):
        if cur: hunks.append(cur)
        cur = [ln]
    else:
        if cur: cur.append(ln)
if cur: hunks.append(cur)
print(len(hunks))
# classify by first added line
for i, h in enumerate(hunks):
    added = [l for l in h[1:] if l.startswith('+')]
    key = 'item1' if any('align_self' in l or 'AlignSelf' in l for l in added[:6]) and not any('picker_cell' in l for l in added) else 'item3'
    print(i, key)
open('/tmp/ce_item1.diff','w').writelines(header + hunks[0] + hunks[1])
open('/tmp/ce_item3.diff','w').writelines(header + hunks[2])
print("wrote splits")
