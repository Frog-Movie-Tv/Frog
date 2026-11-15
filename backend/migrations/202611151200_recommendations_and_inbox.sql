-- A title one account suggests to another. One row per title, sender and
-- recipient: recommending it again refreshes the row rather than adding one.
CREATE TABLE recommendation (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title_id     INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    from_user_id INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    to_user_id   INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    note         TEXT,
    created_at   TEXT    NOT NULL,
    seen_at      TEXT,
    -- NULL until the recipient answers: 'added' or 'dismissed'.
    answered     TEXT,
    answered_at  TEXT,
    UNIQUE (title_id, from_user_id, to_user_id)
);

CREATE INDEX idx_recommendation_to ON recommendation (to_user_id, created_at);

-- What one account is told, in the app: the Notifications page and its bell.
-- Written where the decision to tell is made, whether or not a browser is
-- subscribed for push. `ref_key` makes each event land once.
CREATE TABLE notification (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL,
    title_id   INTEGER REFERENCES title (id) ON DELETE CASCADE,
    actor_id   INTEGER REFERENCES app_user (id) ON DELETE CASCADE,
    payload    TEXT    NOT NULL DEFAULT '{}',
    ref_key    TEXT,
    created_at TEXT    NOT NULL,
    read_at    TEXT
);

CREATE INDEX idx_notification_user ON notification (user_id, created_at);
CREATE UNIQUE INDEX idx_notification_ref ON notification (user_id, ref_key)
    WHERE ref_key IS NOT NULL;
