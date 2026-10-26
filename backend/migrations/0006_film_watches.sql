-- Marking a film watched set its status without recording a watch, so those
-- films were missing from the diary and the stats. They get one watch now,
-- undated: it counts towards all-time totals without inventing a diary date.
INSERT INTO watch_event (title_id, episode_id, watched_at, is_rewatch, source, undated)
SELECT l.title_id, NULL, COALESCE(l.finished_at, l.updated_at), 0, 'manual', 1
  FROM library_entry l
  JOIN title t ON t.id = l.title_id
 WHERE t.kind = 'movie'
   AND l.status = 'completed'
   AND NOT EXISTS (SELECT 1 FROM watch_event w WHERE w.title_id = l.title_id);
