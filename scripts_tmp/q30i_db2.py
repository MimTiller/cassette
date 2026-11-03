import sqlite3
conn = sqlite3.connect('/root/cassette-v100/discord.db')
cur = conn.cursor()
rows = cur.execute("SELECT name FROM sqlite3_sql WHERE 0=1").fetchall()
tables = [r[0] for r in cur.execute("SELECT * FROM sqlite3_sql WHERE 0").fetchall()] if False else None
q = cur.execute("SELECT name FROM sqlite3_sql")
