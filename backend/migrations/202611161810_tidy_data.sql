-- Small repairs, all additive (a rollback runs on this schema unchanged).

-- A title finished before it was started (16 entries on the live instance,
-- mostly AniList start days set after the finish). The finish is the firmer
-- fact, so the start moves back to it, now and on every later write,
-- whichever path writes the entry.
UPDATE library_entry SET started_at = finished_at
 WHERE started_at IS NOT NULL AND finished_at IS NOT NULL
   AND substr(finished_at, 1, 10) < substr(started_at, 1, 10);

CREATE TRIGGER library_entry_finished_after_start_insert AFTER INSERT ON library_entry
WHEN NEW.started_at IS NOT NULL AND NEW.finished_at IS NOT NULL
 AND substr(NEW.finished_at, 1, 10) < substr(NEW.started_at, 1, 10)
BEGIN
    UPDATE library_entry SET started_at = NEW.finished_at
     WHERE user_id = NEW.user_id AND title_id = NEW.title_id;
END;

CREATE TRIGGER library_entry_finished_after_start_update
AFTER UPDATE OF started_at, finished_at ON library_entry
WHEN NEW.started_at IS NOT NULL AND NEW.finished_at IS NOT NULL
 AND substr(NEW.finished_at, 1, 10) < substr(NEW.started_at, 1, 10)
BEGIN
    UPDATE library_entry SET started_at = NEW.finished_at
     WHERE user_id = NEW.user_id AND title_id = NEW.title_id;
END;

-- Ratings stored a hair off a whole number (55.00000000000001) by a score
-- conversion: whole again.
UPDATE library_entry SET rating = ROUND(rating)
 WHERE rating IS NOT NULL AND rating <> ROUND(rating) AND ABS(rating - ROUND(rating)) < 0.000001;
UPDATE season_rating SET rating = ROUND(rating)
 WHERE rating <> ROUND(rating) AND ABS(rating - ROUND(rating)) < 0.000001;

-- Two indexes another one already covers: the episode table's unique
-- (title_id, season_number, episode_number), and idx_watch_last's
-- (user_id, title_id, watched_at).
DROP INDEX IF EXISTS idx_episode_title;
DROP INDEX IF EXISTS idx_watch_title;
