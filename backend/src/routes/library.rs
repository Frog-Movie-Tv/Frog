use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{self, LibraryItem, LibraryPatch, TitleRow};

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

/// Map a sort key to SQL. An allowlist, never interpolation of user input.
fn order_clause(sort: Option<&str>, order: Option<&str>) -> String {
    let dir = match order {
        Some("asc") => "ASC",
        _ => "DESC",
    };
    let column = match sort.unwrap_or("added") {
        "name" => "t.name COLLATE NOCASE",
        "rating" => "l.rating",
        "release" => "COALESCE(t.release_date, '')",
        "updated" => "l.updated_at",
        "last_watched" => "(SELECT MAX(w.watched_at) FROM watch_event w WHERE w.title_id = t.id)",
        _ => "l.added_at",
    };
    // NULLs last in either direction: an unrated title should never head the list.
    format!("ORDER BY ({column}) IS NULL, {column} {dir}, t.name COLLATE NOCASE ASC")
}

pub async fn list(
    _: Auth,
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let mut sql = String::from(
        "SELECT t.id, t.kind, t.tmdb_id, t.name, t.overview, t.poster_path, t.backdrop_path,
                t.logo_path, t.release_date, t.last_air_date, t.tmdb_status, t.runtime,
                t.episode_count, t.season_count, t.genres_json, t.tmdb_vote, t.imdb_id,
                t.collection_id, t.accent_json
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE 1 = 1",
    );

    if let Some(status) = &q.status {
        if !store::STATUSES.contains(&status.as_str()) {
            return Err(AppError::BadRequest(format!("unknown status {status}")));
        }
        sql.push_str(" AND l.status = ?");
    }
    if q.kind.is_some() {
        sql.push_str(" AND t.kind = ?");
    }
    if q.genre.is_some() {
        sql.push_str(" AND t.genres_json LIKE ?");
    }
    if q.q.is_some() {
        sql.push_str(" AND (t.name LIKE ? OR t.original_name LIKE ?)");
    }
    if q.favourite == Some(true) {
        sql.push_str(" AND l.favourite = 1");
    }

    sql.push(' ');
    sql.push_str(&order_clause(q.sort.as_deref(), q.order.as_deref()));
    sql.push_str(" LIMIT ? OFFSET ?");

    let mut query = sqlx::query_as::<_, TitleRow>(&sql);
    if let Some(status) = &q.status {
        query = query.bind(status.clone());
    }
    if let Some(kind) = &q.kind {
        query = query.bind(kind.clone());
    }
    if let Some(genre) = &q.genre {
        query = query.bind(format!("%\"{genre}\"%"));
    }
    if let Some(term) = &q.q {
        let like = format!("%{term}%");
        query = query.bind(like.clone()).bind(like);
    }
    query = query
        .bind(q.limit.unwrap_or(500).clamp(1, 2000))
        .bind(q.offset.unwrap_or(0).max(0));

    let rows = query.fetch_all(&state.pool).await?;

    let mut items: Vec<LibraryItem> = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(store::build_item(&state.pool, row).await?);
    }

    Ok(Json(json!({ "items": items })))
}

fn to_map(rows: Vec<(String, i64)>) -> Value {
    Value::Object(rows.into_iter().map(|(k, v)| (k, json!(v))).collect())
}

/// Counts for the library header and the first cut of the stats page.
pub async fn stats(_: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let by_status: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, COUNT(*) FROM library_entry GROUP BY status")
            .fetch_all(&state.pool)
            .await?;

    let by_kind: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.kind, COUNT(*) FROM library_entry l JOIN title t ON t.id = l.title_id
         GROUP BY t.kind",
    )
    .fetch_all(&state.pool)
    .await?;

    let (films_watched, episodes_watched): (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM watch_event WHERE episode_id IS NULL),
            (SELECT COUNT(*) FROM watch_event WHERE episode_id IS NOT NULL)",
    )
    .fetch_one(&state.pool)
    .await?;

    // Runtime is per-episode for shows, so this is a decent estimate rather
    // than a precise figure, and the UI says so.
    let (minutes,): (Option<f64>,) = sqlx::query_as(
        "SELECT SUM(COALESCE(e.runtime, t.runtime, 0))
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           LEFT JOIN episode e ON e.id = w.episode_id",
    )
    .fetch_one(&state.pool)
    .await?;

    let (mean_rating,): (Option<f64>,) =
        sqlx::query_as("SELECT AVG(rating) FROM library_entry WHERE rating IS NOT NULL")
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
    /// what makes progress, "up next" and the heatmap work straight away.
    #[serde(default = "yes")]
    pub sync_seasons: bool,
}

fn yes() -> bool {
    true
}

pub async fn upsert(
    _: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id)): Path<(String, i64)>,
    Json(body): Json<UpsertBody>,
) -> AppResult<Json<Value>> {
    if kind != "movie" && kind != "tv" {
        return Err(AppError::BadRequest("kind must be movie or tv".into()));
    }

    let (title_id, detail) = store::ensure_title(&state, &kind, tmdb_id).await?;
    let is_new = store::get_entry(&state.pool, title_id).await?.is_none();
    store::apply_patch(&state.pool, title_id, &body.patch).await?;

    if kind == "tv" && is_new && body.sync_seasons {
        if let Err(e) = store::sync_all_seasons(&state, title_id, &detail).await {
            tracing::warn!(error = %e, "could not sync every season");
        }
    }

    let row = store::get_title(&state.pool, title_id).await?;
    let item = store::build_item(&state.pool, row).await?;
    Ok(Json(json!({ "item": item })))
}

pub async fn remove(
    _: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id)): Path<(String, i64)>,
) -> AppResult<Json<Value>> {
    let Some(title_id) = store::find_title_id(&state.pool, &kind, tmdb_id).await? else {
        return Err(AppError::NotFound("not in your library".into()));
    };
    store::remove_entry(&state.pool, title_id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::order_clause;

    #[test]
    fn sort_keys_are_allowlisted() {
        assert!(order_clause(Some("name"), Some("asc")).contains("t.name"));
        assert!(order_clause(Some("rating"), None).contains("l.rating"));
        // Anything unknown falls back to added_at rather than reaching SQL.
        let injected = order_clause(Some("name; DROP TABLE title"), None);
        assert!(injected.contains("l.added_at"));
        assert!(!injected.contains("DROP"));
    }
}
