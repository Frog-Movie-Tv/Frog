//! Invite links. There is no open sign-up: an administrator makes a link, it
//! works once, and it stops working after its expiry.
//!
//! Only the SHA-256 of a code is stored, as with sessions and recovery codes,
//! so the database never holds a working invitation. The code itself is shown
//! exactly once, when it is made.

use axum::extract::{Path, State};
use axum::Json;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{new_code, AdminAuth, Role};
use crate::error::{AppError, AppResult};
use crate::invites;
use crate::state::AppState;

/// How long an invite lasts unless asked otherwise, and the most it may last.
const DEFAULT_DAYS: i64 = 7;
const MAX_DAYS: i64 = 90;

#[derive(Deserialize)]
pub struct NewInvite {
    /// admin or user. Nobody can invite another owner.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub days: Option<i64>,
}

/// Make a link. The code comes back once and is never retrievable again.
pub async fn create(
    AdminAuth(auth): AdminAuth,
    State(state): State<AppState>,
    Json(body): Json<NewInvite>,
) -> AppResult<Json<Value>> {
    let role = match body.role.as_deref().unwrap_or("user") {
        "user" => Role::User,
        "admin" => Role::Admin,
        _ => {
            return Err(AppError::BadRequest(
                "an invite can be for a user or an admin".into(),
            ))
        }
    };
    // Only an owner hands out administrator accounts.
    if role == Role::Admin && auth.role() < Role::Owner {
        return Err(AppError::Forbidden(
            "Only the owner can invite an administrator.".into(),
        ));
    }

    let days = body.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
    let code = new_code();
    let now = Utc::now();
    let expires = now + Duration::days(days);

    invites::create(
        &state.pool,
        &code,
        role,
        auth.user_id(),
        &now.to_rfc3339(),
        &expires.to_rfc3339(),
    )
    .await?;

    Ok(Json(json!({
        "code": code,
        "path": format!("/join?code={code}"),
        "role": role,
        "expires_at": expires.to_rfc3339(),
    })))
}

pub async fn list(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let rows = invites::list(&state.pool).await?;

    let now = Utc::now().to_rfc3339();
    let invites: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, role, created_by, created_at, expires_at, used_at, used_by)| {
                let state = if used_at.is_some() {
                    "used"
                } else if expires_at <= now {
                    "expired"
                } else {
                    "open"
                };
                json!({
                    "id": id,
                    "role": role,
                    "created_by": created_by,
                    "created_at": created_at,
                    "expires_at": expires_at,
                    "used_at": used_at,
                    "used_by": used_by,
                    "state": state,
                })
            },
        )
        .collect();

    Ok(Json(json!({ "invites": invites })))
}

/// Withdraw a link that has not been used yet.
pub async fn revoke(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    if !invites::revoke(&state.pool, &id).await? {
        return Err(AppError::NotFound("no open invite with that id".into()));
    }
    Ok(Json(json!({ "ok": true })))
}
