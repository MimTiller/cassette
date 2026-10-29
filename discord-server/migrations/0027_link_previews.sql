-- 0027_link_previews.sql
-- Link preview cards under messages, and a day-long cache of fetched pages
-- (see link_previews.rs). A page with nothing to show is cached with NULLs
-- so it isn't refetched.
CREATE TABLE message_embeds (
    message_id  INTEGER NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    url         TEXT    NOT NULL,
    title       TEXT,
    description TEXT,
    site_name   TEXT,
    image_path  TEXT,
    PRIMARY KEY (message_id, position)
);

CREATE TABLE link_preview_cache (
    url         TEXT    PRIMARY KEY,
    fetched_at  INTEGER NOT NULL,
    title       TEXT,
    description TEXT,
    site_name   TEXT,
    image_path  TEXT
);
