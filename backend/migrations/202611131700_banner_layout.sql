-- How a profile's banner is laid out: across the top ("classic") or as a
-- portrait column beside everything else ("vertical"), and, across the top,
-- how tall ("short", "tall", or "full" for the whole picture). Nothing is
-- the classic, short banner every profile had before.
ALTER TABLE app_user ADD COLUMN banner_layout TEXT;
ALTER TABLE app_user ADD COLUMN banner_height TEXT;
