-- Events: a countdown to one upcoming title, with what to catch up on
-- before it. Each event comes from a pack compiled into the release
-- (`backend/events/{slug}/`); these rows hold what the pack cannot: the
-- titles it names made Frog's, the day it opens here, and the
-- administrator's switch and pinned moment.

CREATE TABLE event (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT NOT NULL UNIQUE,
    title_id    INTEGER REFERENCES title (id) ON DELETE SET NULL,
    -- The day it opens in FROG_REGION, from TMDB, else the pack's.
    release_day TEXT,
    released_checked_at TEXT,
    state       TEXT NOT NULL DEFAULT 'on' CHECK (state IN ('on', 'off')),
    -- An exact moment, the same for everyone, instead of midnight wherever
    -- each person is.
    pinned_at   TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- The catch-up list, in watch order, as the pack has it.
CREATE TABLE event_title (
    event_id    INTEGER NOT NULL REFERENCES event (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    title_id    INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    season_from INTEGER,
    season_to   INTEGER,
    group_name  TEXT NOT NULL,
    essential   INTEGER NOT NULL DEFAULT 1,
    note        TEXT,
    PRIMARY KEY (event_id, position)
);

CREATE INDEX idx_event_title_title ON event_title (title_id);

-- Someone who asked not to see an event on Home, nor hear about it.
CREATE TABLE event_hidden (
    user_id  INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    event_id INTEGER NOT NULL REFERENCES event (id) ON DELETE CASCADE,
    at       TEXT NOT NULL,
    PRIMARY KEY (user_id, event_id)
);

-- The premiere has played for this account, on whichever device came first.
CREATE TABLE event_seen (
    user_id  INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    event_id INTEGER NOT NULL REFERENCES event (id) ON DELETE CASCADE,
    at       TEXT NOT NULL,
    PRIMARY KEY (user_id, event_id)
);
