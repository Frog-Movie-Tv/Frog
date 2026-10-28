use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store;

#[derive(Deserialize)]
pub struct WatchBody {
    /// false clears every watch of this episode; true adds one.
    #[serde(default = "yes")]
    pub watched: bool,
    /// ISO timestamp. Defaults to now, which is what a checkmark means.
    pub watched_at: Option<String>,
}

fn yes() -> bool {
    true
}

async fn episode_id(pool: &SqlitePool, title_id: i64, season: i64, episode: i64) -> AppResult<i64> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM episode WHERE title_id = ? AND season_number = ? AND episode_number = ?",
    )
    .bind(title_id)
    .bind(season)
    .bind(episode)
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.0)
        .ok_or_else(|| AppError::NotFound(format!("no S{season}E{episode} for this show")))
}

/// Make sure the show exists locally and its seasons are present, so ticking an
/// episode works even on a title that was never formally added.
async fn resolve(state: &AppState, kind: &str, tmdb_id: i64) -> AppResult<i64> {
    if kind != "tv" {
        return Err(AppError::BadRequest("only shows have episodes".into()));
    }
    match store::find_title_id(&state.pool, kind, tmdb_id).await? {
        Some(id) => Ok(id),
        None => {
            let (id, detail) = store::ensure_title(state, kind, tmdb_id).await?;
            store::sync_all_seasons(state, id, &detail).await?;
            Ok(id)
        }
    }
}

pub async fn set_watched(
    auth: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id, season, episode)): Path<(String, i64, i64, i64)>,
    Json(body): Json<WatchBody>,
) -> AppResult<Json<Value>> {
    let title_id = resolve(&state, &kind, tmdb_id).await?;
    let ep_id = episode_id(&state.pool, title_id, season, episode).await?;

    if body.watched {
        let when = body.watched_at.unwrap_or_else(|| Utc::now().to_rfc3339());
        // Only my own watches make this a rewatch.
        let seen_before: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM watch_event WHERE user_id = ? AND episode_id = ?")
                .bind(auth.user_id())
                .bind(ep_id)
                .fetch_one(&state.pool)
                .await?;

        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch, source)
             VALUES (?, ?, ?, ?, ?, 'manual')",
        )
        .bind(auth.user_id())
        .bind(title_id)
        .bind(ep_id)
        .bind(&when)
        .bind(i64::from(seen_before.0 > 0))
        .execute(&state.pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM watch_event WHERE user_id = ? AND episode_id = ?")
            .bind(auth.user_id())
            .bind(ep_id)
            .execute(&state.pool)
            .await?;
    }

    after_change(&state, auth.user_id(), title_id).await
}

/// When a batch of episodes was watched.
///
/// Marking a whole back catalogue as watched is a statement about the past,
/// and stamping every episode with today's date flooded the diary with
/// hundreds of entries that never happened. So bulk marking asks.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum When {
    /// Just now.
    #[default]
    Today,
    /// Each episode on the day it aired; undated if the air date is unknown.
    Release,
    /// One chosen day for all of them.
    Date,
    /// Watched, but no idea when: counted everywhere, kept out of the diary.
    Unknown,
}

#[derive(Deserialize)]
pub struct BulkWatchBody {
    #[serde(default = "yes")]
    pub watched: bool,
    #[serde(default)]
    pub when: When,
    /// yyyy-mm-dd, required when `when` is `date`.
    pub date: Option<String>,
    /// Specials (season 0 episode numbers) to tick as well, when marking a
    /// whole show: a finished show means its seasons, and specials are chosen.
    #[serde(default)]
    pub specials: Vec<i64>,
}

/// Turn a yyyy-mm-dd into the timestamp watches are stored with, refusing
/// anything that is not a real calendar date or lies in the future.
pub fn evening_of(date: &str) -> AppResult<String> {
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("pick a valid date".into()))?;
    if day > Utc::now().date_naive() {
        return Err(AppError::BadRequest("that date is in the future".into()));
    }
    Ok(format!("{date}T20:00:00Z"))
}

/// Tick or clear every aired episode of one season, or of the whole show when
/// `season` is `None` (specials excluded).
async fn bulk(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    season: Option<i64>,
    body: &BulkWatchBody,
) -> AppResult<()> {
    if !body.watched {
        sqlx::query(
            "DELETE FROM watch_event
              WHERE user_id = ?3
                AND episode_id IN (
                    SELECT id FROM episode
                     WHERE title_id = ?1
                       AND (?2 IS NOT NULL OR season_number > 0)
                       AND (?2 IS NULL OR season_number = ?2))",
        )
        .bind(title_id)
        .bind(season)
        .bind(user_id)
        .execute(&state.pool)
        .await?;
        return Ok(());
    }

    let mode = match body.when {
        When::Today => "today",
        When::Release => "release",
        When::Date => "date",
        When::Unknown => "unknown",
    };
    let chosen = match body.when {
        When::Date => Some(evening_of(body.date.as_deref().unwrap_or(""))?),
        _ => None,
    };

    // Only episodes with no watch yet, so this never inflates rewatches, and
    // only ones that have aired.
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated)
         SELECT ?6, ?1, e.id,
                CASE ?2 WHEN 'release' THEN COALESCE(e.air_date || 'T20:00:00Z', ?3)
                        WHEN 'date' THEN ?4
                        ELSE ?3 END,
                0, 'manual',
                CASE ?2 WHEN 'unknown' THEN 1
                        WHEN 'release' THEN e.air_date IS NULL
                        ELSE 0 END
           FROM episode e
          WHERE e.title_id = ?1
            AND (?5 IS NOT NULL OR e.season_number > 0)
            AND (?5 IS NULL OR e.season_number = ?5)
            AND (e.air_date IS NULL OR e.air_date <= date('now'))
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ?6 AND w.episode_id = e.id)",
    )
    .bind(title_id)
    .bind(mode)
    .bind(Utc::now().to_rfc3339())
    .bind(chosen.as_deref())
    .bind(season)
    .bind(user_id)
    .execute(&state.pool)
    .await?;

    if season.is_none() {
        for special in &body.specials {
            sqlx::query(
                "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                          source, undated)
                 SELECT ?6, ?1, e.id,
                        CASE ?2 WHEN 'release' THEN COALESCE(e.air_date || 'T20:00:00Z', ?3)
                                WHEN 'date' THEN ?4
                                ELSE ?3 END,
                        0, 'manual',
                        CASE ?2 WHEN 'unknown' THEN 1
                                WHEN 'release' THEN e.air_date IS NULL
                                ELSE 0 END
                   FROM episode e
                  WHERE e.title_id = ?1 AND e.season_number = 0 AND e.episode_number = ?5
                    AND (e.air_date IS NULL OR e.air_date <= date('now'))
                    AND NOT EXISTS (
                          SELECT 1 FROM watch_event w
                           WHERE w.user_id = ?6 AND w.episode_id = e.id)",
            )
            .bind(title_id)
            .bind(mode)
            .bind(Utc::now().to_rfc3339())
            .bind(chosen.as_deref())
            .bind(special)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
        }
    }

    Ok(())
}

/// Tick or clear a whole season in one go.
pub async fn set_season_watched(
    auth: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id, season)): Path<(String, i64, i64)>,
    Json(body): Json<BulkWatchBody>,
) -> AppResult<Json<Value>> {
    let title_id = resolve(&state, &kind, tmdb_id).await?;
    bulk(&state, auth.user_id(), title_id, Some(season), &body).await?;
    after_change(&state, auth.user_id(), title_id).await
}

/// Tick or clear every aired episode of a show.
pub async fn set_show_watched(
    auth: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id)): Path<(String, i64)>,
    Json(body): Json<BulkWatchBody>,
) -> AppResult<Json<Value>> {
    let title_id = resolve(&state, &kind, tmdb_id).await?;
    bulk(&state, auth.user_id(), title_id, None, &body).await?;
    after_change(&state, auth.user_id(), title_id).await
}

#[derive(Deserialize)]
pub struct EpisodeStateBody {
    /// 0..100, or null to clear.
    pub rating: Option<f64>,
    pub notes: Option<String>,
}

pub async fn set_state(
    auth: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id, season, episode)): Path<(String, i64, i64, i64)>,
    Json(body): Json<EpisodeStateBody>,
) -> AppResult<Json<Value>> {
    if let Some(r) = body.rating {
        if !(0.0..=100.0).contains(&r) {
            return Err(AppError::BadRequest(
                "rating must be between 0 and 100".into(),
            ));
        }
    }

    let title_id = resolve(&state, &kind, tmdb_id).await?;
    let ep_id = episode_id(&state.pool, title_id, season, episode).await?;

    sqlx::query(
        "INSERT INTO episode_state (user_id, episode_id, rating, notes, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (user_id, episode_id) DO UPDATE SET
            rating = excluded.rating,
            notes = excluded.notes,
            updated_at = excluded.updated_at",
    )
    .bind(auth.user_id())
    .bind(ep_id)
    .bind(body.rating)
    .bind(&body.notes)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// After any watch change, nudge the library entry to match reality: a show you
/// start becomes "watching", and finishing the last episode completes it.
async fn after_change(state: &AppState, user_id: i64, title_id: i64) -> AppResult<Json<Value>> {
    crate::anilist::touch(&state.pool, user_id).await;
    let progress = store::progress(&state.pool, user_id, title_id).await?;
    let entry = store::get_entry(&state.pool, user_id, title_id).await?;

    let desired = if progress.total > 0
        && progress.watched >= progress.total
        && crate::routes::settings::auto_complete(&state.pool, user_id).await
    {
        Some("completed")
    } else if progress.watched > 0 {
        match entry.as_ref().map(|e| e.status.as_str()) {
            // Never drag something out of paused or dropped behind your back.
            Some("paused") | Some("dropped") => None,
            Some("watching") => None,
            _ => Some("watching"),
        }
    } else {
        None
    };

    if let Some(status) = desired {
        if entry.as_ref().map(|e| e.status.as_str()) != Some(status) {
            let patch = store::LibraryPatch {
                status: Some(status.to_string()),
                ..Default::default()
            };
            store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
        }
    }

    let row = store::get_title(&state.pool, title_id).await?;
    let item = store::build_item(&state.pool, user_id, row).await?;
    Ok(Json(json!({ "item": item })))
}
