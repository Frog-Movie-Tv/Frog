-- The colour a profile wears, as two words from a fixed list ("teal",
-- "dark"). The values live in the app; the server keeps only the names, and
-- nothing means the theme's own colour.
ALTER TABLE app_user ADD COLUMN profile_color TEXT;
ALTER TABLE app_user ADD COLUMN profile_shade TEXT;
