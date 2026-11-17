-- One timestamp format: `YYYY-MM-DDTHH:MM:SSZ`, UTC to the second
-- (`crate::clock`). Rust used to write `+00:00` with nanoseconds, imports and
-- day-only watches `Z`, and the badge triggers milliseconds; SQL compares
-- these columns as text, which is only right when every row has one shape.
-- Day-only values (`2026-11-16`) are not timestamps and stay as they are.
-- Additive: an older release reads both shapes.

UPDATE airing_film SET listed_at = strftime('%Y-%m-%dT%H:%M:%SZ', listed_at)
 WHERE listed_at GLOB '????-??-??T*' AND listed_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', listed_at) IS NOT NULL;
UPDATE airing_show SET listed_at = strftime('%Y-%m-%dT%H:%M:%SZ', listed_at)
 WHERE listed_at GLOB '????-??-??T*' AND listed_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', listed_at) IS NOT NULL;
UPDATE airing_state SET started_at = strftime('%Y-%m-%dT%H:%M:%SZ', started_at)
 WHERE started_at GLOB '????-??-??T*' AND started_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', started_at) IS NOT NULL;
UPDATE airing_state SET listed_at = strftime('%Y-%m-%dT%H:%M:%SZ', listed_at)
 WHERE listed_at GLOB '????-??-??T*' AND listed_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', listed_at) IS NOT NULL;
UPDATE airing_state SET schedules_at = strftime('%Y-%m-%dT%H:%M:%SZ', schedules_at)
 WHERE schedules_at GLOB '????-??-??T*' AND schedules_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', schedules_at) IS NOT NULL;
UPDATE airtime_show SET tvmaze_checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', tvmaze_checked_at)
 WHERE tvmaze_checked_at GLOB '????-??-??T*' AND tvmaze_checked_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', tvmaze_checked_at) IS NOT NULL;
UPDATE airtime_show SET anilist_checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', anilist_checked_at)
 WHERE anilist_checked_at GLOB '????-??-??T*' AND anilist_checked_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', anilist_checked_at) IS NOT NULL;
UPDATE airtime_show SET times_at = strftime('%Y-%m-%dT%H:%M:%SZ', times_at)
 WHERE times_at GLOB '????-??-??T*' AND times_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', times_at) IS NOT NULL;
UPDATE anilist_chain SET fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at)
 WHERE fetched_at GLOB '????-??-??T*' AND fetched_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at) IS NOT NULL;
UPDATE anilist_entry SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
 WHERE updated_at GLOB '????-??-??T*' AND updated_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) IS NOT NULL;
UPDATE anilist_entry SET next_airing_at = strftime('%Y-%m-%dT%H:%M:%SZ', next_airing_at)
 WHERE next_airing_at GLOB '????-??-??T*' AND next_airing_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', next_airing_at) IS NOT NULL;
UPDATE anilist_entry SET next_try_at = strftime('%Y-%m-%dT%H:%M:%SZ', next_try_at)
 WHERE next_try_at GLOB '????-??-??T*' AND next_try_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', next_try_at) IS NOT NULL;
UPDATE anilist_entry SET gone_at = strftime('%Y-%m-%dT%H:%M:%SZ', gone_at)
 WHERE gone_at GLOB '????-??-??T*' AND gone_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', gone_at) IS NOT NULL;
UPDATE anilist_link SET expires_at = strftime('%Y-%m-%dT%H:%M:%SZ', expires_at)
 WHERE expires_at GLOB '????-??-??T*' AND expires_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) IS NOT NULL;
UPDATE anilist_link SET connected_at = strftime('%Y-%m-%dT%H:%M:%SZ', connected_at)
 WHERE connected_at GLOB '????-??-??T*' AND connected_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', connected_at) IS NOT NULL;
UPDATE anilist_link SET synced_at = strftime('%Y-%m-%dT%H:%M:%SZ', synced_at)
 WHERE synced_at GLOB '????-??-??T*' AND synced_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', synced_at) IS NOT NULL;
UPDATE anilist_link SET attempted_at = strftime('%Y-%m-%dT%H:%M:%SZ', attempted_at)
 WHERE attempted_at GLOB '????-??-??T*' AND attempted_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', attempted_at) IS NOT NULL;
UPDATE anilist_link SET retry_at = strftime('%Y-%m-%dT%H:%M:%SZ', retry_at)
 WHERE retry_at GLOB '????-??-??T*' AND retry_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', retry_at) IS NOT NULL;
UPDATE anilist_media SET fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at)
 WHERE fetched_at GLOB '????-??-??T*' AND fetched_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at) IS NOT NULL;
UPDATE anilist_oauth SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE anime_map_state SET fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at)
 WHERE fetched_at GLOB '????-??-??T*' AND fetched_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at) IS NOT NULL;
UPDATE api_token SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE api_token SET last_used_at = strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at)
 WHERE last_used_at GLOB '????-??-??T*' AND last_used_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at) IS NOT NULL;
UPDATE app_setting SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
 WHERE updated_at GLOB '????-??-??T*' AND updated_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) IS NOT NULL;
UPDATE app_user SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE app_user SET last_sign_in_at = strftime('%Y-%m-%dT%H:%M:%SZ', last_sign_in_at)
 WHERE last_sign_in_at GLOB '????-??-??T*' AND last_sign_in_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', last_sign_in_at) IS NOT NULL;
UPDATE badge SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE badge SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
 WHERE updated_at GLOB '????-??-??T*' AND updated_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) IS NOT NULL;
UPDATE badge_award SET awarded_at = strftime('%Y-%m-%dT%H:%M:%SZ', awarded_at)
 WHERE awarded_at GLOB '????-??-??T*' AND awarded_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', awarded_at) IS NOT NULL;
UPDATE chapter_read SET read_at = strftime('%Y-%m-%dT%H:%M:%SZ', read_at)
 WHERE read_at GLOB '????-??-??T*' AND read_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', read_at) IS NOT NULL;
UPDATE chapter_read SET logged_at = strftime('%Y-%m-%dT%H:%M:%SZ', logged_at)
 WHERE logged_at GLOB '????-??-??T*' AND logged_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', logged_at) IS NOT NULL;
UPDATE episode_state SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
 WHERE updated_at GLOB '????-??-??T*' AND updated_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) IS NOT NULL;
UPDATE extension_log SET logged_at = strftime('%Y-%m-%dT%H:%M:%SZ', logged_at)
 WHERE logged_at GLOB '????-??-??T*' AND logged_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', logged_at) IS NOT NULL;
UPDATE image_cache SET fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at)
 WHERE fetched_at GLOB '????-??-??T*' AND fetched_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at) IS NOT NULL;
UPDATE image_cache SET served_at = strftime('%Y-%m-%dT%H:%M:%SZ', served_at)
 WHERE served_at GLOB '????-??-??T*' AND served_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', served_at) IS NOT NULL;
UPDATE invite SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE invite SET expires_at = strftime('%Y-%m-%dT%H:%M:%SZ', expires_at)
 WHERE expires_at GLOB '????-??-??T*' AND expires_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) IS NOT NULL;
UPDATE invite SET used_at = strftime('%Y-%m-%dT%H:%M:%SZ', used_at)
 WHERE used_at GLOB '????-??-??T*' AND used_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', used_at) IS NOT NULL;
UPDATE library_entry SET started_at = strftime('%Y-%m-%dT%H:%M:%SZ', started_at)
 WHERE started_at GLOB '????-??-??T*' AND started_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', started_at) IS NOT NULL;
UPDATE library_entry SET finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', finished_at)
 WHERE finished_at GLOB '????-??-??T*' AND finished_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', finished_at) IS NOT NULL;
UPDATE library_entry SET added_at = strftime('%Y-%m-%dT%H:%M:%SZ', added_at)
 WHERE added_at GLOB '????-??-??T*' AND added_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', added_at) IS NOT NULL;
UPDATE library_entry SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
 WHERE updated_at GLOB '????-??-??T*' AND updated_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) IS NOT NULL;
UPDATE notification SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE notification SET read_at = strftime('%Y-%m-%dT%H:%M:%SZ', read_at)
 WHERE read_at GLOB '????-??-??T*' AND read_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', read_at) IS NOT NULL;
UPDATE notification SET removed_at = strftime('%Y-%m-%dT%H:%M:%SZ', removed_at)
 WHERE removed_at GLOB '????-??-??T*' AND removed_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', removed_at) IS NOT NULL;
UPDATE password_reset SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE password_reset SET expires_at = strftime('%Y-%m-%dT%H:%M:%SZ', expires_at)
 WHERE expires_at GLOB '????-??-??T*' AND expires_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) IS NOT NULL;
UPDATE password_reset SET used_at = strftime('%Y-%m-%dT%H:%M:%SZ', used_at)
 WHERE used_at GLOB '????-??-??T*' AND used_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', used_at) IS NOT NULL;
UPDATE push_sent SET sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', sent_at)
 WHERE sent_at GLOB '????-??-??T*' AND sent_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', sent_at) IS NOT NULL;
UPDATE push_sent_title SET sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', sent_at)
 WHERE sent_at GLOB '????-??-??T*' AND sent_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', sent_at) IS NOT NULL;
UPDATE push_subscription SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE push_subscription SET last_ok_at = strftime('%Y-%m-%dT%H:%M:%SZ', last_ok_at)
 WHERE last_ok_at GLOB '????-??-??T*' AND last_ok_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', last_ok_at) IS NOT NULL;
UPDATE push_vapid SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE recommendation SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE recommendation SET seen_at = strftime('%Y-%m-%dT%H:%M:%SZ', seen_at)
 WHERE seen_at GLOB '????-??-??T*' AND seen_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', seen_at) IS NOT NULL;
UPDATE recommendation SET answered_at = strftime('%Y-%m-%dT%H:%M:%SZ', answered_at)
 WHERE answered_at GLOB '????-??-??T*' AND answered_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', answered_at) IS NOT NULL;
UPDATE recovery_code SET used_at = strftime('%Y-%m-%dT%H:%M:%SZ', used_at)
 WHERE used_at GLOB '????-??-??T*' AND used_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', used_at) IS NOT NULL;
UPDATE season_rating SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
 WHERE updated_at GLOB '????-??-??T*' AND updated_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) IS NOT NULL;
UPDATE session SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE session SET expires_at = strftime('%Y-%m-%dT%H:%M:%SZ', expires_at)
 WHERE expires_at GLOB '????-??-??T*' AND expires_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) IS NOT NULL;
UPDATE shelf SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE studio_match SET checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', checked_at)
 WHERE checked_at GLOB '????-??-??T*' AND checked_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', checked_at) IS NOT NULL;
UPDATE title SET refreshed_at = strftime('%Y-%m-%dT%H:%M:%SZ', refreshed_at)
 WHERE refreshed_at GLOB '????-??-??T*' AND refreshed_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', refreshed_at) IS NOT NULL;
UPDATE title SET people_synced_at = strftime('%Y-%m-%dT%H:%M:%SZ', people_synced_at)
 WHERE people_synced_at GLOB '????-??-??T*' AND people_synced_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', people_synced_at) IS NOT NULL;
UPDATE title_external SET gone_at = strftime('%Y-%m-%dT%H:%M:%SZ', gone_at)
 WHERE gone_at GLOB '????-??-??T*' AND gone_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', gone_at) IS NOT NULL;
UPDATE title_external SET added_at = strftime('%Y-%m-%dT%H:%M:%SZ', added_at)
 WHERE added_at GLOB '????-??-??T*' AND added_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', added_at) IS NOT NULL;
UPDATE title_logo_check SET checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', checked_at)
 WHERE checked_at GLOB '????-??-??T*' AND checked_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', checked_at) IS NOT NULL;
UPDATE title_logo_check SET next_at = strftime('%Y-%m-%dT%H:%M:%SZ', next_at)
 WHERE next_at GLOB '????-??-??T*' AND next_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', next_at) IS NOT NULL;
UPDATE title_merge SET merged_at = strftime('%Y-%m-%dT%H:%M:%SZ', merged_at)
 WHERE merged_at GLOB '????-??-??T*' AND merged_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', merged_at) IS NOT NULL;
UPDATE title_merge SET undone_at = strftime('%Y-%m-%dT%H:%M:%SZ', undone_at)
 WHERE undone_at GLOB '????-??-??T*' AND undone_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', undone_at) IS NOT NULL;
UPDATE title_rating SET fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at)
 WHERE fetched_at GLOB '????-??-??T*' AND fetched_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at) IS NOT NULL;
UPDATE title_rating_check SET checked_at = strftime('%Y-%m-%dT%H:%M:%SZ', checked_at)
 WHERE checked_at GLOB '????-??-??T*' AND checked_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', checked_at) IS NOT NULL;
UPDATE title_rating_check SET next_at = strftime('%Y-%m-%dT%H:%M:%SZ', next_at)
 WHERE next_at GLOB '????-??-??T*' AND next_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', next_at) IS NOT NULL;
UPDATE tmdb_cache SET fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at)
 WHERE fetched_at GLOB '????-??-??T*' AND fetched_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', fetched_at) IS NOT NULL;
UPDATE tmdb_cache SET expires_at = strftime('%Y-%m-%dT%H:%M:%SZ', expires_at)
 WHERE expires_at GLOB '????-??-??T*' AND expires_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) IS NOT NULL;
UPDATE user_image SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
 WHERE updated_at GLOB '????-??-??T*' AND updated_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) IS NOT NULL;
UPDATE watch_event SET watched_at = strftime('%Y-%m-%dT%H:%M:%SZ', watched_at)
 WHERE watched_at GLOB '????-??-??T*' AND watched_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', watched_at) IS NOT NULL;
UPDATE watch_event SET logged_at = strftime('%Y-%m-%dT%H:%M:%SZ', logged_at)
 WHERE logged_at GLOB '????-??-??T*' AND logged_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', logged_at) IS NOT NULL;
UPDATE watch_with SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;
UPDATE watching_with SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
 WHERE created_at GLOB '????-??-??T*' AND created_at NOT GLOB '????-??-??T??:??:??Z'
   AND strftime('%Y-%m-%dT%H:%M:%SZ', created_at) IS NOT NULL;

-- The badge triggers write the same shape.
DROP TRIGGER IF EXISTS watch_event_logged;
CREATE TRIGGER watch_event_logged AFTER INSERT ON watch_event
WHEN NEW.logged_how IS NULL
BEGIN
    UPDATE watch_event
       SET logged_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
           logged_how = CASE NEW.source
                            WHEN 'import' THEN 'import'
                            WHEN 'anilist' THEN CASE WHEN NEW.undated THEN 'anilist_first'
                                                     ELSE 'anilist' END
                            WHEN 'extension' THEN 'extension'
                            ELSE 'tick' END
     WHERE id = NEW.id;
END;

DROP TRIGGER IF EXISTS chapter_read_logged;
CREATE TRIGGER chapter_read_logged AFTER INSERT ON chapter_read
WHEN NEW.logged_how IS NULL
BEGIN
    UPDATE chapter_read
       SET logged_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
           logged_how = CASE NEW.source
                            WHEN 'import' THEN 'import'
                            WHEN 'anilist' THEN 'anilist'
                            ELSE 'tick' END
     WHERE id = NEW.id;
END;
