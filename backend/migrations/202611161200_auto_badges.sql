-- Badges Frog gives on its own (`crate::awards`).
--
-- When and how each watch and read reached Frog. A badge about a moment (a
-- triple bill, a streak) counts only watches recorded one at a time, close
-- to when they happened; everything already here stays NULL and counts for
-- totals only.
ALTER TABLE watch_event ADD COLUMN logged_at TEXT;
ALTER TABLE watch_event ADD COLUMN logged_how TEXT;
ALTER TABLE chapter_read ADD COLUMN logged_at TEXT;
ALTER TABLE chapter_read ADD COLUMN logged_how TEXT;

-- Most writers say nothing about either, so the source decides: an import is
-- an import, an undated AniList watch is a first sync catching up, a dated
-- one a later sync. Writers that know better (marking many at once, a copy
-- from someone else's watch, undo) set `logged_how` themselves, and this
-- leaves their row alone. An older image that knows nothing of the columns
-- still gets them filled.
CREATE TRIGGER watch_event_logged AFTER INSERT ON watch_event
WHEN NEW.logged_how IS NULL
BEGIN
    UPDATE watch_event
       SET logged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
           logged_how = CASE NEW.source
                            WHEN 'import' THEN 'import'
                            WHEN 'anilist' THEN CASE WHEN NEW.undated THEN 'anilist_first'
                                                     ELSE 'anilist' END
                            WHEN 'extension' THEN 'extension'
                            ELSE 'tick' END
     WHERE id = NEW.id;
END;

CREATE TRIGGER chapter_read_logged AFTER INSERT ON chapter_read
WHEN NEW.logged_how IS NULL
BEGIN
    UPDATE chapter_read
       SET logged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
           logged_how = CASE NEW.source
                            WHEN 'import' THEN 'import'
                            WHEN 'anilist' THEN 'anilist'
                            ELSE 'tick' END
     WHERE id = NEW.id;
END;

CREATE INDEX idx_watch_logged ON watch_event (user_id, logged_at);

-- An automatic badge is a `badge` row with the catalogue's key in `rule`;
-- hand-made badges keep NULL. Its pictures are drawn from the catalogue's art
-- when asked for, so `large`, `small` and `original` stay empty until an
-- administrator redraws it. `rule_sig` is the catalogue entry it was last
-- brought up to; `rule_kept` is set once an administrator renames it, after
-- which the catalogue no longer changes its words.
ALTER TABLE badge ADD COLUMN rule TEXT;
ALTER TABLE badge ADD COLUMN rule_sig TEXT;
ALTER TABLE badge ADD COLUMN rule_kept INTEGER NOT NULL DEFAULT 0;
CREATE UNIQUE INDEX badge_rule ON badge (rule) WHERE rule IS NOT NULL;

-- What an automatic award rests on: the day it was earned, the watches and
-- titles behind it (JSON), and which version of its rule decided.
ALTER TABLE badge_award ADD COLUMN earned_on TEXT;
ALTER TABLE badge_award ADD COLUMN evidence TEXT;
ALTER TABLE badge_award ADD COLUMN rule_version INTEGER;
