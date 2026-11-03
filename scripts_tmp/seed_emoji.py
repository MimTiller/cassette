import sqlite3
db = sqlite3.connect('/root/cassette-v100/discord.db')
cur = db.cursor()
now = 1793388801428
# A custom-emoji reaction on the "hot single" message (348) by user 368,
# and an emoji-only custom-emoji message at the end of #general (217).
cur.execute("INSERT INTO message_reactions (message_id, user_id, emoji, created_at) VALUES (348, 368, '<:casspop:1>', ?1)", (now + 100,))
cur.execute("INSERT INTO messages (channel_id, author_id, content, created_at) VALUES (217, 368, '<:casspop:1> <:casspop:1> \U0001F97F', ?1)", (now + 2000,))
db.commit()
print(list(cur.execute("SELECT id, content FROM messages WHERE channel_id=217 ORDER BY id DESC LIMIT 3")))
print(list(cur.execute("SELECT message_id, user_id, emoji FROM message_reactions WHERE message_id=348")))
