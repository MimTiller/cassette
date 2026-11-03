"""copy MCP screenshots + save phone set to /root/discord-ref/polish-2026-11-02-phone/"""
import os
import shutil

src = "/root/cassette-v100/discord-clone/.idealyst/screenshots"
dst = "/root/discord-ref/polish-2026-11-02-phone"
os.makedirs(dst, exist_ok=True)
for name in [
    "Discord-Clone-1793643613876.png",  # BEFORE rail clipped
    "Discord-Clone-1793644696957.png",  # member panel bug (before fix)
    "Discord-Clone-1793645494651.png",  # settings blank (before fix)
    "Discord-Clone-1793645716312.png",  # settings fixed
    "Discord-Clone-1793646624443.png",  # settings fixed 2
    "Discord-Clone-1793646874989.png",  # List fixed 1
    "Discord-Clone-1793646922190.png",  # List fixed 2
    "Discord-Clone-1793647553545.png",  # Chat final
]:
    s = os.path.join(src, name)
    if os.path.exists(s):
        shutil.copy2(s, os.path.join(dst, name))
    else:
        print("MISSING:", s)
os.system("ls -la " + dst)