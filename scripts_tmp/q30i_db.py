import sqlite3
conn = sqlite3.connect('/root/cassette-v100/discord.db')
cur = conn.cursor()
print("server 168 channels:", cur.execute("SELECT id, name, server_id, kind FROM channels WHERE server_id=168").fetchall())
for cid, name in cur.execute("SELECT id, name FROM channels WHERE server_id=168").fetchall():
    n = cur.execute("SELECT count(*) FROM messages WHERE channel_id=?", (cid,)).fetchone()[0]
    print(cid, name, "msgs:", n)
print("users pol:", cur.execute("SELECT id, username FROM users").fetchall()[-8:])
print("dms:", cur.execute("SELECT id, name, participants FROM dm_channels LIMIT 10").fetchall())
