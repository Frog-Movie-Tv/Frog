-- Release days and times from TheTVDB (crate::airing).
--
-- `air_at` is the moment an episode comes out, as UTC in RFC 3339
-- ("2025-03-21T01:00:00Z"), when TheTVDB knows the time of day; NULL keeps
-- the day alone. `air_source` is 'tvdb' when air_date and air_at came from
-- TheTVDB, so a TMDB refresh does not put TMDB's day back over it.
ALTER TABLE episode ADD COLUMN air_at TEXT;
ALTER TABLE episode ADD COLUMN air_source TEXT;
CREATE INDEX idx_episode_air_at ON episode (air_at);

-- What TheTVDB says about one title, shared by every account. `tvdb_id` is
-- NULL when TheTVDB was asked and has no match, so it is not asked again
-- until `checked_at` is old. Never a title's identity: titles are Frog's.
CREATE TABLE title_airing (
    title_id     INTEGER PRIMARY KEY REFERENCES title (id) ON DELETE CASCADE,
    tvdb_id      INTEGER,
    -- "21:00" in `time_zone`, the network's own clock; NULL when unknown.
    airs_time    TEXT,
    time_zone    TEXT,
    -- A film's first release on TheTVDB.
    release_date TEXT,
    checked_at   TEXT NOT NULL
);

-- A film is announced once per account, as an episode is in push_sent.
CREATE TABLE push_sent_title (
    user_id  INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    title_id INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    sent_at  TEXT    NOT NULL,
    PRIMARY KEY (user_id, title_id)
);
