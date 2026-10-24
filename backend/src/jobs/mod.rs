//! Background worker.
//!
//! Phase 1 keeps it deliberately small: expire sessions, drop stale TMDB cache
//! rows, and re-pull episodes for shows that are still airing so new episodes
//! appear without anyone opening the page. Phases 2 and 4 hang the calendar,
//! notification and scrobble-pull jobs off the same loop.

use chrono::Utc;
use std::time::Duration;

use crate::state::AppState;
use crate::store;

const TICK: Duration = Duration::from_secs(60 * 30);

pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Give the HTTP server a moment before the first sweep.
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            if let Err(e) = tick(&state).await {
                tracing::warn!(error = %e, "worker tick failed");
            }
            tokio::time::sleep(TICK).await;
        }
    })
}

async fn tick(state: &AppState) -> anyhow::Result<()> {
    let expired = crate::auth::purge_expired_sessions(&state.pool).await?;
    if expired > 0 {
        tracing::info!(expired, "purged expired sessions");
    }

    let dropped = sqlx::query("DELETE FROM tmdb_cache WHERE expires_at < ?")
        .bind((Utc::now() - chrono::Duration::days(30)).to_rfc3339())
        .execute(&state.pool)
        .await?
        .rows_affected();
    if dropped > 0 {
        tracing::debug!(dropped, "pruned stale TMDB cache rows");
    }

    refresh_airing_shows(state).await;
    Ok(())
}

/// Re-sync shows that are still producing episodes and that I am part-way
/// through. Six hours between refreshes is well inside TMDB's rate limits even
/// with a few hundred shows.
async fn refresh_airing_shows(state: &AppState) {
    if !state.tmdb.is_configured() {
        return;
    }

    let cutoff = (Utc::now() - chrono::Duration::hours(6)).to_rfc3339();
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT t.id, t.tmdb_id
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE t.kind = 'tv'
            AND l.status IN ('watching', 'paused', 'planned')
            AND COALESCE(t.tmdb_status, '') NOT IN ('Ended', 'Canceled')
            AND COALESCE(t.refreshed_at, '') < ?
          ORDER BY COALESCE(t.refreshed_at, '')
          LIMIT 20",
    )
    .bind(&cutoff)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    for (title_id, tmdb_id) in rows {
        match store::ensure_title(state, "tv", tmdb_id).await {
            Ok((_, detail)) => {
                if let Err(e) = store::sync_all_seasons(state, title_id, &detail).await {
                    tracing::warn!(tmdb_id, error = %e, "season refresh failed");
                }
            }
            Err(e) => tracing::warn!(tmdb_id, error = %e, "show refresh failed"),
        }
    }
}
