//! Background worker.
//!
//! Deliberately small: expire sessions and sign-in failures, drop stale TMDB cache
//! rows, and re-pull episodes for shows that are still airing so new episodes
//! appear without anyone opening the page. Shorter loops beside it fill
//! credits and logos, build the Airing list once a day, work out air times
//! every few hours, sync AniList, report server failures, and send
//! notifications the minute something comes out.

use chrono::Utc;
use std::time::Duration;

use crate::catalog;
use crate::state::AppState;

const TICK: Duration = Duration::from_secs(60 * 30);

/// Credits backfill runs far more often than the main tick, so a library from
/// before stats existed fills in within the hour rather than over days.
const CREDITS_TICK: Duration = Duration::from_secs(60 * 2);
const CREDITS_BATCH: i64 = 25;

/// How often the server jobs' log is read for new failures. The update timer
/// runs every 15 minutes, so this hears about a failed one well within that.
const OPS_TICK: Duration = Duration::from_secs(60 * 5);

/// Logos from fanart.tv for tracked titles that have none, a batch at a time.
const LOGO_TICK: Duration = Duration::from_secs(60 * 10);

/// Air times from AniList and TVmaze, a batch of shows at a time
/// (`crate::airtime`); each show is worked out again every few hours.
const AIRTIME_TICK: Duration = Duration::from_secs(60 * 2);

/// How often the Airing list is checked; it is built once a day.
const AIRING_TICK: Duration = Duration::from_secs(60 * 15);

/// Titles given TMDB's days back in one pass, after the switch of air-time
/// source (migration 202611111200_airing).
const RESTORE_BATCH: i64 = 10;

/// Notifications go out at the minute something airs.
const ANNOUNCE_TICK: Duration = Duration::from_secs(60);

pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Give the HTTP server a moment before the first sweep.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let credits = tokio::spawn(backfill_credits(state.clone()));
        let _guard = AbortOnDrop(credits);
        let anilist = tokio::spawn(sync_anilist(state.clone()));
        let _anilist_guard = AbortOnDrop(anilist);
        let ops = tokio::spawn(watch_ops(state.clone()));
        let _ops_guard = AbortOnDrop(ops);
        let logos = tokio::spawn(fill_logos(state.clone()));
        let _logos_guard = AbortOnDrop(logos);
        let airtimes = tokio::spawn(fill_airtimes(state.clone()));
        let _airtimes_guard = AbortOnDrop(airtimes);
        let airing = tokio::spawn(build_airing(state.clone()));
        let _airing_guard = AbortOnDrop(airing);
        let announce = tokio::spawn(announce(state.clone()));
        let _announce_guard = AbortOnDrop(announce);
        let twins = tokio::spawn({
            let state = state.clone();
            async move {
                merge_film_twins(&state).await;
            }
        });
        let _twins_guard = AbortOnDrop(twins);
        loop {
            if let Err(e) = tick(&state).await {
                tracing::warn!(error = %e, "worker tick failed");
            }
            tokio::time::sleep(TICK).await;
        }
    })
}

/// How long a TMDB answer past its time is kept for when TMDB cannot be reached.
const STALE_TMDB_DAYS: i64 = 7;

async fn tick(state: &AppState) -> anyhow::Result<()> {
    state.login_failures.sweep();
    state.reset_failures.sweep();
    crate::reset::purge_expired(&state.pool).await?;

    let expired = crate::auth::purge_expired_sessions(&state.pool).await?;
    if expired > 0 {
        tracing::info!(expired, "purged expired sessions");
    }

    // Stale answers stand in while TMDB is down, which a week covers; kept a
    // month they were half the cache on a small server (120 MB of 250).
    let dropped = sqlx::query("DELETE FROM tmdb_cache WHERE expires_at < ?")
        .bind((Utc::now() - chrono::Duration::days(STALE_TMDB_DAYS)).to_rfc3339())
        .execute(&state.pool)
        .await?
        .rows_affected();
    if dropped > 0 {
        tracing::debug!(dropped, "pruned stale TMDB cache rows");
    }

    // The picture cache to its budget. A failure leaves it as it was until
    // the next tick, which is all that is at stake.
    match crate::images::evict(&state.pool, crate::images::budget_bytes(), Utc::now()).await {
        Ok(gone) if gone.pictures > 0 => {
            tracing::info!(
                pictures = gone.pictures,
                bytes = gone.bytes,
                "picture cache trimmed"
            )
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "picture cache could not be trimmed"),
    }

    crate::ratings::refresh_due(state).await;
    fill_missing_episodes(state).await;
    refresh_airing_shows(state).await;
    Ok(())
}

async fn fill_logos(state: AppState) {
    loop {
        crate::fanart::fill_due(&state).await;
        tokio::time::sleep(LOGO_TICK).await;
    }
}

async fn fill_airtimes(state: AppState) {
    loop {
        // Which AniList media is which TMDB show, for anime air times. Kept
        // fresh by the AniList sync too; this covers an instance without it.
        let api = crate::anilist::Api::public(&state);
        if let Err(e) = crate::anilist::ensure_map(&state, api.client).await {
            tracing::debug!(error = %e, "anime map not refreshed");
        }
        restore_tmdb_days(&state).await;
        let done = crate::airtime::refresh_due(&state).await;
        if done > 0 {
            tracing::debug!(done, "air times worked out");
        }
        // A whole batch means more are waiting, as after the day's list is
        // built: carry on, paced by the providers themselves.
        if (done as i64) < crate::airtime::BATCH {
            tokio::time::sleep(AIRTIME_TICK).await;
        } else {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

async fn build_airing(state: AppState) {
    loop {
        crate::airing::refresh_if_due(&state).await;
        tokio::time::sleep(AIRING_TICK).await;
    }
}

/// Episodes whose day an earlier air-time source wrote over TMDB's: read
/// their seasons from TMDB again, which writes TMDB's day back.
pub async fn restore_tmdb_days(state: &AppState) -> usize {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT title_id FROM episode WHERE air_source = 'restore' LIMIT ?",
    )
    .bind(RESTORE_BATCH)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let mut done = 0;
    for (title_id,) in rows {
        match catalog::load(state, title_id).await {
            Ok((_, detail)) => {
                if let Err(e) = catalog::sync_all_seasons(state, title_id, &detail).await {
                    tracing::debug!(title_id, error = %e, "TMDB days not restored yet");
                    break;
                }
            }
            Err(crate::error::AppError::NotFound(_)) => {}
            Err(e) => {
                tracing::debug!(title_id, error = %e, "TMDB days not restored yet");
                break;
            }
        }
        // Whatever TMDB had is written; the mark has done its job.
        let _ = sqlx::query(
            "UPDATE episode SET air_source = NULL WHERE title_id = ? AND air_source = 'restore'",
        )
        .bind(title_id)
        .execute(&state.pool)
        .await;
        reconcile_everyone(state, title_id).await;
        done += 1;
    }
    done
}

async fn announce(state: AppState) {
    loop {
        if let Err(e) = crate::announce::run(&state, Utc::now()).await {
            tracing::warn!(error = %e, "notifications failed");
        }
        tokio::time::sleep(ANNOUNCE_TICK).await;
    }
}

/// Stops the backfill when the worker itself is aborted at shutdown.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// AniList runs on its own short loop: a change made in Frog should reach
/// AniList within a minute or two, not at the next half-hourly tick.
async fn sync_anilist(state: AppState) {
    if state.config.anilist.is_none() {
        return;
    }
    loop {
        match crate::anilist::due(&state.pool).await {
            Ok(users) => {
                for user_id in users {
                    let _ = crate::anilist::sync_user(&state, user_id).await;
                }
            }
            Err(e) => tracing::warn!(error = %e, "could not list AniList accounts"),
        }
        // Looked at twice a minute; `due` decides who is actually synced.
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

async fn watch_ops(state: AppState) {
    loop {
        if let Err(e) = report_ops_failures(&state).await {
            tracing::warn!(error = %e, "could not report server failures");
        }
        tokio::time::sleep(OPS_TICK).await;
    }
}

/// Tell every administrator, once each, about failures the deploy scripts
/// recorded since they were last told (see `crate::ops::new_failures`).
/// Returns how many failures each administrator was told about.
pub async fn report_ops_failures(state: &AppState) -> anyhow::Result<Vec<(i64, usize)>> {
    let events = crate::ops::read(&state.config.database_url);
    if events.is_empty() {
        return Ok(Vec::new());
    }
    let admins: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM app_user WHERE role IN ('owner', 'admin') AND disabled = 0")
            .fetch_all(&state.pool)
            .await?;

    let mut told = Vec::with_capacity(admins.len());
    for (user_id,) in admins {
        let seen: Option<(String,)> =
            sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = 'ops_seen'")
                .bind(user_id)
                .fetch_optional(&state.pool)
                .await?;
        let (failures, mark) =
            crate::ops::new_failures(&events, seen.as_ref().map(|s| s.0.as_str()));

        // Every browser out of reach: keep the mark where it was and try again
        // next time, as new-episode alerts do.
        let mut unreachable = false;
        for f in &failures {
            let result = crate::push::broadcast(state, user_id, &crate::ops::payload(f)).await?;
            if result.sent == 0 && result.failed > 0 {
                unreachable = true;
                break;
            }
        }
        if unreachable {
            continue;
        }

        if let Some(mark) = mark {
            sqlx::query(
                "INSERT INTO app_setting (user_id, key, value, updated_at)
                 VALUES (?, 'ops_seen', ?, ?)
                 ON CONFLICT (user_id, key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(&mark)
            .bind(Utc::now().to_rfc3339())
            .execute(&state.pool)
            .await?;
        }
        if !failures.is_empty() {
            tracing::info!(
                user_id,
                failures = failures.len(),
                "told an administrator about server failures"
            );
        }
        told.push((user_id, failures.len()));
    }
    Ok(told)
}

async fn backfill_credits(state: AppState) {
    loop {
        backfill_credits_once(&state).await;
        tokio::time::sleep(CREDITS_TICK).await;
    }
}

/// AniList-only films that TMDB has too, from before opening one checked:
/// each account's history moves to TMDB's title and the copy points there,
/// so nobody sees the same film twice. Once a start; a film someone's
/// AniList list keeps is left to the sync, which places it by the same rule
/// and would otherwise bring the copy back.
pub async fn merge_film_twins(state: &AppState) -> usize {
    if !state.tmdb.is_configured() {
        return 0;
    }
    let copies: Vec<(i64, String)> = sqlx::query_as(
        "SELECT t.id, x.external_id
           FROM title t
           JOIN title_external x ON x.title_id = t.id AND x.provider = 'anilist' AND x.is_primary = 1
          WHERE t.kind = 'movie' AND t.merged_into IS NULL",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut merged = 0;
    for (from, media_id) in copies {
        let listed: Option<(i64,)> =
            sqlx::query_as("SELECT user_id FROM anilist_entry WHERE media_id = ? LIMIT 1")
                .bind(media_id.parse::<i64>().unwrap_or(0))
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);
        if listed.is_some() {
            continue;
        }
        let to = match catalog::film_twin_of(state, from).await {
            Ok(Some(to)) => to,
            Ok(None) => continue,
            Err(e) => {
                tracing::debug!(title_id = from, error = %e, "film duplicates paused");
                break;
            }
        };
        match crate::store::merge_copy(&state.pool, from, to, "the same film on TMDB").await {
            Ok(()) => merged += 1,
            Err(e) => tracing::warn!(from, to, error = %e, "could not merge a duplicate film"),
        }
    }
    if merged > 0 {
        tracing::info!(merged, "merged AniList films into the TMDB films they are");
    }
    merged
}

/// Fill in credits for titles stored before they were kept. Most details are
/// still in the TMDB cache, so this rarely touches the network.
pub async fn backfill_credits_once(state: &AppState) -> usize {
    if !state.tmdb.is_configured() {
        return 0;
    }
    // Only titles TMDB supplies: an AniList title's credits come with its
    // detail.
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT t.id
           FROM title t
           JOIN title_external x ON x.title_id = t.id AND x.provider = 'tmdb' AND x.is_primary = 1
          WHERE t.people_synced_at IS NULL
            AND (EXISTS (SELECT 1 FROM library_entry l WHERE l.title_id = t.id)
                 OR EXISTS (SELECT 1 FROM watch_event w WHERE w.title_id = t.id))
          LIMIT ?",
    )
    .bind(CREDITS_BATCH)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut done = 0;
    for (title_id,) in rows {
        match catalog::load(state, title_id).await {
            Ok(_) => done += 1,
            Err(e) => {
                // Most likely offline; the rest of the batch would fail too.
                tracing::debug!(title_id, error = %e, "credits backfill paused");
                break;
            }
        }
    }
    if done > 0 {
        tracing::info!(done, "stored credits for older titles");
    }
    done
}

/// Re-sync shows that are still producing episodes and that I am part-way
/// through. Six hours between refreshes is well inside TMDB's rate limits even
/// with a few hundred shows.
async fn refresh_airing_shows(state: &AppState) {
    if !state.tmdb.is_configured() {
        return;
    }

    let cutoff = (Utc::now() - chrono::Duration::hours(6)).to_rfc3339();
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT t.id
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE t.kind = 'tv'
            AND l.status IN ('watching', 'paused', 'planned', 'completed')
            AND COALESCE(t.tmdb_status, '') NOT IN ('Ended', 'Canceled')
            AND COALESCE(t.refreshed_at, '') < ?
          ORDER BY COALESCE(t.refreshed_at, '')
          LIMIT 20",
    )
    .bind(&cutoff)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    for (title_id,) in rows {
        // From whichever provider the show comes from: TMDB, or AniList.
        match catalog::load(state, title_id).await {
            Ok((_, detail)) => {
                if let Err(e) = catalog::sync_all_seasons(state, title_id, &detail).await {
                    tracing::warn!(title_id, error = %e, "season refresh failed");
                    continue;
                }
                // New episodes out, or a season ended: everyone with the show
                // gets the status that now fits.
                reconcile_everyone(state, title_id).await;
            }
            Err(e) => tracing::warn!(title_id, error = %e, "show refresh failed"),
        }
    }
}

async fn reconcile_everyone(state: &AppState, title_id: i64) {
    let people: Vec<(i64,)> =
        sqlx::query_as("SELECT user_id FROM library_entry WHERE title_id = ?")
            .bind(title_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    for (user_id,) in people {
        if let Err(e) = crate::release::reconcile(&state.pool, user_id, title_id).await {
            tracing::warn!(title_id, error = %e, "status refresh failed");
        }
    }
}

/// Shows somebody tracks with no episodes listed, whatever TMDB says of them:
/// a show that has ended is left out of the refresh above, and an older import
/// could leave one like this. Random, so one with nothing on TMDB holds up no
/// other.
async fn fill_missing_episodes(state: &AppState) {
    if !state.tmdb.is_configured() {
        return;
    }
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT t.id FROM library_entry l JOIN title t ON t.id = l.title_id
           JOIN title_external x ON x.title_id = t.id AND x.provider = 'tmdb'
                                AND x.is_primary = 1
          WHERE t.kind = 'tv'
            AND NOT EXISTS (SELECT 1 FROM episode e WHERE e.title_id = t.id)
          ORDER BY RANDOM()
          LIMIT 25",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    for (title_id,) in rows {
        match catalog::ensure_episodes(state, title_id, None).await {
            Ok(()) => reconcile_everyone(state, title_id).await,
            Err(e) => tracing::warn!(title_id, error = %e, "episodes not listed"),
        }
    }
}
