-- Single-use links that set a new password, one per account at most. Only the
-- SHA-256 of a code is stored, as with invites and sessions. Additive: a
-- previous image ignores the table.
CREATE TABLE password_reset (
    code_hash  TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    created_by INTEGER REFERENCES app_user (id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at    TEXT
);

CREATE INDEX idx_password_reset_user ON password_reset (user_id);
