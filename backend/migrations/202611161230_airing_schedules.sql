-- Which countries a show on the Airing list is made in, so TVmaze's day
-- schedules for those countries can be read; and when they last were.
ALTER TABLE airing_show ADD COLUMN countries TEXT NOT NULL DEFAULT '';
ALTER TABLE airing_state ADD COLUMN schedules_at TEXT;
