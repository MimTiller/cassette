import sqlite3
conn = sqlite3.connect('/root/cassette-v100/discord.db')
cur = conn.cursor()
cols = [d[0] for d in cur.execute("SELECT * FROM friendships LIMIT 1").description]
print("friend cols:", cols)
print("users ids:", cur.execute("SELECT id, username FROM users WHERE username IN ('polA88785','polB88785','polC88785')").fetchall())
for uid, un in cur.execute("SELECT id, username FROM users WHERE username='polA88785'").fetchall():
    print("friendships:", cur.execute("SELECT * FROM friendships WHERE userA_id=? OR userB_id=?", (uid, uid)).fetchall() if 'userA_id' in cols else cur.execute("SELECT * FROM friendships").fetchall()[:8])
print("dm parts:", cur.execute("SELECT * FROM dm_participants").fetchall()[:20])
print("dm msgs sample:", cur.execute("SELECT channel_id, author_id, content FROM dm_messages LIMIT 8").fetchall())
