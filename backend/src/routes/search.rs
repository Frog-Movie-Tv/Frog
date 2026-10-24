use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::models::SearchItem;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "first_page")]
    pub page: u32,
}

fn first_page() -> u32 {
    1
}

pub async fn search(
    _: Auth,
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let term = q.q.trim();
    if term.is_empty() {
        return Err(AppError::BadRequest(
            "give me something to search for".into(),
        ));
    }

    let raw = state.tmdb.search_multi(term, q.page.clamp(1, 500)).await?;
    let mut items = collect(&raw, None);
    annotate_library(&state, &mut items).await?;

    Ok(Json(serde_json::json!({
        "page": raw.get("page").and_then(Value::as_i64).unwrap_or(1),
        "total_pages": raw.get("total_pages").and_then(Value::as_i64).unwrap_or(1),
        "results": items,
    })))
}

pub async fn trending(_: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let raw = state.tmdb.trending().await?;
    let mut items = collect(&raw, None);
    annotate_library(&state, &mut items).await?;
    Ok(Json(serde_json::json!({ "results": items })))
}

fn collect(raw: &Value, force_kind: Option<&str>) -> Vec<SearchItem> {
    raw.get("results")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| SearchItem::from_value(v, force_kind))
                .collect()
        })
        .unwrap_or_default()
}

/// Mark results that are already tracked, so the grid can show their status
/// instead of offering to add them again.
pub async fn annotate_library(state: &AppState, items: &mut [SearchItem]) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }

    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, l.status
           FROM library_entry l JOIN title t ON t.id = l.title_id",
    )
    .fetch_all(&state.pool)
    .await?;

    for item in items.iter_mut() {
        item.in_library = rows
            .iter()
            .find(|(kind, id, _)| *kind == item.kind && *id == item.tmdb_id)
            .map(|(_, _, status)| status.clone());
    }
    Ok(())
}
