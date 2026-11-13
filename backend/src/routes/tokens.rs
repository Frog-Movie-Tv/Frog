//! Settings → Connections → Browser extension: making, listing and revoking
//! personal tokens. These go by the session, never by a token, so a token
//! cannot make another.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::addon;
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

/// A link that installs the Firefox build from this Frog, good for an hour.
pub async fn addon_link(_auth: Auth, State(state): State<AppState>) -> Json<Value> {
    let key = state.addon.issue();
    Json(json!({ "url": format!("/extension/frog.xpi?key={key}") }))
}

#[derive(Deserialize)]
pub struct AddonKey {
    #[serde(default)]
    pub key: String,
}

/// The Firefox build, as Firefox installs from a link; or, when Frog has none
/// to give, the release page's own copy.
pub async fn addon_file(State(state): State<AppState>, Query(q): Query<AddonKey>) -> Response {
    if !state.addon.admits(&q.key) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let releases = &state.config.extension_releases;
    match state
        .addon
        .file(&state.http, releases, env!("CARGO_PKG_VERSION"))
        .await
    {
        Some(bytes) => (
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(addon::CONTENT_TYPE),
                ),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-store"),
                ),
            ],
            bytes,
        )
            .into_response(),
        None => Redirect::to(&addon::latest(releases)).into_response(),
    }
}
