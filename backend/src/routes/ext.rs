//! `/api/ext`: what the browser extension calls, with a personal token.
//!
//! Every handler here takes `TokenAuth`, and nothing outside this module
//! does, so a token reaches these four and no other route.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::catalog;
use crate::error::AppResult;
use crate::extension::{self, LogRequest, Signals};
use crate::state::AppState;
use crate::store;
use crate::tokens::TokenAuth;

/// Who the token belongs to, and what is next in each show being watched.
pub async fn me(auth: TokenAuth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let watching = extension::watching(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({
        "username": auth.identity.username,
        "watching": watching,
    })))
}

pub async fn resolve(
    auth: TokenAuth,
    State(state): State<AppState>,
    Json(signals): Json<Signals>,
) -> AppResult<Json<Value>> {
    let found = extension::resolve(&state, auth.user_id(), signals).await?;
    Ok(Json(json!(found)))
}

pub async fn log(
    auth: TokenAuth,
    State(state): State<AppState>,
    Json(body): Json<LogRequest>,
) -> AppResult<Json<Value>> {
    let logged = extension::log(&state, auth.user_id(), auth.token_id, body).await?;
    let title = catalog::get_title(&state.pool, logged.title_id).await?;
    let name = title.name.clone();
    let item = store::build_item(&state.pool, auth.user_id(), title).await?;
    Ok(Json(json!({
        "log_id": logged.log_id,
        "already": logged.already,
        "title_id": logged.title_id,
        "name": name,
        "season": logged.season,
        "episode": logged.episode,
        "status": item.entry.as_ref().map(|e| e.status.clone()),
    })))
}

#[derive(Deserialize)]
pub struct UndoBody {
    pub log_id: i64,
}

pub async fn undo(
    auth: TokenAuth,
    State(state): State<AppState>,
    Json(body): Json<UndoBody>,
) -> AppResult<Json<Value>> {
    let title_id = extension::undo(&state, auth.user_id(), body.log_id).await?;
    Ok(Json(json!({ "ok": true, "title_id": title_id })))
}
