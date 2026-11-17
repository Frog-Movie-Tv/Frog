//! Events over HTTP. Reading, hiding and the premiere played are the
//! account's own, behind `Auth`; switching an event off and pinning its
//! moment sit behind `AdminAuth`. Events themselves are made in packs, never
//! here (`crate::events::pack`).

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{AdminAuth, Auth};
use crate::error::{AppError, AppResult};
use crate::events::{records, view};
use crate::state::AppState;

fn missing() -> AppError {
    AppError::NotFound("no such event".into())
}

/// Now, scheduled and past. Administrators also see events switched off.
pub async fn list(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    view::list(&state.pool, auth.user_id(), auth.is_admin())
        .await
        .map(Json)
}

/// One event's whole page. An administrator may open one switched off, to
/// preview it.
pub async fn page(
    auth: Auth,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let k = view::one(&state.pool, &slug, auth.is_admin())
        .await?
        .ok_or_else(missing)?;
    view::page(&state.pool, auth.user_id(), &k).await.map(Json)
}

async fn set_hidden(auth: &Auth, state: &AppState, slug: &str, hidden: bool) -> AppResult<()> {
    let k = view::one(&state.pool, slug, false)
        .await?
        .ok_or_else(missing)?;
    records::set_hidden(&state.pool, auth.user_id(), k.row.id, hidden).await
}

/// Off Home, and no reminders. The page stays.
pub async fn hide(
    auth: Auth,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    set_hidden(&auth, &state, &slug, true).await?;
    Ok(Json(json!({ "hidden": true })))
}

pub async fn show(
    auth: Auth,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    set_hidden(&auth, &state, &slug, false).await?;
    Ok(Json(json!({ "hidden": false })))
}

/// The premiere has started on this device. `first` is false when another
/// device of the account already played it.
pub async fn seen(
    auth: Auth,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let k = view::one(&state.pool, &slug, false)
        .await?
        .ok_or_else(missing)?;
    let first = records::mark_seen(&state.pool, auth.user_id(), k.row.id).await?;
    Ok(Json(json!({ "first": first })))
}

/// Every pack this release carries, with its switch and moment.
pub async fn admin_list(
    AdminAuth(auth): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    view::list(&state.pool, auth.user_id(), true)
        .await
        .map(Json)
}

#[derive(Deserialize)]
pub struct StateBody {
    pub on: bool,
}

pub async fn set_state(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<StateBody>,
) -> AppResult<Json<Value>> {
    let row = records::by_slug(&state.pool, &slug)
        .await?
        .ok_or_else(missing)?;
    records::set_state(&state.pool, row.id, body.on).await?;
    Ok(Json(json!({ "on": body.on })))
}

#[derive(Deserialize)]
pub struct PinBody {
    /// A moment (YYYY-MM-DDTHH:MM:SSZ), or none to follow the day again.
    pub at: Option<String>,
}

pub async fn set_pin(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<PinBody>,
) -> AppResult<Json<Value>> {
    let row = records::by_slug(&state.pool, &slug)
        .await?
        .ok_or_else(missing)?;
    let at = match body.at.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        None => None,
        Some(a) => Some(
            crate::awards::eligible::parse(a)
                .ok_or_else(|| AppError::BadRequest("That moment could not be read.".into()))?,
        ),
    };
    records::set_pinned(&state.pool, row.id, at).await?;
    Ok(Json(json!({ "pinned": at.is_some() })))
}
