import sqlite3, glob
for p in glob.glob('/root/cassette-v100/*.db') + glob.glob('/root/cassette-v100/discord-clone/*.db'):
    try:
        c = sqlite3.connect(p)
        rows = list(c.execute('SELECT id,name FROM servers'))
        users = list(c.execute('SELECT id,username FROM users'))
        print(p, rows, users[:8])
    except Exception as e:
        print(p, e)
