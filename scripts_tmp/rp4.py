import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
cur = db.cursor()
print([d for d in cur.execute("SELECT * FROM sqlite_schema WHERE name='message_reactions'")])
