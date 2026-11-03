import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
cur = db.cursor()
print("reactions tbl:", [d[0] for d in cur.execute("SELECT id,server_id,channel_id,name FROM channels WHERE server_id=168")])
import sys
rows = list(cur.execute("SELECT * FROM message_reactions"))
print("reaction rows:", rows[:20])
msgs = list(cur.execute("SELECT id,channel_id,author_id,content FROM messages WHERE content LIKE '%casspop%'"))
print("emoji msgs:", msgs)
