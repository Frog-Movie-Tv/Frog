-- Web Push. The VAPID key pair identifies this server to browser push
-- services; it is created on first use and never leaves the database.
CREATE TABLE push_vapid (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    private_pkcs8 BLOB NOT NULL,
    created_at    TEXT NOT NULL
);

-- One row per browser that turned notifications on.
CREATE TABLE push_subscription (
    endpoint   TEXT PRIMARY KEY,
    p256dh     TEXT NOT NULL,
    auth       TEXT NOT NULL,
    user_agent TEXT,
    created_at TEXT NOT NULL,
    last_ok_at TEXT,
    failures   INTEGER NOT NULL DEFAULT 0
);

-- Episodes already announced, so a restart or a second tick never repeats one.
CREATE TABLE push_sent (
    episode_id INTEGER PRIMARY KEY REFERENCES episode (id) ON DELETE CASCADE,
    sent_at    TEXT NOT NULL
);

-- Per-show switch. On by default: following a show is asking to hear about it.
ALTER TABLE library_entry ADD COLUMN notify INTEGER NOT NULL DEFAULT 1;
