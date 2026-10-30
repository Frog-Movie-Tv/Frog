//! Import and export over HTTP. The decisions live in `crate::transfer`;
//! this module checks the batch size, names the account and shapes the
//! answer.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::transfer::{self, ApplyItem, Conflict, Incoming, MAX_BATCH};

#[derive(Debug, Deserialize)]
pub struct MatchRequest {
    pub items: Vec<Incoming>,
}

pub async fn match_titles(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<MatchRequest>,
) -> AppResult<Json<Value>> {
    if body.items.len() > MAX_BATCH {
        return Err(AppError::BadRequest(format!(
            "send at most {MAX_BATCH} rows at a time"
        )));
    }
    let results = transfer::match_rows(&state, auth.user_id(), body.items).await?;
    Ok(Json(json!({ "results": results })))
}

#[derive(Debug, Deserialize)]
pub struct ApplyRequest {
    pub items: Vec<ApplyItem>,
    /// What to do about titles already in the library.
    #[serde(default)]
    pub on_conflict: Conflict,
}

pub async fn apply(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<ApplyRequest>,
) -> AppResult<Json<Value>> {
    if body.items.len() > MAX_BATCH {
        return Err(AppError::BadRequest(format!(
            "import at most {MAX_BATCH} rows at a time"
        )));
    }
    let applied =
        transfer::apply_items(&state, auth.user_id(), body.items, body.on_conflict).await?;
    Ok(Json(json!(applied)))
}

/// A complete backup: everything needed to rebuild the library elsewhere.
pub async fn export_json(
    auth: Auth,
    State(state): State<AppState>,
) -> AppResult<impl IntoResponse> {
    let body = transfer::library_json(&state, auth.user_id()).await?;
    Ok(download(
        "frog-library.json",
        "application/json; charset=utf-8",
        serde_json::to_string_pretty(&body)?,
    ))
}

/// A flat sheet, for anything that reads CSV.
pub async fn export_csv(auth: Auth, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let body = transfer::library_csv(&state, auth.user_id()).await?;
    Ok(download(
        "frog-library.csv",
        "text/csv; charset=utf-8",
        body,
    ))
}

fn download(filename: &str, content_type: &'static str, body: String) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    (headers, body)
}
