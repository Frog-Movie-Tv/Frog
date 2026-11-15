//! Who a watch was with: a show's "watching with", a diary day's, and the
//! answers of the people named.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::together;

pub async fn get_default(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let ids = together::default_for(&state.pool, auth.user_id(), title_id).await?;
    Ok(Json(
        json!({ "with": together::names(&state.pool, &ids).await? }),
    ))
}

#[derive(Deserialize)]
pub struct WithBody {
    #[serde(default)]
    pub with: Vec<i64>,
}

pub async fn set_default(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
    Json(body): Json<WithBody>,
) -> AppResult<Json<Value>> {
    crate::catalog::get_title(&state.pool, title_id).await?;
    let ids = together::set_default(&state.pool, auth.user_id(), title_id, &body.with).await?;
    Ok(Json(
        json!({ "with": together::names(&state.pool, &ids).await? }),
    ))
}

#[derive(Deserialize)]
pub struct DayBody {
    pub title_id: i64,
    /// yyyy-mm-dd, as the diary groups it.
    pub day: String,
    pub film: bool,
    #[serde(default)]
    pub with: Vec<i64>,
}

pub async fn set_on_day(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<DayBody>,
) -> AppResult<Json<Value>> {
    if chrono::NaiveDate::parse_from_str(&body.day, "%Y-%m-%d").is_err() {
        return Err(AppError::BadRequest("pick a valid date".into()));
    }
    together::set_on_day(
        &state,
        auth.user_id(),
        body.title_id,
        &body.day,
        body.film,
        &body.with,
    )
    .await?;
    let ids = together::on_day(
        &state.pool,
        auth.user_id(),
        body.title_id,
        &body.day,
        body.film,
    )
    .await?;
    Ok(Json(
        json!({ "with": together::names(&state.pool, &ids).await? }),
    ))
}

#[derive(Deserialize)]
pub struct AnswerBody {
    /// `add` (to my diary) or `not_me`.
    pub action: String,
    /// The status a title not yet in the library comes in with.
    pub status: Option<String>,
}

pub async fn answer(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AnswerBody>,
) -> AppResult<Json<Value>> {
    match body.action.as_str() {
        "add" => {
            if let Some(s) = &body.status {
                if !crate::store::STATUSES.contains(&s.as_str()) {
                    return Err(AppError::BadRequest(format!("unknown status {s}")));
                }
            }
            let copied =
                together::add_to_mine(&state.pool, auth.user_id(), id, body.status.as_deref())
                    .await?;
            Ok(Json(json!({ "copied": copied })))
        }
        "not_me" => {
            together::not_me(&state.pool, auth.user_id(), id).await?;
            Ok(Json(json!({ "ok": true })))
        }
        other => Err(AppError::BadRequest(format!("unknown answer {other}"))),
    }
}
