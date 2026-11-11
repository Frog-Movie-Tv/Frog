//! Shelves over HTTP: hand-picked lists, watchlists and smart shelves.
//! What a shelf holds and every statement on it is `crate::shelves`.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::catalog;
use crate::error::{AppError, AppResult};
use crate::shelves::{self, kind_of, rules_of, summary, titles_of, Rules};
use crate::state::AppState;
use crate::store;

pub async fn list(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let shelves = shelves::summaries(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "shelves": shelves })))
}

pub async fn get(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let row = shelves::find(&state.pool, auth.user_id(), id).await?;
    let titles = titles_of(&state.pool, auth.user_id(), &row).await?;
    let shelf = summary(&row, &titles);
    let items = store::build_items(&state.pool, auth.user_id(), titles).await?;
    Ok(Json(json!({ "shelf": shelf, "items": items })))
}

fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::BadRequest(
            "a shelf name needs 1 to 60 characters".into(),
        ));
    }
    Ok(name.to_string())
}

#[derive(Deserialize)]
pub struct CreateBody {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub rules: Rules,
    /// Local title ids to start a manual shelf with, in order. Used to bring
    /// a deleted shelf back.
    #[serde(default)]
    pub title_ids: Vec<i64>,
}

pub async fn create(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> AppResult<Json<Value>> {
    let name = clean_name(&body.name)?;
    if !["manual", "smart", "watchlist"].contains(&body.kind.as_str()) {
        return Err(AppError::BadRequest(
            "kind must be manual, watchlist or smart".into(),
        ));
    }
    body.rules.validate()?;
    let watchlist = body.kind == "watchlist";
    let kind = if watchlist {
        "manual"
    } else {
        body.kind.as_str()
    };
    let id = shelves::create(
        &state.pool,
        auth.user_id(),
        &name,
        kind,
        &body.rules,
        watchlist,
        &body.title_ids,
    )
    .await?;

    let row = shelves::find(&state.pool, auth.user_id(), id).await?;
    let titles = titles_of(&state.pool, auth.user_id(), &row).await?;
    Ok(Json(json!({ "shelf": summary(&row, &titles) })))
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub name: Option<String>,
    pub rules: Option<Rules>,
}

pub async fn update(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> AppResult<Json<Value>> {
    let row = shelves::find(&state.pool, auth.user_id(), id).await?;
    if let Some(name) = &body.name {
        shelves::rename(&state.pool, auth.user_id(), id, &clean_name(name)?).await?;
    }
    if let Some(rules) = &body.rules {
        if row.2 != "smart" {
            return Err(AppError::BadRequest("only smart shelves have rules".into()));
        }
        rules.validate()?;
        shelves::set_rules(&state.pool, auth.user_id(), id, rules).await?;
    }
    let row = shelves::find(&state.pool, auth.user_id(), id).await?;
    let titles = titles_of(&state.pool, auth.user_id(), &row).await?;
    Ok(Json(json!({ "shelf": summary(&row, &titles) })))
}

pub async fn remove(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let row = shelves::find(&state.pool, auth.user_id(), id).await?;
    // Hand back what undo needs to put it back exactly.
    let title_ids = shelves::delete(&state.pool, auth.user_id(), id).await?;
    Ok(Json(json!({
        "name": row.1,
        "kind": kind_of(&row),
        "rules": rules_of(&row.3),
        "title_ids": title_ids,
    })))
}

#[derive(Deserialize)]
pub struct OrderBody {
    pub ids: Vec<i64>,
}

pub async fn reorder(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<OrderBody>,
) -> AppResult<Json<Value>> {
    shelves::reorder(&state.pool, auth.user_id(), &body.ids).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ItemBody {
    /// Frog's id for the title.
    pub title_id: i64,
}

pub async fn add_item(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ItemBody>,
) -> AppResult<Json<Value>> {
    shelves::hand_picked(&state.pool, auth.user_id(), id).await?;
    let title_id = catalog::get_title(&state.pool, body.title_id).await?.id;
    shelves::add_item(&state.pool, id, title_id).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_item(
    auth: Auth,
    State(state): State<AppState>,
    Path((id, title_id)): Path<(i64, i64)>,
) -> AppResult<Json<Value>> {
    shelves::hand_picked(&state.pool, auth.user_id(), id).await?;
    shelves::remove_item(&state.pool, id, title_id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ItemOrderBody {
    /// Local title ids, in the new order.
    pub title_ids: Vec<i64>,
}

pub async fn reorder_items(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ItemOrderBody>,
) -> AppResult<Json<Value>> {
    shelves::hand_picked(&state.pool, auth.user_id(), id).await?;
    shelves::reorder_items(&state.pool, id, &body.title_ids).await?;
    Ok(Json(json!({ "ok": true })))
}

/// The hand-picked shelves and watchlists, and whether this title is on each:
/// the title page's "Add to shelf" menu.
pub async fn for_title(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let rows = shelves::holding(&state.pool, auth.user_id(), title_id).await?;
    Ok(Json(json!({
        "shelves": rows.into_iter().map(|(id, name, on)| json!({ "id": id, "name": name, "on": on })).collect::<Vec<_>>()
    })))
}
