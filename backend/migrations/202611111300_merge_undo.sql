-- Undoing a merge from the app (store::undo_merge).
--
-- A merge undone keeps its row, marked, so the list in Settings shows it and
-- the same AniList film is not merged again behind the person's back.
ALTER TABLE title_merge ADD COLUMN undone_at TEXT;

-- An AniList entry the person took back out of its TMDB title: the sync keeps
-- it as its own AniList title instead of placing it again. Choosing a place
-- by hand clears it.
ALTER TABLE anilist_entry ADD COLUMN kept_apart INTEGER NOT NULL DEFAULT 0;
