-- Where a title comes from and the language it was made in, as TMDB says:
-- `countries_json` its origin countries' codes (["KR"]), `original_language`
-- an ISO 639-1 code ("ko"). NULL until the title is next read from TMDB or
-- the worker fills it (`jobs::backfill_origins_once`); '[]' and '' when TMDB
-- has nothing, so it is not asked again.
ALTER TABLE title ADD COLUMN countries_json TEXT;
ALTER TABLE title ADD COLUMN original_language TEXT;
