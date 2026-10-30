use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::catalog::{self, TitleRow, TITLE_COLUMNS};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{self, LibraryPatch};
use crate::text;

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
///
/// `user_id` is written into the sort subquery rather than bound, because a
/// placeholder inside ORDER BY would have to be bound between the filters and
/// the paging ones and silently shift if either changed. It comes from the
/// session as an `i64`, so there is nothing in it to escape.
pub fn order_clause(sort: Option<&str>, order: Option<&str>, user_id: i64) -> String {
    let dir = match order {
        Some("asc") => "ASC",
        _ => "DESC",
    };
    let column: String = match sort.unwrap_or("added") {
        "name" => "t.name COLLATE NOCASE".into(),
        "rating" => "l.rating".into(),
        "release" => "COALESCE(t.release_date, '')".into(),
        "updated" => "l.updated_at".into(),
        "last_watched" => format!(
            "(SELECT MAX(w.watched_at) FROM watch_event w
                WHERE w.title_id = t.id AND w.user_id = {user_id})"
        ),
        _ => "l.added_at".into(),
    };
    // NULLs last in either direction: an unrated title should never head the list.
    format!("ORDER BY ({column}) IS NULL, {column} {dir}, t.name COLLATE NOCASE ASC")
}

pub async fn list(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let mut sql = format!(
        "SELECT {TITLE_COLUMNS}
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?"
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
        // Folded on both sides, so neither case nor accents decide a match.
        // A row written by an older binary has no search_name until the next
        // start fills it; until then its plain name still matches.
        sql.push_str(
            " AND COALESCE(t.search_name, lower(t.name || char(10) || COALESCE(t.original_name, '')))
                  LIKE ? ESCAPE '\\'",
        );
    }
    if q.favourite == Some(true) {
        sql.push_str(" AND l.favourite = 1");
    }

    sql.push(' ');
    sql.push_str(&order_clause(
        q.sort.as_deref(),
        q.order.as_deref(),
        auth.user_id(),
    ));
    sql.push_str(" LIMIT ? OFFSET ?");

    let mut query = sqlx::query_as::<_, TitleRow>(&sql).bind(auth.user_id());
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
        // Typed literally: a % or _ in a title is a character, not a wildcard.
        let folded = text::fold(term.trim())
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        query = query.bind(format!("%{folded}%"));
    }
    // The whole library unless asked for less: a Netflix import alone brings
    // six hundred titles, and the page has no "load more".
    query = query
        .bind(q.limit.unwrap_or(10_000).clamp(1, 10_000))
        .bind(q.offset.unwrap_or(0).max(0));

    let rows = query.fetch_all(&state.pool).await?;
    let items = store::build_items(&state.pool, auth.user_id(), rows).await?;

    Ok(Json(json!({ "items": items })))
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
    /// what makes progress, "up next" and the heatmap work straight away.
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
    crate::anilist::touch(&state.pool, auth.user_id()).await;

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

#[cfg(test)]
mod tests {
    use super::order_clause;

    #[test]
    fn sort_keys_are_allowlisted() {
        assert!(order_clause(Some("name"), Some("asc"), 1).contains("t.name"));
        assert!(order_clause(Some("rating"), None, 1).contains("l.rating"));
        // Anything unknown falls back to added_at rather than reaching SQL.
        let injected = order_clause(Some("name; DROP TABLE title"), None, 1);
        assert!(injected.contains("l.added_at"));
        assert!(!injected.contains("DROP"));
    }

    #[test]
    fn sorting_by_last_watched_only_counts_my_watches() {
        let clause = order_clause(Some("last_watched"), None, 7);
        assert!(clause.contains("w.user_id = 7"));
    }
}
