-- A manga's chapters as they were read, for the diary, Activity and Stats.
-- How far someone is stays `library_entry.progress`; these are the days the
-- count moved on. Only reads with a day are kept: a list brought in from
-- AniList for the first time is where someone is, not what they read today.
-- They belong to the library entry and leave with it.
CREATE TABLE chapter_read (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id  INTEGER NOT NULL,
    title_id INTEGER NOT NULL,
    chapter  INTEGER NOT NULL CHECK (chapter > 0),
    read_at  TEXT    NOT NULL,
    source   TEXT    NOT NULL DEFAULT 'manual',
    FOREIGN KEY (user_id, title_id) REFERENCES library_entry (user_id, title_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_chapter_read_day ON chapter_read (user_id, read_at);
CREATE INDEX idx_chapter_read_title ON chapter_read (user_id, title_id, chapter);
