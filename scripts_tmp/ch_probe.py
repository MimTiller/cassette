import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
cur = db.cursor()
for cid in [r[0] for r in cur.execute("SELECT id FROM channels WHERE server_id=168")]:
    print(cid, list(cur.execute("SELECT id, author_id, content FROM messages WHERE channel_id = ?", (cid,))))
