-- AniList entries TMDB does not have, syncs that fail without pretending
-- they worked, and old episode ratings carried over to seasons.

-- Shared: a title Frog keeps without a TMDB entry, made from the AniList
-- entry it came from. Its tmdb_id is that AniList id made negative, so its
-- address and every (kind, tmdb_id) lookup keep working, and a real TMDB id
-- (always positive) can never collide with it. When TMDB turns up, each
-- account's data moves to the TMDB title and merged_into points there; the
-- row itself is never reused for anything else.
ALTER TABLE title ADD COLUMN anilist_id INTEGER;
ALTER TABLE title ADD COLUMN merged_into INTEGER REFERENCES title (id) ON DELETE SET NULL;
CREATE UNIQUE INDEX idx_title_anilist ON title (anilist_id) WHERE anilist_id IS NOT NULL;

-- Episodes watched of a title Frog has no episode list for (an AniList-only
-- show). Ticks are the progress of every other show; this is only read when
-- a title has no episodes.
ALTER TABLE library_entry ADD COLUMN progress INTEGER;

-- synced_at is the last sync that finished. A failed one records when it
-- was tried and when to try again, backing off as failures pile up; a
-- refused token waits for a reconnect instead of being retried.
ALTER TABLE anilist_link ADD COLUMN attempted_at TEXT;
ALTER TABLE anilist_link ADD COLUMN failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE anilist_link ADD COLUMN retry_at TEXT;
ALTER TABLE anilist_link ADD COLUMN needs_reconnect INTEGER NOT NULL DEFAULT 0;
UPDATE anilist_link SET attempted_at = synced_at;

-- An entry TMDB could not place is looked for again on a schedule rather
-- than every few minutes; any change on AniList's side looks again at once.
ALTER TABLE anilist_entry ADD COLUMN tries INTEGER NOT NULL DEFAULT 0;
ALTER TABLE anilist_entry ADD COLUMN next_try_at TEXT;
-- No longer on the AniList list. Nothing in Frog is removed for it.
ALTER TABLE anilist_entry ADD COLUMN gone_at TEXT;

-- Every time one account's data moved from one title to another (an
-- AniList-only title that TMDB later found, or a person's correction), with
-- what both sides held before, so a merge can be looked into or undone.
CREATE TABLE title_merge (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    from_title  INTEGER NOT NULL,
    to_title    INTEGER NOT NULL,
    reason      TEXT    NOT NULL,
    before_json TEXT    NOT NULL,
    merged_at   TEXT    NOT NULL
);

CREATE INDEX idx_title_merge_user ON title_merge (user_id, merged_at);

-- Episode ratings become season ratings where that is unambiguous: every
-- rated episode of the season has the same rating, the season has no rating
-- of its own yet, and no AniList entry of the same account rates the show
-- in parts (those seasons took AniList's scores already). Everything else
-- stays where it is in episode_state, untouched, for nothing is thrown away.
INSERT OR IGNORE INTO season_rating
       (user_id, title_id, season_number, first_episode, rating, updated_at)
SELECT s.user_id, e.title_id, e.season_number, 1, MIN(s.rating), MAX(s.updated_at)
  FROM episode_state s
  JOIN episode e ON e.id = s.episode_id
  JOIN title t ON t.id = e.title_id
 WHERE s.rating IS NOT NULL AND s.rating >= 0 AND s.rating <= 100
   AND e.season_number > 0 AND t.kind = 'tv'
   AND NOT EXISTS (SELECT 1 FROM anilist_entry a
                    WHERE a.user_id = s.user_id AND a.title_id = e.title_id
                      AND a.state = 'placed')
 GROUP BY s.user_id, e.title_id, e.season_number
HAVING COUNT(DISTINCT s.rating) = 1;
