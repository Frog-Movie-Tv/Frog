-- How each AniList entry was matched, so Settings can say why, and what
-- AniList says about its release.
--   outcome: matched | no_candidate | not_confident | upcoming | apply_failed
ALTER TABLE anilist_entry ADD COLUMN outcome TEXT;
-- Which evidence placed it: TMDB id list, TVDB id list, air date, ...
ALTER TABLE anilist_entry ADD COLUMN method TEXT;
-- 0..100: how sure the match is. Only confident ones are applied.
ALTER TABLE anilist_entry ADD COLUMN confidence INTEGER;
-- One line for the person: why it is unmatched, or what to check.
ALTER TABLE anilist_entry ADD COLUMN note TEXT;
-- Chosen by the person: never replaced by an automatic match.
ALTER TABLE anilist_entry ADD COLUMN manual INTEGER NOT NULL DEFAULT 0;
-- Which version of the matcher placed it; an older automatic placement is
-- looked at again, and moved when a better one is certain.
ALTER TABLE anilist_entry ADD COLUMN resolver INTEGER NOT NULL DEFAULT 0;
-- AniList's own release state: FINISHED, RELEASING, NOT_YET_RELEASED, ...
ALTER TABLE anilist_entry ADD COLUMN media_status TEXT;
-- yyyy-mm-dd when AniList knows the whole date, else NULL.
ALTER TABLE anilist_entry ADD COLUMN start_date TEXT;
ALTER TABLE anilist_entry ADD COLUMN next_airing_at TEXT;
ALTER TABLE anilist_entry ADD COLUMN next_airing_episode INTEGER;

-- Shared: TMDB's next episode of a show, so "waiting for the next episode"
-- and "waiting for a new season" can be told apart.
ALTER TABLE title ADD COLUMN next_air_date TEXT;
ALTER TABLE title ADD COLUMN next_air_season INTEGER;
ALTER TABLE title ADD COLUMN next_air_episode INTEGER;

-- Shared: the prequel cache also keeps start dates, release state and the
-- main-run entries a special or film hangs off. Refilled on the next sync.
ALTER TABLE anilist_chain ADD COLUMN start_date TEXT;
ALTER TABLE anilist_chain ADD COLUMN media_status TEXT;
ALTER TABLE anilist_chain ADD COLUMN related TEXT NOT NULL DEFAULT '[]';
DELETE FROM anilist_chain;

-- A rating per season, or per part of a season where AniList splits one
-- TMDB season into several entries. 0..100 like every other rating.
CREATE TABLE season_rating (
    user_id       INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    title_id      INTEGER NOT NULL REFERENCES title (id) ON DELETE CASCADE,
    season_number INTEGER NOT NULL,
    first_episode INTEGER NOT NULL DEFAULT 1,
    rating        REAL    NOT NULL CHECK (rating >= 0 AND rating <= 100),
    updated_at    TEXT    NOT NULL,
    PRIMARY KEY (user_id, title_id, season_number, first_episode)
);

-- AniList scores used to land on the whole show in Frog. Each belongs to the
-- season (or part) its entry covers, which the placement says exactly.
INSERT OR IGNORE INTO season_rating
       (user_id, title_id, season_number, first_episode, rating, updated_at)
SELECT a.user_id, a.title_id, a.season, a.episode_offset + 1, a.al_score, a.updated_at
  FROM anilist_entry a
  JOIN title t ON t.id = a.title_id
 WHERE a.state = 'placed' AND t.kind = 'tv' AND a.season IS NOT NULL
   AND a.al_score > 0 AND a.al_score <= 100;

-- What Frog last saw for those entries is now that season rating, so the
-- move itself is not mistaken for a change to send back to AniList.
UPDATE anilist_entry
   SET frog_score = COALESCE(al_score, 0)
 WHERE state = 'placed' AND season IS NOT NULL;
