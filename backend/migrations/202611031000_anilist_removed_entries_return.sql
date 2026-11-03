-- An AniList entry taken out of the library in Frog used to be left alone for
-- good. AniList is where the list is kept, so such entries come back on the
-- next sync; "Not in Frog" in Settings is how to keep one out.
UPDATE anilist_entry SET detached = 0 WHERE detached = 1;
