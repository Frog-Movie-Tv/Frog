-- AniList, connected per account through AniList's own sign-in.
--
-- The token is AniList's: it lasts a year and cannot be refreshed, so the
-- expiry is kept to ask for a reconnect in time. It never leaves the server.
CREATE TABLE anilist_link (
    user_id         INTEGER PRIMARY KEY REFERENCES app_user (id) ON DELETE CASCADE,
    anilist_user_id INTEGER NOT NULL,
    anilist_name    TEXT    NOT NULL,
    token           TEXT    NOT NULL,
    expires_at      TEXT    NOT NULL,
    connected_at    TEXT    NOT NULL,
    synced_at       TEXT,
    -- Something changed in Frog since the last sync; the worker picks it up.
    dirty           INTEGER NOT NULL DEFAULT 1,
    last_error      TEXT,
    -- What the last sync did, as JSON, for Settings to show.
    last_summary    TEXT
);

-- The state parameter of a sign-in in progress, so a callback can only finish
-- a sign-in this account started. Short-lived.
CREATE TABLE anilist_oauth (
    state      TEXT    PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    created_at TEXT    NOT NULL
);

-- One row per entry on someone's AniList, and where it lives in Frog.
--
-- AniList counts every season (and every cour) as its own entry; Frog keeps
-- one show with TMDB's seasons. `season` and `episode_offset` say where the
-- entry's first episode is in TMDB's numbering, and the run continues in
-- order from there, across a season boundary if it has to.
--
-- The two snapshots are what each side looked like at the last sync. A side
-- that differs from its snapshot is the one that changed, which is how a
-- change made in either place wins over the other's stale copy.
CREATE TABLE anilist_entry (
    user_id         INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    media_id        INTEGER NOT NULL,
    name            TEXT    NOT NULL,
    format          TEXT,
    episodes        INTEGER,
    -- 'placed', 'unsure' (needs a choice) or 'ignored'.
    state           TEXT    NOT NULL DEFAULT 'unsure'
                    CHECK (state IN ('placed', 'unsure', 'ignored')),
    title_id        INTEGER REFERENCES title (id) ON DELETE SET NULL,
    season          INTEGER,
    episode_offset  INTEGER NOT NULL DEFAULT 0,
    candidates_json TEXT    NOT NULL DEFAULT '[]',
    al_status       TEXT,
    al_progress     INTEGER,
    al_score        INTEGER,
    frog_status     TEXT,
    frog_progress   INTEGER,
    frog_score      INTEGER,
    -- Removed from the Frog library after it was synced: left alone from then
    -- on rather than added back, and never deleted from AniList.
    detached        INTEGER NOT NULL DEFAULT 0,
    updated_at      TEXT    NOT NULL,
    PRIMARY KEY (user_id, media_id)
);

CREATE INDEX idx_anilist_entry_title ON anilist_entry (user_id, title_id);

-- Shared: which TVDB/IMDb entry an AniList id is, from the community-kept
-- Anime-IDs list. Titles are matched by id from this, never by name.
CREATE TABLE anime_map (
    anilist_id    INTEGER PRIMARY KEY,
    tvdb_id       INTEGER,
    tvdb_season   INTEGER,
    tvdb_epoffset INTEGER NOT NULL DEFAULT 0,
    imdb_id       TEXT
);

CREATE INDEX idx_anime_map_tvdb ON anime_map (tvdb_id, tvdb_season);

-- Shared: episode counts of AniList media, so placing a later season can add
-- up the earlier ones without asking AniList again.
CREATE TABLE anilist_media (
    id         INTEGER PRIMARY KEY,
    episodes   INTEGER,
    format     TEXT,
    name       TEXT,
    fetched_at TEXT NOT NULL
);

CREATE TABLE anime_map_state (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    fetched_at TEXT    NOT NULL
);
