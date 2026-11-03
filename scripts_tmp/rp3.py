import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
cur = db.cursor()
for mid in (1, 342, 117):
    print(list(cur.execute("SELECT channel_id, author_id, content FROM messages WHERE id = ?", (mid,))))
