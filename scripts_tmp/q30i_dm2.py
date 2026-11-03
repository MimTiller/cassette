import sqlite3
conn = sqlite3.connect('/root/cassette-v100/discord.db')
cur = conn.cursor()
bychan = {}
for ch, u in cur.execute("SELECT dm_channel_id, user_id FROM dm_participants").fetchall() if cur.execute("SELECT 1 WHERE 0") else cur.execute("SELECT channel_id, user_id FROM dm_participants").fetchall():
    bychan.setdefault(ch, []).append(u)
for ch, us in sorted(bychan.items()):
    if 367 in us or 368 in us:
        n = cur.execute("SELECT count(*) FROM dm_messages WHERE channel_id=?", (ch,)).fetchone()[0]
        if n:
            print(ch, us, n, cur.execute("SELECT author_id, content FROM dm_messages WHERE channel_id=?", (ch,)).fetchall())
print("Avery friends:", cur.execute("SELECT * FROM friendships WHERE min_user_id=367 OR max_user_id=367").fetchall())
