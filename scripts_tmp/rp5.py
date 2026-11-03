import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
cur = db.cursor()
schema = [d for d in cur.execute("SELECT * FROM sqlite_schema WHERE name='messages'")]
print(schema[0][4])
print(list(cur.execute("SELECT id,channel_id,author_id,content,created_at FROM messages WHERE channel_id=217 ORDER BY id")))
