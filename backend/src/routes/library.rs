use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::catalog;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{self, LibraryFilter, LibraryPatch, Page};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub kind: Option<String>,
    pub genre: Option<String>,
    pub q: Option<String>,
    pub favourite: Option<bool>,
    /// added | updated | name | rating | release | last_watched
    pub sort: Option<String>,
    /// asc | desc
    pub order: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    if let Some(status) = &q.status {
        if !store::STATUSES.contains(&status.as_str()) {
            return Err(AppError::BadRequest(format!("unknown status {status}")));
        }
    }
    let filter = LibraryFilter {
        status: q.status,
        kind: q.kind,
        genre: q.genre,
        term: q.q,
        favourite: q.favourite == Some(true),
    };

    // The whole library unless asked for less: a Netflix import alone brings
    // six hundred titles, and the page has no "load more".
    let page = Page {
        sort: q.sort.as_deref(),
        order: q.order.as_deref(),
        limit: q.limit.unwrap_or(10_000).clamp(1, 10_000),
        offset: q.offset.unwrap_or(0).max(0),
    };

    let rows = store::library_titles(&state.pool, auth.user_id(), &filter, &page).await?;
    let items = store::build_items(&state.pool, auth.user_id(), rows).await?;
    let counts = store::library_counts(&state.pool, auth.user_id(), &filter).await?;
    let genres = store::library_genres(&state.pool, auth.user_id()).await?;

    Ok(Json(json!({
        "items": items,
        "counts": to_map(counts),
        "genres": genres,
    })))
}

fn to_map(rows: Vec<(String, i64)>) -> Value {
    Value::Object(rows.into_iter().map(|(k, v)| (k, json!(v))).collect())
}

/// Counts for the library header and the first cut of the stats page.
pub async fn stats(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let by_status: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM library_entry WHERE user_id = ? GROUP BY status",
    )
    .bind(auth.user_id())
    .fetch_all(&state.pool)
    .await?;

    let by_kind: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.kind, COUNT(*) FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?
         GROUP BY t.kind",
    )
    .bind(auth.user_id())
    .fetch_all(&state.pool)
    .await?;

    // Watches of titles still in the library, as on the stats page.
    let (films_watched, episodes_watched): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(w.episode_id IS NULL), 0), COALESCE(SUM(w.episode_id IS NOT NULL), 0)
           FROM watch_event w
           JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
          WHERE w.user_id = ?",
    )
    .bind(auth.user_id())
    .fetch_one(&state.pool)
    .await?;

    // Runtime is per-episode for shows, so this is a decent estimate rather
    // than a precise figure, and the UI says so.
    // SQLite types SUM() by its operands, so summing INTEGER runtimes yields
    // an INTEGER and decoding it as f64 fails. CAST pins the type.
    let (minutes,): (Option<f64>,) = sqlx::query_as(
        "SELECT CAST(SUM(COALESCE(e.runtime, t.runtime, 0)) AS REAL)
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ?",
    )
    .bind(auth.user_id())
    .fetch_one(&state.pool)
    .await?;

    let (mean_rating,): (Option<f64>,) = sqlx::query_as(
        "SELECT AVG(rating) FROM library_entry WHERE user_id = ? AND rating IS NOT NULL",
    )
    .bind(auth.user_id())
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(json!({
        "by_status": to_map(by_status),
        "by_kind": to_map(by_kind),
        "films_watched": films_watched,
        "episodes_watched": episodes_watched,
        "minutes_watched": minutes.unwrap_or(0.0).round() as i64,
        "mean_rating": mean_rating,
    })))
}

#[derive(Deserialize)]
pub struct UpsertBody {
    #[serde(flatten)]
    pub patch: LibraryPatch,
    /// Pull every season into the local database. Default on for shows: it is
    /// what makes progress, and "up next" work straight away.
    #[serde(default = "yes")]
    pub sync_seasons: bool,
}

fn yes() -> bool {
    true
}

pub async fn upsert(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
    Json(body): Json<UpsertBody>,
) -> AppResult<Json<Value>> {
    let kind = catalog::get_title(&state.pool, title_id).await?.kind;
    let before = store::get_entry(&state.pool, auth.user_id(), title_id).await?;
    let is_new = before.is_none();
    store::apply_patch(&state.pool, auth.user_id(), title_id, &body.patch).await?;

    // Finishing a film is watching it: without a watch it would never reach
    // the diary or the stats. A film already watched keeps its history.
    let finished_now = body.patch.status.as_deref() == Some("completed")
        && before.as_ref().map(|e| e.status.as_str()) != Some("completed");
    if kind == "movie" && finished_now {
        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch, source)
             SELECT ?, ?, NULL, ?, 0, 'manual'
              WHERE NOT EXISTS (
                    SELECT 1 FROM watch_event WHERE user_id = ? AND title_id = ?)",
        )
        .bind(auth.user_id())
        .bind(title_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(auth.user_id())
        .bind(title_id)
        .execute(&state.pool)
        .await?;
    }

    if kind == "tv" && is_new && body.sync_seasons {
        if let Err(e) = catalog::ensure_episodes(&state, title_id, None).await {
            tracing::warn!(error = %e, "could not sync every season");
        }
    }

    let row = catalog::get_title(&state.pool, title_id).await?;
    let item = store::build_item(&state.pool, auth.user_id(), row).await?;
    Ok(Json(json!({ "item": item })))
}

pub async fn remove(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
) -> AppResult<Json<Value>> {
    if store::get_entry(&state.pool, auth.user_id(), title_id)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound("not in your library".into()));
    }
    store::remove_entry(&state.pool, auth.user_id(), title_id).await?;
    Ok(Json(json!({ "ok": true })))
}
