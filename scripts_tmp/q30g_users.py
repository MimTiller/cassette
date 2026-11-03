import sqlite3, json
c = sqlite3.connect('/root/cassette-v100/discord.db')
users = [(r[1], r[0]) for r in c.execute("select id,username from users where username like 'pol%'")]
print(users)
try:
    print(list(c.execute("select id,name,avatar_url from users where username in ('polA88785','polB88785','polC88785','polD88785')")))
except Exception as e:
    print("avatar cols err", e)
