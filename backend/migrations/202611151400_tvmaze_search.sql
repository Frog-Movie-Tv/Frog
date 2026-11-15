-- Shows TVmaze did not find by TMDB's ids are now also searched for by name
-- and year (crate::airtime): ask about them again on the next pass rather
-- than a week after the last miss.
UPDATE airtime_show SET tvmaze_checked_at = NULL WHERE tvmaze_id IS NULL;
