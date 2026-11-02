-- Titles looked at before MDBList had a key were recorded as "nothing found"
-- and left for a month, and so were anime films asked about by MyAnimeList id
-- the way MDBList refuses. Anything without a score from MDBList is due again.
UPDATE title_rating_check
   SET next_at = checked_at
 WHERE NOT EXISTS (
        SELECT 1 FROM title_rating r
         WHERE r.title_id = title_rating_check.title_id
           AND r.source NOT IN ('tmdb', 'anilist')
       );
