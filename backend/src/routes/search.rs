use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::models::SearchItem;

/// TMDB's genre id for Animation, shared by films and shows.
const ANIMATION_GENRE: i64 = 16;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "first_page")]
    pub page: u32,
    /// Leave animation out of the results.
    #[serde(default)]
    pub hide_animation: bool,
}

fn first_page() -> u32 {
    1
}

pub async fn search(
    auth: Auth,
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
    rank(&mut items, term, q.hide_animation);
    annotate_library(&state, auth.user_id(), &mut items).await?;

    Ok(Json(serde_json::json!({
        "page": raw.get("page").and_then(Value::as_i64).unwrap_or(1),
        "total_pages": raw.get("total_pages").and_then(Value::as_i64).unwrap_or(1),
        "results": items,
    })))
}

pub async fn trending(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let raw = state.tmdb.trending().await?;
    let mut items = collect(&raw, None);
    annotate_library(&state, auth.user_id(), &mut items).await?;
    Ok(Json(serde_json::json!({ "results": items })))
}

/// Put the thing you almost certainly meant at the top.
///
/// TMDB's own relevance ordering surfaces a lot of one-minute shorts and
/// unreleased fan projects that happen to share a word with the query. An
/// exact title match wins outright; after that it is popularity, damped by how
/// few people have rated it so obscure entries sink.
fn rank(items: &mut Vec<SearchItem>, term: &str, hide_animation: bool) {
    if hide_animation {
        items.retain(|i| !i.genre_ids.contains(&ANIMATION_GENRE));
    }

    let needle = term.trim().to_lowercase();

    items.sort_by(|a, b| score(b, &needle).total_cmp(&score(a, &needle)));
}

fn score(item: &SearchItem, needle: &str) -> f64 {
    let name = item.name.to_lowercase();
    let exact = if name == needle { 1000.0 } else { 0.0 };
    let starts = if name.starts_with(needle) { 250.0 } else { 0.0 };

    let votes = item.vote_count.unwrap_or(0) as f64;
    // Fifty ratings is roughly where a title stops being a curiosity.
    let confidence = (votes / (votes + 50.0)).clamp(0.05, 1.0);
    let popularity = item.popularity.unwrap_or(0.0).min(500.0);

    exact + starts + popularity * confidence
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
pub async fn annotate_library(
    state: &AppState,
    user_id: i64,
    items: &mut [SearchItem],
) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }

    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, l.status
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?",
    )
    .bind(user_id)
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
