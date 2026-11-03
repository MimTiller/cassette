import sqlite3
conn = sqlite3.connect('/root/cassette-v100/discord.db')
cur = conn.cursor()
for r in cur.execute("SELECT id, channel_id, author_id, content, created_at FROM messages WHERE channel_id=217 ORDER BY created_at"):
    print(r)
print("attachments:", cur.execute("SELECT message_id, url, kind FROM message_attachments").fetchall()[:10])
print("embeds:", cur.execute("SELECT message_id FROM message_embeds").fetchall()[:10])
print("reactions:", cur.execute("SELECT message_id, user_id, shortcode FROM message_reactions").fetchall()[:12])
print("pins:", cur.execute("SELECT message_id FROM message_pins").fetchall())
