-- Frog initial schema.
-- Single-user by design: there is exactly one row in app_user (id = 1) and
-- every library row belongs to it implicitly, so no user_id columns anywhere.

CREATE TABLE app_user (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    username      TEXT    NOT NULL UNIQUE,
    password_hash TEXT    NOT NULL,
    created_at    TEXT    NOT NULL
);

CREATE TABLE session (
    token_hash TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    user_agent TEXT
);

CREATE INDEX idx_session_expires ON session (expires_at);

-- Raw TMDB responses, keyed by the request path + query. Lets the whole app
-- work read-only offline once a title has been opened at least once.
CREATE TABLE tmdb_cache (
    cache_key  TEXT PRIMARY KEY,
    body       TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX idx_tmdb_cache_expires ON tmdb_cache (expires_at);

-- Binary cache for images proxied from image.tmdb.org. Serving them from our
-- own origin means the service worker can cache them and canvas reads are
-- same-origin.
CREATE TABLE image_cache (
    path         TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    bytes        BLOB NOT NULL,
    fetched_at   TEXT NOT NULL
);

-- Denormalised copy of the TMDB fields the library grid needs, so filtering
-- and sorting a few thousand titles never touches the network.
CREATE TABLE title (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    kind           TEXT    NOT NULL CHECK (kind IN ('movie', 'tv')),
    tmdb_id        INTEGER NOT NULL,
    name           TEXT    NOT NULL,
    original_name  TEXT,
    overview       TEXT,
    poster_path    TEXT,
    backdrop_path  TEXT,
    logo_path      TEXT,
    release_date   TEXT,
    last_air_date  TEXT,
    tmdb_status    TEXT,
    runtime        INTEGER,
    episode_count  INTEGER,
    season_count   INTEGER,
    genres_json    TEXT    NOT NULL DEFAULT '[]',
    tmdb_vote      REAL,
    imdb_id        TEXT,
    collection_id  INTEGER,
    accent_json    TEXT,
    refreshed_at   TEXT,
    UNIQUE (kind, tmdb_id)
);

CREATE INDEX idx_title_name ON title (name);

CREATE TABLE library_entry (
    title_id      INTEGER PRIMARY KEY REFERENCES title (id) ON DELETE CASCADE,
    status        TEXT    NOT NULL CHECK (status IN ('watching', 'completed', 'planned', 'paused', 'dropped')),
    rating        REAL,
    favourite     INTEGER NOT NULL DEFAULT 0,
    notes         TEXT,
    started_at    TEXT,
    finished_at   TEXT,
    rewatch_count INTEGER NOT NULL DEFAULT 0,
    added_at      TEXT    NOT NULL,
    updated_at    TEXT    NOT NULL
);

CREATE INDEX idx_library_status ON library_entry (status);

CREATE TABLE episode (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    title_id        INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    season_number   INTEGER NOT NULL,
    episode_number  INTEGER NOT NULL,
    tmdb_episode_id INTEGER,
    name            TEXT,
    overview        TEXT,
    still_path      TEXT,
    air_date        TEXT,
    runtime         INTEGER,
    UNIQUE (title_id, season_number, episode_number)
);

CREATE INDEX idx_episode_title ON episode (title_id, season_number, episode_number);
CREATE INDEX idx_episode_air ON episode (air_date);

-- Per-episode rating / note. Separate from watches so a rewatch does not
-- duplicate or clobber what you thought of the episode.
CREATE TABLE episode_state (
    episode_id INTEGER PRIMARY KEY REFERENCES episode (id) ON DELETE CASCADE,
    rating     REAL,
    notes      TEXT,
    updated_at TEXT NOT NULL
);

-- The diary. One row per viewing; episode_id is NULL for films.
CREATE TABLE watch_event (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    title_id   INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    episode_id INTEGER REFERENCES episode (id) ON DELETE CASCADE,
    watched_at TEXT    NOT NULL,
    is_rewatch INTEGER NOT NULL DEFAULT 0,
    source     TEXT    NOT NULL DEFAULT 'manual'
);

CREATE INDEX idx_watch_title ON watch_event (title_id);
CREATE INDEX idx_watch_episode ON watch_event (episode_id);
CREATE INDEX idx_watch_date ON watch_event (watched_at);

-- Free-form app settings (theme JSON, score format, home row order, ...).
CREATE TABLE app_setting (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Shelves land in phase 3; the tables exist now so the schema stays stable.
CREATE TABLE shelf (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    kind       TEXT    NOT NULL CHECK (kind IN ('manual', 'smart')),
    rules_json TEXT    NOT NULL DEFAULT '{}',
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT    NOT NULL
);

CREATE TABLE shelf_item (
    shelf_id INTEGER NOT NULL REFERENCES shelf (id) ON DELETE CASCADE,
    title_id INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (shelf_id, title_id)
);
