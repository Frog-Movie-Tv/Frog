//! Background worker.
//!
//! Deliberately small: expire sessions, drop stale TMDB cache
//! rows, and re-pull episodes for shows that are still airing so new episodes
//! appear without anyone opening the page. Phases 2 and 4 hang the calendar,
//! notification and scrobble-pull jobs off the same loop.

use chrono::Utc;
use std::time::Duration;

use crate::state::AppState;
use crate::store;

const TICK: Duration = Duration::from_secs(60 * 30);

/// Credits backfill runs far more often than the main tick, so a library from
/// before stats existed fills in within the hour rather than over days.
const CREDITS_TICK: Duration = Duration::from_secs(60 * 2);
const CREDITS_BATCH: i64 = 25;

pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Give the HTTP server a moment before the first sweep.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let credits = tokio::spawn(backfill_credits(state.clone()));
        let _guard = AbortOnDrop(credits);
        let anilist = tokio::spawn(sync_anilist(state.clone()));
        let _anilist_guard = AbortOnDrop(anilist);
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
    if let Err(e) = announce_new_episodes(state, chrono::Local::now().naive_local()).await {
        tracing::warn!(error = %e, "new-episode notifications failed");
    }
    Ok(())
}

/// One notification: every new episode of one show on one day.
#[derive(Debug, PartialEq)]
pub struct Announcement {
    pub episode_ids: Vec<i64>,
    pub tmdb_id: i64,
    pub show: String,
    pub poster_path: Option<String>,
    /// (season, episode, name)
    pub episodes: Vec<(i64, i64, Option<String>)>,
    pub aired_today: bool,
}

/// (episode id, tmdb id, show, poster, season, episode, episode name, air date)
type DueRow = (
    i64,
    i64,
    String,
    Option<String>,
    i64,
    i64,
    Option<String>,
    String,
);

/// New episodes of followed shows that air today, or aired yesterday while the
/// server was off, not yet announced and not already watched.
pub async fn due_announcements(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    today: chrono::NaiveDate,
) -> anyhow::Result<Vec<Announcement>> {
    let yesterday = today - chrono::Duration::days(1);
    let rows: Vec<DueRow> = sqlx::query_as(
        "SELECT e.id, t.tmdb_id, t.name, t.poster_path, e.season_number, e.episode_number,
                e.name, e.air_date
           FROM episode e
           JOIN title t ON t.id = e.title_id
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = ?1
          WHERE t.kind = 'tv'
            AND l.status IN ('watching', 'planned', 'paused')
            AND l.notify = 1
            AND e.season_number > 0
            AND e.air_date IN (?2, ?3)
            AND NOT EXISTS (
                  SELECT 1 FROM push_sent p
                   WHERE p.user_id = ?1 AND p.episode_id = e.id)
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ?1 AND w.episode_id = e.id)
          ORDER BY t.name, e.season_number, e.episode_number",
    )
    .bind(user_id)
    .bind(today.to_string())
    .bind(yesterday.to_string())
    .fetch_all(pool)
    .await?;

    let mut out: Vec<Announcement> = Vec::new();
    for (id, tmdb_id, show, poster_path, season, episode, name, air_date) in rows {
        let aired_today = air_date == today.to_string();
        match out.last_mut() {
            Some(a) if a.tmdb_id == tmdb_id => {
                a.episode_ids.push(id);
                a.episodes.push((season, episode, name));
                a.aired_today |= aired_today;
            }
            _ => out.push(Announcement {
                episode_ids: vec![id],
                tmdb_id,
                show,
                poster_path,
                episodes: vec![(season, episode, name)],
                aired_today,
            }),
        }
    }
    Ok(out)
}

impl Announcement {
    pub fn payload(&self) -> serde_json::Value {
        let code = |s: i64, e: i64| format!("S{s:02}E{e:02}");
        let when = if self.aired_today {
            "out today"
        } else {
            "out now"
        };
        let body = match self.episodes.as_slice() {
            [(s, e, Some(name))] => format!("{} · {name} is {when}", code(*s, *e)),
            [(s, e, None)] => format!("{} is {when}", code(*s, *e)),
            [first, .., last] => format!(
                "{} new episodes, {}–{}, are {when}",
                self.episodes.len(),
                code(first.0, first.1),
                code(last.0, last.1)
            ),
            [] => String::new(),
        };
        serde_json::json!({
            "title": self.show,
            "body": body,
            "url": format!("/title/tv/{}", self.tmdb_id),
            // One notification per show: a later one replaces it, never stacks.
            "tag": format!("frog-tv-{}", self.tmdb_id),
            "image": self.poster_path.as_ref().map(|p| format!("/img/w342{p}")),
        })
    }
}

/// The hour, in the server's local time, from which this account hears about
/// new episodes: its own choice in Settings, else FROG_NOTIFY_HOUR.
pub async fn notify_hour(pool: &sqlx::SqlitePool, user_id: i64, default: u32) -> u32 {
    let raw: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = 'preferences'")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    raw.and_then(|(json,)| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|prefs| prefs.get("notifyHour").and_then(serde_json::Value::as_u64))
        .filter(|hour| *hour < 24)
        .map(|hour| hour as u32)
        .unwrap_or(default)
}

async fn announce_new_episodes(state: &AppState, now: chrono::NaiveDateTime) -> anyhow::Result<()> {
    use chrono::Timelike;
    // Only accounts with a subscribed browser can be told anything, and each
    // is told about its own shows from its own library.
    let listeners: Vec<(i64,)> = sqlx::query_as("SELECT DISTINCT user_id FROM push_subscription")
        .fetch_all(&state.pool)
        .await?;

    for (user_id,) in listeners {
        // Evening by default: when most episodes are watchable.
        if now.hour() < notify_hour(&state.pool, user_id, state.config.notify_hour).await {
            continue;
        }
        for a in due_announcements(&state.pool, user_id, now.date()).await? {
            let result = crate::push::broadcast(state, user_id, &a.payload()).await?;
            // Nobody reached because the network is down: try again next tick.
            if result.sent == 0 && result.failed > 0 {
                continue;
            }
            let sent_at = Utc::now().to_rfc3339();
            for id in &a.episode_ids {
                sqlx::query(
                    "INSERT OR IGNORE INTO push_sent (user_id, episode_id, sent_at)
                     VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(id)
                .bind(&sent_at)
                .execute(&state.pool)
                .await?;
            }
            tracing::info!(
                user_id,
                show = %a.show,
                episodes = a.episode_ids.len(),
                "announced new episodes"
            );
        }
    }
    Ok(())
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
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn backfill_credits(state: AppState) {
    loop {
        backfill_credits_once(&state).await;
        tokio::time::sleep(CREDITS_TICK).await;
    }
}

/// Fill in credits for titles stored before they were kept. Most details are
/// still in the TMDB cache, so this rarely touches the network.
pub async fn backfill_credits_once(state: &AppState) -> usize {
    if !state.tmdb.is_configured() {
        return 0;
    }
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id
           FROM title t
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
    for (kind, tmdb_id) in rows {
        match store::ensure_title(state, &kind, tmdb_id).await {
            Ok(_) => done += 1,
            Err(e) => {
                // Most likely offline; the rest of the batch would fail too.
                tracing::debug!(tmdb_id, error = %e, "credits backfill paused");
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
