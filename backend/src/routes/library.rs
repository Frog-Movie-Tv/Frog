use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::bulk::{self, Marking};
use crate::catalog;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{self, EntryRecord, LibraryFilter, LibraryPatch, Page};

/// Room to send a whole library back: ten thousand entries with long notes.
pub const BULK_MAX_BYTES: usize = 32 * 1024 * 1024;

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
    let filter = q.filter()?;

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

impl ListQuery {
    /// What the list narrows by, checked.
    pub fn filter(&self) -> AppResult<LibraryFilter> {
        if let Some(status) = &self.status {
            if !store::STATUSES.contains(&status.as_str()) {
                return Err(AppError::BadRequest(format!("unknown status {status}")));
            }
        }
        Ok(LibraryFilter {
            status: self.status.clone(),
            kind: self.kind.clone(),
            genre: self.genre.clone(),
            term: self.q.clone(),
            favourite: self.favourite == Some(true),
        })
    }
}

pub fn to_map(rows: Vec<(String, i64)>) -> Value {
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
        let now = chrono::Utc::now().to_rfc3339();
        store::add_first_film_watch(&state.pool, auth.user_id(), title_id, &now, false).await?;
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

/// One action on many titles, in one request.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BulkBody {
    Remove {
        title_ids: Vec<i64>,
    },
    /// What `remove`, `status` or `favourite` said a moment ago.
    Restore {
        entries: Vec<EntryRecord>,
    },
    Watched {
        title_ids: Vec<i64>,
        #[serde(flatten)]
        marking: Marking,
    },
    Status {
        title_ids: Vec<i64>,
        status: String,
        /// When a title moved to Completed was watched.
        #[serde(flatten)]
        marking: Marking,
    },
    Favourite {
        title_ids: Vec<i64>,
        favourite: bool,
    },
    Shelf {
        title_ids: Vec<i64>,
        shelf_id: i64,
    },
}

pub async fn bulk(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<BulkBody>,
) -> AppResult<Json<Value>> {
    let me = auth.user_id();
    let pool = &state.pool;
    Ok(Json(match body {
        BulkBody::Remove { title_ids } => {
            bulk::check_size(&title_ids)?;
            json!({ "before": bulk::remove(pool, me, &title_ids).await? })
        }
        BulkBody::Restore { entries } => {
            json!({ "restored": store::restore_entries(pool, me, &entries).await? })
        }
        BulkBody::Watched { title_ids, marking } => {
            bulk::check_size(&title_ids)?;
            json!(bulk::mark_watched(&state, me, &title_ids, &marking, false).await?)
        }
        BulkBody::Status {
            title_ids,
            status,
            marking,
        } => {
            bulk::check_size(&title_ids)?;
            if !store::STATUSES.contains(&status.as_str()) {
                return Err(AppError::BadRequest(format!("unknown status {status}")));
            }
            if status == "completed" {
                json!(bulk::mark_watched(&state, me, &title_ids, &marking, true).await?)
            } else {
                json!({ "before": bulk::set_status(pool, me, &title_ids, &status).await? })
            }
        }
        BulkBody::Favourite {
            title_ids,
            favourite,
        } => {
            bulk::check_size(&title_ids)?;
            json!({ "before": bulk::set_favourite(pool, me, &title_ids, favourite).await? })
        }
        BulkBody::Shelf {
            title_ids,
            shelf_id,
        } => {
            bulk::check_size(&title_ids)?;
            json!({ "added": bulk::add_to_shelf(pool, me, shelf_id, &title_ids).await? })
        }
    }))
}
