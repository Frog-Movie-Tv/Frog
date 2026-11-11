//! The calendar and the diary over HTTP; `crate::timeline` reads them.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::AppResult;
use crate::state::AppState;
use crate::timeline;

#[derive(Debug, Deserialize)]
pub struct Range {
    /// Inclusive yyyy-mm-dd. Defaults to a month either side of today.
    pub from: Option<String>,
    pub to: Option<String>,
}

/// Episodes of anything followed, plus release dates of films on the
/// watchlist, inside a window.
pub async fn calendar(
    auth: Auth,
    State(state): State<AppState>,
    Query(range): Query<Range>,
) -> AppResult<Json<Value>> {
    let from = normalise(range.from.as_deref().unwrap_or(""), -45);
    let to = normalise(range.to.as_deref().unwrap_or(""), 120);
    let entries = timeline::calendar(&state.pool, auth.user_id(), &from, &to).await?;
    Ok(Json(json!({ "from": from, "to": to, "entries": entries })))
}

/// Accept a plain date, or fall back to an offset from today.
fn normalise(value: &str, fallback_days: i64) -> String {
    let looks_like_date = value.len() == 10 && value.as_bytes()[4] == b'-';
    if looks_like_date {
        return value.to_string();
    }
    (chrono::Utc::now() + chrono::Duration::days(fallback_days))
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Debug, Deserialize)]
pub struct DiaryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Everything watched and read, newest first, one entry per title per day.
pub async fn diary(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<DiaryQuery>,
) -> AppResult<Json<Value>> {
    let limit = q.limit.unwrap_or(60).clamp(1, 300);
    let offset = q.offset.unwrap_or(0).max(0);
    let (entries, total, undated) =
        timeline::diary(&state.pool, auth.user_id(), limit, offset).await?;
    Ok(Json(
        json!({ "entries": entries, "total": total, "undated": undated }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct RedateBody {
    /// Frog's id for the title.
    pub title_id: i64,
    /// The diary day the group sits on now, yyyy-mm-dd.
    pub day: String,
    pub when: crate::bulk::When,
    pub date: Option<String>,
}

/// Move a day's group of watches to another date, onto each episode's
/// release date, or out of the diary altogether.
pub async fn redate(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<RedateBody>,
) -> AppResult<Json<Value>> {
    let changed = timeline::redate(
        &state.pool,
        auth.user_id(),
        body.title_id,
        &body.day,
        &body.when,
        body.date.as_deref(),
    )
    .await?;
    Ok(Json(json!({ "changed": changed })))
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn a_real_date_is_kept_and_anything_else_falls_back() {
        assert_eq!(normalise("2026-03-01", 10), "2026-03-01");
        assert_eq!(normalise("nonsense", 0).len(), 10);
        assert_ne!(normalise("", 30), normalise("", -30));
    }
}
