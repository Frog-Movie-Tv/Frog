-- A profile people make their own: a name to show, a few lines about
-- themselves, a banner (a picture, or the backdrop of a title they love) and
-- up to four titles pinned to the top.
ALTER TABLE app_user ADD COLUMN display_name TEXT;
ALTER TABLE app_user ADD COLUMN bio TEXT;
-- A TMDB backdrop path, used when no banner picture is uploaded.
ALTER TABLE app_user ADD COLUMN banner_backdrop TEXT;
-- [{"kind": "movie", "tmdb_id": 603}, ...], at most four.
ALTER TABLE app_user ADD COLUMN pins_json TEXT NOT NULL DEFAULT '[]';

-- Uploaded profile pictures and banners. Small, so the database holds them
-- and they travel with every backup.
CREATE TABLE user_image (
    user_id    INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL CHECK (kind IN ('avatar', 'banner')),
    mime       TEXT    NOT NULL,
    bytes      BLOB    NOT NULL,
    updated_at TEXT    NOT NULL,
    PRIMARY KEY (user_id, kind)
);
