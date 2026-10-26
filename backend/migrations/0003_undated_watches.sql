-- Bulk-marking a back catalogue used to stamp every episode with today's
-- date, flooding the diary. A watch can now be recorded without a date: it
-- still counts as watched everywhere, but stays out of the diary timeline.
ALTER TABLE watch_event ADD COLUMN undated INTEGER NOT NULL DEFAULT 0;
