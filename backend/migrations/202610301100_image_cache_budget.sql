-- The picture cache is held to a budget (see `crate::images`): pictures
-- nobody has looked at for longest go first. served_at is when a picture was
-- last served (marked at most daily); size is its length, kept so the worker
-- can add the cache up without reading every picture.
ALTER TABLE image_cache ADD COLUMN served_at TEXT;
ALTER TABLE image_cache ADD COLUMN size INTEGER;
UPDATE image_cache SET served_at = fetched_at, size = length(bytes);
