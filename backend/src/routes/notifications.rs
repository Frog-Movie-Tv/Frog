//! The Notifications page, its bell, recommending a title, and the switches
//! for what each account wants to hear about.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::AppResult;
use crate::inbox::{self, Wants};
use crate::recommend;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct FeedQuery {
    /// An id: the page of notifications older than it.
    pub before: Option<i64>,
}

/// One page of everything, the recommendations to you, and the count.
pub async fn feed(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<FeedQuery>,
) -> AppResult<Json<Value>> {
    let pool = &state.pool;
    let me = auth.user_id();
    let items = inbox::list(pool, me, q.before, 50).await?;
    let next = (items.len() == 50)
        .then(|| items.last().and_then(|i| i["id"].as_i64()))
        .flatten();
    // Only the first page carries the recommendations; later pages are more of Everything.
    let for_you = if q.before.is_none() {
        Some(recommend::for_me(pool, me).await?)
    } else {
        None
    };
    Ok(Json(json!({
        "items": items,
        "next": next,
        "for_you": for_you,
        "unread": inbox::unread(pool, me).await?,
    })))
}

/// The bell's number, asked on load, on focus and now and then.
pub async fn unread(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({ "unread": inbox::unread(&state.pool, auth.user_id()).await? }),
    ))
}

#[derive(Deserialize)]
pub struct ReadBody {
    /// None or empty: every one.
    #[serde(default)]
    pub ids: Vec<i64>,
}

pub async fn read(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<ReadBody>,
) -> AppResult<Json<Value>> {
    inbox::mark_read(&state.pool, auth.user_id(), &body.ids).await?;
    Ok(Json(
        json!({ "unread": inbox::unread(&state.pool, auth.user_id()).await? }),
    ))
}

/// Off the page: one notification.
pub async fn remove(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let ids = inbox::remove(&state.pool, auth.user_id(), Some(id)).await?;
    Ok(Json(json!({
        "ids": ids,
        "unread": inbox::unread(&state.pool, auth.user_id()).await?,
    })))
}

/// Off the page: every notification, Clear all.
pub async fn clear(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let ids = inbox::remove(&state.pool, auth.user_id(), None).await?;
    Ok(Json(json!({
        "ids": ids,
        "unread": inbox::unread(&state.pool, auth.user_id()).await?,
    })))
}

#[derive(Deserialize)]
pub struct RestoreBody {
    pub ids: Vec<i64>,
}

/// Undo of a removal.
pub async fn restore(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<RestoreBody>,
) -> AppResult<Json<Value>> {
    inbox::restore(&state.pool, auth.user_id(), &body.ids).await?;
    Ok(Json(
        json!({ "unread": inbox::unread(&state.pool, auth.user_id()).await? }),
    ))
}

pub async fn get_wants(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(json!(
        inbox::wants(&state.pool, auth.user_id()).await?
    )))
}

pub async fn put_wants(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<Wants>,
) -> AppResult<Json<Value>> {
    inbox::set_wants(&state.pool, auth.user_id(), &body).await?;
    Ok(Json(json!(body)))
}

#[derive(Deserialize)]
pub struct PeopleQuery {
    /// Marks who already has this title from you.
    pub title_id: Option<i64>,
    /// `recommend` (the default) or `together`.
    pub r#for: Option<String>,
}

/// Who a people picker may offer this viewer.
pub async fn people(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<PeopleQuery>,
) -> AppResult<Json<Value>> {
    let kind = match q.r#for.as_deref() {
        Some("together") => inbox::TOGETHER,
        _ => inbox::RECOMMENDATION,
    };
    let people = recommend::people(&state.pool, auth.user_id(), q.title_id, kind).await?;
    Ok(Json(json!({ "people": people })))
}

#[derive(Deserialize)]
pub struct RecommendBody {
    pub to: Vec<i64>,
    pub note: Option<String>,
}

pub async fn recommend(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
    Json(body): Json<RecommendBody>,
) -> AppResult<Json<Value>> {
    let told = recommend::send(
        &state,
        auth.user_id(),
        title_id,
        &body.to,
        body.note.as_deref(),
    )
    .await?;
    Ok(Json(json!({ "told": told })))
}

#[derive(Deserialize)]
pub struct AnswerBody {
    /// `added`, `dismissed`, or null to take the answer back.
    pub answer: Option<String>,
}

pub async fn answer(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AnswerBody>,
) -> AppResult<Json<Value>> {
    recommend::answer(&state.pool, auth.user_id(), id, body.answer.as_deref()).await?;
    Ok(Json(json!({ "ok": true })))
}
