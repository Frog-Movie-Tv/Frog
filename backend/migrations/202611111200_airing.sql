-- Air times from AniList and TVmaze, matched to TMDB's own episodes
-- (crate::airtime), and the Airing page's week of every show on air
-- (crate::airing). TMDB's day is never replaced any more.
--
-- The times the previous source set are thrown away. The days it wrote over
-- TMDB's are marked 'restore', and the worker puts TMDB's back
-- (jobs::restore_tmdb_days). Its per-title table is emptied here and dropped
-- in a later release: a failed update runs the previous image on this schema,
-- and that image still reads it.
UPDATE episode SET air_at = NULL, air_source = 'restore' WHERE air_source = 'tvdb';
DELETE FROM title_airing;

-- How one TMDB show is found on AniList and TVmaze, for air times only.
-- Shared, and never a title's identity. `name` to `imdb_id` are copied from
-- TMDB whenever Frog reads the show there: `tvdb_id` and `imdb_id` are TMDB's
-- external ids, used only to find the show on TVmaze.
CREATE TABLE airtime_show (
    tmdb_id            INTEGER PRIMARY KEY,
    name               TEXT,
    original_name      TEXT,
    first_year         INTEGER,
    -- Animation from Japan: times come from AniList.
    anime              INTEGER NOT NULL DEFAULT 0,
    tvdb_id            INTEGER,
    imdb_id            TEXT,
    -- NULL with a checked_at: TVmaze was asked and has no such show.
    tvmaze_id          INTEGER,
    tvmaze_checked_at  TEXT,
    -- AniList media found by name, as a JSON list, for a show the anime map
    -- does not cover.
    anilist_ids        TEXT,
    anilist_checked_at TEXT,
    -- When this show's times were last worked out. NULL is due now.
    times_at           TEXT
);

-- Every show with an episode in the Airing window, as TMDB lists it.
-- Shared; rebuilt once a day (airing::refresh_list).
CREATE TABLE airing_show (
    tmdb_id     INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    -- Name and original name, folded (text::fold), for the page's search.
    search_name TEXT    NOT NULL,
    poster_path TEXT,
    network     TEXT,
    -- TMDB's genre names between bars: "|Animation|Drama|".
    genres      TEXT    NOT NULL DEFAULT '',
    anime       INTEGER NOT NULL DEFAULT 0,
    popularity  REAL    NOT NULL DEFAULT 0,
    listed_at   TEXT    NOT NULL
);

CREATE INDEX idx_airing_show_popularity ON airing_show (popularity DESC);

-- Episodes of those shows inside the window. `air_date` is TMDB's;
-- `air_at` the moment, UTC, when AniList or TVmaze knows it ('anilist' or
-- 'tvmaze' in `air_source`).
CREATE TABLE airing_episode (
    tmdb_id    INTEGER NOT NULL REFERENCES airing_show (tmdb_id) ON DELETE CASCADE,
    season     INTEGER NOT NULL,
    episode    INTEGER NOT NULL,
    name       TEXT,
    air_date   TEXT    NOT NULL,
    air_at     TEXT,
    air_source TEXT,
    PRIMARY KEY (tmdb_id, season, episode)
);

CREATE INDEX idx_airing_episode_day ON airing_episode (air_date);
CREATE INDEX idx_airing_episode_at ON airing_episode (air_at);

-- Films released in the window where this instance is (FROG_REGION), as
-- TMDB lists them: in cinemas (release types 2 and 3) or digital (4).
CREATE TABLE airing_film (
    tmdb_id      INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL,
    search_name  TEXT    NOT NULL,
    poster_path  TEXT,
    genres       TEXT    NOT NULL DEFAULT '',
    popularity   REAL    NOT NULL DEFAULT 0,
    release_date TEXT    NOT NULL,
    release_type INTEGER NOT NULL,
    listed_at    TEXT    NOT NULL
);

CREATE INDEX idx_airing_film_day ON airing_film (release_date);

-- When the list was last built, and when a build last started.
CREATE TABLE airing_state (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    started_at TEXT,
    listed_at  TEXT,
    shows      INTEGER NOT NULL DEFAULT 0
);
