-- Badges an administrator makes by hand and gives to people. A badge is
-- shared, like a title: one row, however many hold it. The pictures live in
-- the database, as profile pictures do, so they travel with every backup.
CREATE TABLE badge (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    description   TEXT,
    -- Finished, round, lossless PNGs: 1024 px opened large, 128 px elsewhere.
    large         BLOB    NOT NULL,
    small         BLOB    NOT NULL,
    -- What was uploaded (an SVG already drawn to pixels) and how the editor
    -- was left, so editing the badge starts where it stopped.
    original      BLOB    NOT NULL,
    original_mime TEXT    NOT NULL,
    settings_json TEXT    NOT NULL DEFAULT '{}',
    created_at    TEXT    NOT NULL,
    updated_at    TEXT    NOT NULL
);

-- Who holds which badge. Personal: the holder decides whether it shows and
-- in what order.
CREATE TABLE badge_award (
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    badge_id   INTEGER NOT NULL REFERENCES badge (id) ON DELETE CASCADE,
    awarded_at TEXT    NOT NULL,
    awarded_by INTEGER REFERENCES app_user (id) ON DELETE SET NULL,
    shown      INTEGER NOT NULL DEFAULT 1,
    position   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, badge_id)
);

CREATE INDEX badge_award_badge ON badge_award (badge_id);
