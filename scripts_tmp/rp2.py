import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
cur = db.cursor()
names = [r[0] for r in cur.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name LIKE '%react%'")]
print("react tables:", names)
for t in names:
    cols = [d[1] for d in cur.execute("SELECT * FROM " + t + " LIMIT 0").description]
    print(t, cols, cur.execute("SELECT COUNT(*) FROM " + t).fetchall())
    for row in cur.execute("SELECT * FROM " + t + " LIMIT 20"):
        print(row)
