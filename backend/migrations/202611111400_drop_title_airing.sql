-- The table of the air-time source Frog no longer reads (crate::airtime took
-- its place). Dropped in the same release that stopped reading it, so this
-- release raises db::SCHEMA_GENERATION to 2: on a failed update,
-- deploy/update.sh restores the pre-update snapshot instead of starting the
-- previous image on a schema it can no longer read.
DROP TABLE IF EXISTS title_airing;
