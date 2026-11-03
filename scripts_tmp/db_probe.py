import sqlite3

conn = sqlite3.connect('/root/cassette-v100/discord.db')
cur = conn.cursor()
for cid in [244, 245]:
    try:
        rows = cur.execute("SELECT id, author_id, content, created_at FROM messages WHERE channel_id = " + str(cid) + " ORDER BY created_at").fetchall()
        print(cid, len(rows))
        for r in rows[:15]:
            print(r)
    except Exception as e:
        print(cid, e)
