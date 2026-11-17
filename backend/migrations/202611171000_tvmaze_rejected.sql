-- A TVmaze show once linked to a TMDB show and found to be the wrong one
-- (it ended long before episodes TMDB lists), so the next look passes it over.
ALTER TABLE airtime_show ADD COLUMN tvmaze_rejected INTEGER;
