-- A watchlist is a hand-picked shelf whose titles are picked from everything,
-- not only the library. Kept as a flag on a manual shelf rather than a third
-- kind: the kind's CHECK would need `shelf` rebuilt, which a migration cannot
-- do to a table others point at, and the release before reads it as a plain
-- hand-picked shelf.
ALTER TABLE shelf ADD COLUMN watchlist INTEGER NOT NULL DEFAULT 0;
