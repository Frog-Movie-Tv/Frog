//! Settings → Connections → Browser extension: making, listing and revoking
//! personal tokens. These go by the session, never by a token, so a token
//! cannot make another.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tokens;

pub async fn list(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let tokens = tokens::list(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "tokens": tokens })))
}

#[derive(Deserialize)]
pub struct NewToken {
    pub name: String,
}

/// The token itself is in this answer and nowhere else, ever.
pub async fn create(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<NewToken>,
) -> AppResult<Json<Value>> {
    let (token, raw) = tokens::create(&state.pool, auth.user_id(), &body.name).await?;
    Ok(Json(json!({ "token": token, "secret": raw })))
}

pub async fn revoke(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    if !tokens::revoke(&state.pool, auth.user_id(), id).await? {
        return Err(AppError::NotFound("no such token".into()));
    }
    Ok(Json(json!({ "ok": true })))
}
