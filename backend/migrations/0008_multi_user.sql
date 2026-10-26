-- Multi-user. 0001 said "single-user by design: no user_id columns anywhere",
-- so every personal row belonged to the one account implicitly. Each of them
-- now names its owner, and isolation is a NOT NULL foreign key rather than a
-- convention a future query can forget.
--
-- Shared by everyone: title, episode, title_person, tmdb_cache, image_cache
-- and the VAPID key pair, which identifies the server rather than a person.
-- Everything touched below is per user.
--
-- Rebuilding is the only way to add a NOT NULL foreign key or change a primary
-- key in SQLite, so each table is recreated, copied and renamed. Existing rows
-- belong to the account that was already here, which becomes the owner. The
-- `WHERE EXISTS` guards keep a fresh database (no account yet) from trying to
-- give rows a NULL owner; there cannot be personal data without an account.

-- app_user first, while nothing references it yet: the single-row CHECK has to
-- go, and roles, disabling and the profile switch come in.
CREATE TABLE app_user_new (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    username       TEXT    NOT NULL UNIQUE,
    password_hash  TEXT    NOT NULL,
    role           TEXT    NOT NULL DEFAULT 'user' CHECK (role IN ('owner', 'admin', 'user')),
    -- A disabled account keeps its data and cannot sign in.
    disabled       INTEGER NOT NULL DEFAULT 0,
    -- Off by default: a profile other accounts can look at is something you
    -- turn on, never something that happens to you.
    profile_public INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT    NOT NULL,
    totp_secret    TEXT,
    totp_pending   TEXT,
    totp_last_step INTEGER NOT NULL DEFAULT 0
);

INSERT INTO app_user_new (id, username, password_hash, role, disabled, profile_public,
                          created_at, totp_secret, totp_pending, totp_last_step)
SELECT id, username, password_hash, 'owner', 0, 0,
       created_at, totp_secret, totp_pending, totp_last_step
  FROM app_user;

DROP TABLE app_user;
ALTER TABLE app_user_new RENAME TO app_user;

-- Invite links. Only the hash is stored, as with sessions and recovery codes,
-- so a leaked database file hands over no working invitation. Single use is
-- used_at; expiring is expires_at.
CREATE TABLE invite (
    code_hash  TEXT    PRIMARY KEY,
    role       TEXT    NOT NULL CHECK (role IN ('admin', 'user')),
    created_by INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    created_at TEXT    NOT NULL,
    expires_at TEXT    NOT NULL,
    used_at    TEXT,
    used_by    INTEGER REFERENCES app_user (id) ON DELETE SET NULL
);

CREATE INDEX idx_invite_expires ON invite (expires_at);

-- Sessions belong to an account, so "sign out everywhere" is one DELETE and a
-- disabled or deleted account loses its sessions with it.
CREATE TABLE session_new (
    token_hash TEXT    PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    created_at TEXT    NOT NULL,
    expires_at TEXT    NOT NULL,
    user_agent TEXT
);

INSERT INTO session_new (token_hash, user_id, created_at, expires_at, user_agent)
SELECT token_hash, (SELECT MIN(id) FROM app_user), created_at, expires_at, user_agent
  FROM session
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE session;
ALTER TABLE session_new RENAME TO session;

CREATE INDEX idx_session_expires ON session (expires_at);
CREATE INDEX idx_session_user ON session (user_id);

CREATE TABLE library_entry_new (
    user_id       INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    title_id      INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    status        TEXT    NOT NULL CHECK (status IN ('watching', 'completed', 'planned', 'paused', 'dropped')),
    rating        REAL,
    favourite     INTEGER NOT NULL DEFAULT 0,
    notes         TEXT,
    started_at    TEXT,
    finished_at   TEXT,
    rewatch_count INTEGER NOT NULL DEFAULT 0,
    added_at      TEXT    NOT NULL,
    updated_at    TEXT    NOT NULL,
    notify        INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (user_id, title_id)
);

INSERT INTO library_entry_new (user_id, title_id, status, rating, favourite, notes, started_at,
                               finished_at, rewatch_count, added_at, updated_at, notify)
SELECT (SELECT MIN(id) FROM app_user), title_id, status, rating, favourite, notes, started_at,
       finished_at, rewatch_count, added_at, updated_at, notify
  FROM library_entry
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE library_entry;
ALTER TABLE library_entry_new RENAME TO library_entry;

CREATE INDEX idx_library_status ON library_entry (user_id, status);

CREATE TABLE episode_state_new (
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    episode_id INTEGER NOT NULL REFERENCES episode (id) ON DELETE CASCADE,
    rating     REAL,
    notes      TEXT,
    updated_at TEXT    NOT NULL,
    PRIMARY KEY (user_id, episode_id)
);

INSERT INTO episode_state_new (user_id, episode_id, rating, notes, updated_at)
SELECT (SELECT MIN(id) FROM app_user), episode_id, rating, notes, updated_at
  FROM episode_state
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE episode_state;
ALTER TABLE episode_state_new RENAME TO episode_state;

CREATE TABLE watch_event_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    title_id   INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    episode_id INTEGER REFERENCES episode (id) ON DELETE CASCADE,
    watched_at TEXT    NOT NULL,
    is_rewatch INTEGER NOT NULL DEFAULT 0,
    source     TEXT    NOT NULL DEFAULT 'manual',
    undated    INTEGER NOT NULL DEFAULT 0
);

INSERT INTO watch_event_new (id, user_id, title_id, episode_id, watched_at, is_rewatch,
                             source, undated)
SELECT id, (SELECT MIN(id) FROM app_user), title_id, episode_id, watched_at, is_rewatch,
       source, undated
  FROM watch_event
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE watch_event;
ALTER TABLE watch_event_new RENAME TO watch_event;

CREATE INDEX idx_watch_title ON watch_event (user_id, title_id);
CREATE INDEX idx_watch_episode ON watch_event (user_id, episode_id);
CREATE INDEX idx_watch_date ON watch_event (user_id, watched_at);

-- Preferences (theme, mascot, score format, home rows, ...) are one JSON blob
-- per account, so a theme is yours rather than the instance's.
CREATE TABLE app_setting_new (
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    key        TEXT    NOT NULL,
    value      TEXT    NOT NULL,
    updated_at TEXT    NOT NULL,
    PRIMARY KEY (user_id, key)
);

INSERT INTO app_setting_new (user_id, key, value, updated_at)
SELECT (SELECT MIN(id) FROM app_user), key, value, updated_at
  FROM app_setting
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE app_setting;
ALTER TABLE app_setting_new RENAME TO app_setting;

-- shelf_item hangs off shelf with ON DELETE CASCADE, and dropping a parent
-- table runs that cascade, so the items are parked in a constraint-free copy
-- while shelf is rebuilt.
CREATE TABLE shelf_item_tmp AS SELECT shelf_id, title_id, position FROM shelf_item;
DROP TABLE shelf_item;

CREATE TABLE shelf_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    kind       TEXT    NOT NULL CHECK (kind IN ('manual', 'smart')),
    rules_json TEXT    NOT NULL DEFAULT '{}',
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT    NOT NULL
);

INSERT INTO shelf_new (id, user_id, name, kind, rules_json, position, created_at)
SELECT id, (SELECT MIN(id) FROM app_user), name, kind, rules_json, position, created_at
  FROM shelf
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE shelf;
ALTER TABLE shelf_new RENAME TO shelf;

CREATE INDEX idx_shelf_user ON shelf (user_id, position);

CREATE TABLE shelf_item (
    shelf_id INTEGER NOT NULL REFERENCES shelf (id) ON DELETE CASCADE,
    title_id INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (shelf_id, title_id)
);

INSERT INTO shelf_item (shelf_id, title_id, position)
SELECT shelf_id, title_id, position FROM shelf_item_tmp
 WHERE shelf_id IN (SELECT id FROM shelf);

DROP TABLE shelf_item_tmp;

-- One browser can be two accounts, so the endpoint alone is no longer the key.
CREATE TABLE push_subscription_new (
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    endpoint   TEXT    NOT NULL,
    p256dh     TEXT    NOT NULL,
    auth       TEXT    NOT NULL,
    user_agent TEXT,
    created_at TEXT    NOT NULL,
    last_ok_at TEXT,
    failures   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, endpoint)
);

INSERT INTO push_subscription_new (user_id, endpoint, p256dh, auth, user_agent, created_at,
                                   last_ok_at, failures)
SELECT (SELECT MIN(id) FROM app_user), endpoint, p256dh, auth, user_agent, created_at,
       last_ok_at, failures
  FROM push_subscription
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE push_subscription;
ALTER TABLE push_subscription_new RENAME TO push_subscription;

-- An episode announced to one account still has to be announced to the others.
CREATE TABLE push_sent_new (
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    episode_id INTEGER NOT NULL REFERENCES episode (id) ON DELETE CASCADE,
    sent_at    TEXT    NOT NULL,
    PRIMARY KEY (user_id, episode_id)
);

INSERT INTO push_sent_new (user_id, episode_id, sent_at)
SELECT (SELECT MIN(id) FROM app_user), episode_id, sent_at
  FROM push_sent
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE push_sent;
ALTER TABLE push_sent_new RENAME TO push_sent;

CREATE TABLE recovery_code_new (
    user_id   INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    code_hash TEXT    NOT NULL,
    used_at   TEXT,
    PRIMARY KEY (user_id, code_hash)
);

INSERT INTO recovery_code_new (user_id, code_hash, used_at)
SELECT (SELECT MIN(id) FROM app_user), code_hash, used_at
  FROM recovery_code
 WHERE EXISTS (SELECT 1 FROM app_user);

DROP TABLE recovery_code;
ALTER TABLE recovery_code_new RENAME TO recovery_code;
