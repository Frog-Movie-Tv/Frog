-- The stats page ranks actors, directors and studios across everything
-- watched. Credits only lived inside cached TMDB responses, which expire and
-- cannot be queried, so the few that stats need are kept here per title.
CREATE TABLE title_person (
    title_id     INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    role         TEXT    NOT NULL CHECK (role IN ('cast', 'director', 'creator', 'studio')),
    tmdb_id      INTEGER NOT NULL,
    name         TEXT    NOT NULL,
    -- A headshot for people, a logo for studios.
    profile_path TEXT,
    ord          INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (title_id, role, tmdb_id)
);

CREATE INDEX idx_title_person_person ON title_person (role, tmdb_id);

-- NULL until credits have been stored, so the worker knows which titles from
-- before this migration still need them.
ALTER TABLE title ADD COLUMN people_synced_at TEXT;
