import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
print("emoji:", list(db.execute("SELECT id,server_id,name,image_path FROM server_emoji")))
print("servers:", list(db.execute("SELECT id,name FROM servers")))
print("msgs with <: :", list(db.execute("SELECT id,server_id,channel_id,content FROM messages WHERE content LIKE '%<:%' ")))
