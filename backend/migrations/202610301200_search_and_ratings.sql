-- Shared: the characters of a title's cast, so searching for one reads this
-- short list rather than every cached TMDB response.
CREATE TABLE title_character (
    title_id    INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    ord         INTEGER NOT NULL,
    name        TEXT    NOT NULL,
    -- `text::words` of the name, joined with spaces.
    search_name TEXT    NOT NULL,
    actor       TEXT,
    actor_id    INTEGER,
    PRIMARY KEY (title_id, ord)
);

-- The worker stores characters with credits; ask it to go round once more.
UPDATE title SET people_synced_at = NULL
 WHERE id IN (SELECT title_id FROM title_external WHERE provider = 'tmdb' AND is_primary = 1);

-- Shared: what other sites score a title, on Frog's own title. `source` is
-- imdb, tomatoes, popcorn, metacritic, metacriticuser, letterboxd, trakt, tmdb,
-- anilist, myanimelist or rogerebert. `score` is 0-100, whatever the site uses.
CREATE TABLE title_rating (
    title_id   INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    source     TEXT    NOT NULL,
    score      REAL    NOT NULL,
    votes      INTEGER,
    fetched_at TEXT    NOT NULL,
    PRIMARY KEY (title_id, source)
);

-- Shared: when a title's outside ratings were last looked for.
CREATE TABLE title_rating_check (
    title_id   INTEGER PRIMARY KEY REFERENCES title (id) ON DELETE CASCADE,
    age_rating TEXT,
    checked_at TEXT NOT NULL,
    next_at    TEXT NOT NULL
);
CREATE INDEX idx_title_rating_check_next ON title_rating_check (next_at);

-- Shared: a studio AniList names, and the logo of the TMDB company with exactly
-- that name, if there is one. AniList has no pictures of studios.
CREATE TABLE studio_match (
    name_key   TEXT PRIMARY KEY,
    logo_path  TEXT,
    checked_at TEXT NOT NULL
);
