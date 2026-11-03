import sqlite3
con = sqlite3.connect("/root/cassette-v100/discord.db")
cur = con.cursor()
names = [r[0] for r in cur.execute("SELECT name FROM sqlite_objects WHERE kind='table'")] if False else [r[0] for r in cur.execute("SELECT name FROM sqlite_temp_schema WHERE kind='table'")]
print("TABLES:", names)
con.close()
