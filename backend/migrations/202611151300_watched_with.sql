-- Who else was there for one watch, by account. The watch is its owner's
-- (`user_id`); the person named is told, and their own diary only changes
-- when they say so.
CREATE TABLE watch_with (
    watch_event_id INTEGER NOT NULL REFERENCES watch_event (id) ON DELETE CASCADE,
    user_id        INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    with_user_id   INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    created_at     TEXT    NOT NULL,
    PRIMARY KEY (watch_event_id, with_user_id)
);

CREATE INDEX idx_watch_with_user ON watch_with (user_id);
CREATE INDEX idx_watch_with_with ON watch_with (with_user_id);

-- Who a show is being watched with now: every episode ticked from then on
-- names them. Clearing it changes no watch already made.
CREATE TABLE watching_with (
    user_id      INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    title_id     INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    with_user_id INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    created_at   TEXT    NOT NULL,
    PRIMARY KEY (user_id, title_id, with_user_id)
);
