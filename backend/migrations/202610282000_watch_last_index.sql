-- The last time a title was watched, read straight off the index: sorting a
-- library of hundreds by "last watched" took over a second without it.
CREATE INDEX idx_watch_last ON watch_event (user_id, title_id, watched_at);
