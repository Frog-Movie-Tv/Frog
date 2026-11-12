-- Air times are matched by name, day and number, and TVmaze and AniList give
-- the day as well as the moment (crate::airtime). Every show is worked out
-- again at once rather than when its last pass grows old.
UPDATE airtime_show SET times_at = NULL;
