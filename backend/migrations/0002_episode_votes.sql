-- The heatmap looked empty until you had rated episodes yourself. Keeping
-- TMDB's own episode score lets it show the shape of a season straight away,
-- with your own ratings drawn over the top.
ALTER TABLE episode ADD COLUMN tmdb_vote REAL;
