-- A finished film always counts as watched. Imports used to bring finished
-- films in without a watch when the file had no date, so they were missing
-- from the stats; give each one an undated watch, as importing does now.
INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch, source, undated)
SELECT l.user_id, l.title_id, NULL, COALESCE(l.finished_at, l.updated_at), 0, 'import', 1
  FROM library_entry l
  JOIN title t ON t.id = l.title_id
 WHERE t.kind = 'movie'
   AND l.status = 'completed'
   AND NOT EXISTS (SELECT 1 FROM watch_event w
                    WHERE w.user_id = l.user_id AND w.title_id = l.title_id
                      AND w.episode_id IS NULL);
