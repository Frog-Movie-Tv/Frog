-- Personal tokens for the browser extension, and what the extension logged.
--
-- A token is shown once and only its SHA-256 is kept, like a session's. It
-- reaches /api/ext and nothing else (`tokens::TokenAuth`).
CREATE TABLE api_token (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    token_hash   TEXT    NOT NULL UNIQUE,
    created_at   TEXT    NOT NULL,
    last_used_at TEXT
);

CREATE INDEX idx_api_token_user ON api_token (user_id);

-- One row per watch the extension wrote, so the same episode is not logged
-- twice within a few hours and undo takes back exactly what it added.
-- `status_before` is the library status before the log, NULL when the title
-- was not in the library, which is what undo puts back.
CREATE TABLE extension_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id       INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    token_id      INTEGER REFERENCES api_token (id) ON DELETE SET NULL,
    title_id      INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    episode_id    INTEGER REFERENCES episode (id) ON DELETE CASCADE,
    watch_id      INTEGER REFERENCES watch_event (id) ON DELETE SET NULL,
    status_before TEXT,
    logged_at     TEXT    NOT NULL
);

CREATE INDEX idx_extension_log_recent ON extension_log (user_id, title_id, logged_at);
