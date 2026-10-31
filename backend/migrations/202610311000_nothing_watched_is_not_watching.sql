-- An import could mark a show Watching with nothing to tick and never list its
-- episodes: Netflix's history names a show with no episode. Such a show read
-- "Finished" at 0 of 50 on Home. It goes to the watchlist, where a new import
-- now puts it.
--
-- Only that exact state: a show from TMDB with no episode listed for anyone, so
-- it cannot have been ticked, no watch of this account's and no AniList entry,
-- whose word stands. Marking a show Watching by hand lists its episodes, so
-- this does not meet one of those. Nothing else on the row changes.
UPDATE library_entry
   SET status = 'planned'
 WHERE status = 'watching'
   AND title_id IN (SELECT t.id
                      FROM title t
                      JOIN title_external x ON x.title_id = t.id
                                           AND x.provider = 'tmdb' AND x.is_primary = 1
                     WHERE t.kind = 'tv')
   AND NOT EXISTS (SELECT 1 FROM episode e WHERE e.title_id = library_entry.title_id)
   AND NOT EXISTS (SELECT 1 FROM watch_event w
                    WHERE w.user_id = library_entry.user_id
                      AND w.title_id = library_entry.title_id)
   AND NOT EXISTS (SELECT 1 FROM anilist_entry a
                    WHERE a.user_id = library_entry.user_id
                      AND a.title_id = library_entry.title_id);
