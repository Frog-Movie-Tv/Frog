-- A catalog Frog owns. A title is Frog's own row, `title.id`, and every
-- provider that knows it is a row here: TMDB, AniList, IMDb. Personal data
-- already points at `title.id` and does not change.
--
-- (provider, external_id) is the key, so one provider's id can never name
-- two titles. A title has at most one primary provider, the one its
-- metadata comes from; IMDb is never primary (Frog cannot read it). A
-- provider that loses a title only sets gone_at: the title and everyone's
-- history stay.
--
-- TMDB ids are only unique per kind, so they are written as TMDB's own path
-- does: "movie/550", "tv/1399". AniList ids are the number, IMDb's are
-- "tt0113277".
--
-- The columns this replaces (title.tmdb_id, title.anilist_id,
-- title.imdb_id) are dropped by `db::rebuild_title` right after the
-- migrations run: SQLite can only drop a column in a UNIQUE constraint by
-- rebuilding the table, and that needs foreign keys off, which a migration's
-- transaction cannot do.
CREATE TABLE title_external (
    title_id    INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    provider    TEXT    NOT NULL CHECK (provider IN ('tmdb', 'anilist', 'imdb')),
    external_id TEXT    NOT NULL,
    is_primary  INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    gone_at     TEXT,
    added_at    TEXT    NOT NULL,
    PRIMARY KEY (provider, external_id),
    CHECK (provider != 'imdb' OR is_primary = 0)
);

CREATE INDEX idx_title_external_title ON title_external (title_id);
CREATE UNIQUE INDEX idx_title_external_primary ON title_external (title_id) WHERE is_primary = 1;

-- Every TMDB title, with TMDB as its provider.
INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
SELECT id, 'tmdb', kind || '/' || tmdb_id, 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  FROM title WHERE tmdb_id > 0;

-- Every AniList-only title (tmdb_id was the AniList id made negative), with
-- AniList as its provider.
INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
SELECT id, 'anilist', CAST(-tmdb_id AS TEXT), 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  FROM title WHERE tmdb_id < 0;

-- An AniList id kept beside a TMDB id, should any title have one.
INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
SELECT id, 'anilist', CAST(anilist_id AS TEXT), 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  FROM title WHERE anilist_id IS NOT NULL AND tmdb_id > 0;

-- IMDb ids TMDB gave. TMDB has listed one IMDb id on two entries before;
-- the older title keeps it.
INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
SELECT id, 'imdb', imdb_id, 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  FROM title WHERE imdb_id IS NOT NULL AND imdb_id != '' ORDER BY id;

-- Profile pins named titles by TMDB id; they name Frog's titles now, in the
-- same order. A pin whose title is not in the catalog is dropped (a pin can
-- only be made from the library, so there are none).
UPDATE app_user
   SET pins_json = (
       SELECT COALESCE(json_group_array(json_object('title_id', title_id)), '[]')
         FROM (SELECT t.id AS title_id
                 FROM json_each(app_user.pins_json) p
                 JOIN title t ON t.kind = json_extract(p.value, '$.kind')
                             AND t.tmdb_id = json_extract(p.value, '$.tmdb_id')
                ORDER BY p.key))
 WHERE pins_json IS NOT NULL AND json_valid(pins_json) AND pins_json != '[]';
