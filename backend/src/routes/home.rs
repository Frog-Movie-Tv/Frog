//! Home over HTTP; `crate::home` builds it.

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::AppResult;
use crate::home;
use crate::state::AppState;

/// Everything the home screen draws, in one request.
pub async fn home(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    home::home(&state, auth.user_id()).await.map(Json)
}

/// "Because you watched X". Separate from `/home` because it may need TMDB,
/// and Home itself must stay instant.
pub async fn because(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    home::because(&state, auth.user_id()).await.map(Json)
}

/// What other people here have been watching. Separate from `/home`, so a row
/// that reads other accounts is only asked for by those who show it.
pub async fn activity(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let items = crate::activity::feed(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "items": items })))
}

#[derive(serde::Deserialize)]
pub struct EverythingQuery {
    before: Option<String>,
    limit: Option<usize>,
}

/// All of it, page by page, for the Activity page.
pub async fn all_activity(
    auth: Auth,
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<EverythingQuery>,
) -> AppResult<Json<Value>> {
    let limit = q.limit.unwrap_or(40).clamp(1, 100);
    let (items, next) =
        crate::activity::everything(&state.pool, auth.user_id(), q.before.as_deref(), limit)
            .await?;
    Ok(Json(json!({ "items": items, "next": next })))
}
