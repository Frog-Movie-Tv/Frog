use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::auth::Auth;
use crate::bulk::{tick_aired, Marking, When};
use crate::catalog;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store;

#[derive(Deserialize)]
pub struct WatchBody {
    /// Counted, but kept out of the diary. Lets undo put back a watch that
    /// had no date exactly as it was.
    #[serde(default)]
    pub undated: bool,
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

/// The show by Frog's id, with its episodes listed, so ticking one works even
/// on a show that was never formally added.
async fn show(state: &AppState, title_id: i64) -> AppResult<i64> {
    if catalog::get_title(&state.pool, title_id).await?.kind != "tv" {
        return Err(AppError::BadRequest("only shows have episodes".into()));
    }
    catalog::ensure_episodes(state, title_id, None).await?;
    Ok(title_id)
}

pub async fn set_watched(
    auth: Auth,
    State(state): State<AppState>,
    Path((title_id, season, episode)): Path<(i64, i64, i64)>,
    Json(body): Json<WatchBody>,
) -> AppResult<Json<Value>> {
    let title_id = show(&state, title_id).await?;
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
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                      source, undated)
             VALUES (?, ?, ?, ?, ?, 'manual', ?)",
        )
        .bind(auth.user_id())
        .bind(title_id)
        .bind(ep_id)
        .bind(&when)
        .bind(i64::from(seen_before.0 > 0))
        .bind(i64::from(body.undated))
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

/// Tick every aired episode of one season, or of the whole show when `season`
/// is `None` (specials excluded), or clear them.
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
    let marking = Marking {
        when: body.when,
        date: body.date.clone(),
    };
    tick_aired(
        &state.pool,
        user_id,
        title_id,
        season,
        &marking,
        &body.specials,
    )
    .await
}

/// Tick or clear a whole season in one go.
pub async fn set_season_watched(
    auth: Auth,
    State(state): State<AppState>,
    Path((title_id, season)): Path<(i64, i64)>,
    Json(body): Json<BulkWatchBody>,
) -> AppResult<Json<Value>> {
    let title_id = show(&state, title_id).await?;
    bulk(&state, auth.user_id(), title_id, Some(season), &body).await?;
    after_change(&state, auth.user_id(), title_id).await
}

/// Tick or clear every aired episode of a show.
pub async fn set_show_watched(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
    Json(body): Json<BulkWatchBody>,
) -> AppResult<Json<Value>> {
    let title_id = show(&state, title_id).await?;
    bulk(&state, auth.user_id(), title_id, None, &body).await?;
    after_change(&state, auth.user_id(), title_id).await
}

#[derive(Deserialize)]
pub struct EpisodeStateBody {
    /// Episodes are no longer rated: seasons are. Accepted from older
    /// clients and ignored; a rating already stored stays as it was.
    #[allow(dead_code)]
    pub rating: Option<f64>,
    pub notes: Option<String>,
}

pub async fn set_state(
    auth: Auth,
    State(state): State<AppState>,
    Path((title_id, season, episode)): Path<(i64, i64, i64)>,
    Json(body): Json<EpisodeStateBody>,
) -> AppResult<Json<Value>> {
    let title_id = show(&state, title_id).await?;
    let ep_id = episode_id(&state.pool, title_id, season, episode).await?;

    sqlx::query(
        "INSERT INTO episode_state (user_id, episode_id, rating, notes, updated_at)
         VALUES (?, ?, NULL, ?, ?)
         ON CONFLICT (user_id, episode_id) DO UPDATE SET
            notes = excluded.notes,
            updated_at = excluded.updated_at",
    )
    .bind(auth.user_id())
    .bind(ep_id)
    .bind(&body.notes)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// After any watch change, move the library entry to match what has been
/// watched against what has come out; `release::desired_status` has the rules.
async fn after_change(state: &AppState, user_id: i64, title_id: i64) -> AppResult<Json<Value>> {
    // Ticking an episode of a show not in the library puts it there.
    if store::get_entry(&state.pool, user_id, title_id)
        .await?
        .is_none()
    {
        let progress = store::progress(&state.pool, user_id, title_id).await?;
        if progress.watched > 0 {
            let patch = store::LibraryPatch {
                status: Some("watching".into()),
                ..Default::default()
            };
            store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
        }
    }
    crate::release::reconcile(&state.pool, user_id, title_id).await?;

    let row = catalog::get_title(&state.pool, title_id).await?;
    let item = store::build_item(&state.pool, user_id, row).await?;
    Ok(Json(json!({ "item": item })))
}
