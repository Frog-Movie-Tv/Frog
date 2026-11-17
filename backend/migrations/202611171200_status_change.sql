-- Every change of a library entry's status, from now on: what it was (NULL
-- when the entry was added) and what it became (NULL when it was removed).
-- Written by triggers, so every way an entry changes is kept the same way.
-- Badges read it (Back From the Dead, Watchlist Zero); nothing else does.
CREATE TABLE status_change (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    -- No foreign key: a trigger here may not name `title`, which
    -- `db::rebuild_title` drops and makes again on a new database. A title
    -- deleted leaves its rows, which go with the account.
    title_id    INTEGER NOT NULL,
    from_status TEXT,
    to_status   TEXT,
    at          TEXT    NOT NULL
);

CREATE INDEX idx_status_change_user ON status_change (user_id, at);

CREATE TRIGGER library_entry_status_added
AFTER INSERT ON library_entry
BEGIN
    INSERT INTO status_change (user_id, title_id, from_status, to_status, at)
    VALUES (NEW.user_id, NEW.title_id, NULL, NEW.status,
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
END;

CREATE TRIGGER library_entry_status_changed
AFTER UPDATE OF status ON library_entry
WHEN OLD.status IS NOT NEW.status
BEGIN
    INSERT INTO status_change (user_id, title_id, from_status, to_status, at)
    VALUES (NEW.user_id, NEW.title_id, OLD.status, NEW.status,
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
END;

CREATE TRIGGER library_entry_status_removed
AFTER DELETE ON library_entry
-- Not when the account itself is going: the entry leaves by cascade then,
-- and its history goes with it.
WHEN EXISTS (SELECT 1 FROM app_user WHERE id = OLD.user_id)
BEGIN
    INSERT INTO status_change (user_id, title_id, from_status, to_status, at)
    VALUES (OLD.user_id, OLD.title_id, OLD.status, NULL,
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
END;
