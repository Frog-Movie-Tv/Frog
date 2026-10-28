-- Shared: the TMDB entry an AniList id is, straight from Fribb's anime-lists
-- (TMDB id, and for shows TMDB's own season and offset). Placed by these
-- first; the TVDB columns stay for what only Anime-IDs knows.
ALTER TABLE anime_map ADD COLUMN tmdb_kind TEXT;
ALTER TABLE anime_map ADD COLUMN tmdb_id INTEGER;
ALTER TABLE anime_map ADD COLUMN tmdb_season INTEGER;
-- NULL when the list gives none, which is not the same as 0.
ALTER TABLE anime_map ADD COLUMN tmdb_offset INTEGER;

CREATE INDEX idx_anime_map_tmdb ON anime_map (tmdb_id, tmdb_season);

-- Load both lists on the next sync rather than in a week.
DELETE FROM anime_map_state;

-- Shared: how AniList entries follow on from each other, so a later season
-- that no list knows can be counted on from the first one.
CREATE TABLE anilist_chain (
    id         INTEGER PRIMARY KEY,
    -- The one main-series entry before it, or NULL when it is the first.
    prequel    INTEGER,
    episodes   INTEGER,
    format     TEXT,
    year       INTEGER,
    -- JSON array of its titles: English, romaji, native.
    titles     TEXT    NOT NULL DEFAULT '[]',
    fetched_at TEXT    NOT NULL
);
