-- A notification someone removed stays as a row, hidden, so the same event
-- is never written to their page again and Undo can bring it back.
ALTER TABLE notification ADD COLUMN removed_at TEXT;
