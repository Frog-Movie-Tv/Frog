//! Invite links. There is no open sign-up: an administrator makes a link, it
//! works once, and it stops working after its expiry.
//!
//! Only the SHA-256 of a code is stored, as with sessions and recovery codes,
//! so the database never holds a working invitation. The code itself is shown
//! exactly once, when it is made.

use axum::extract::{Path, State};
use axum::Json;
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth::{AdminAuth, Role};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// How long an invite lasts unless asked otherwise, and the most it may last.
const DEFAULT_DAYS: i64 = 7;
const MAX_DAYS: i64 = 90;

pub fn hash_code(code: &str) -> String {
    hex::encode(Sha256::digest(code.trim().as_bytes()))
}

fn new_code() -> String {
    let mut raw = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    hex::encode(raw)
}

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

    sqlx::query(
        "INSERT INTO invite (code_hash, role, created_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash_code(&code))
    .bind(role.as_db())
    .bind(auth.user_id())
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({
        "code": code,
        "path": format!("/join?code={code}"),
        "role": role,
        "expires_at": expires.to_rfc3339(),
    })))
}

/// (code hash, role, created by, created at, expires at, used at, used by)
type InviteRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
);

pub async fn list(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let rows: Vec<InviteRow> = sqlx::query_as(
        "SELECT i.code_hash, i.role, c.username, i.created_at, i.expires_at,
                i.used_at, u.username
           FROM invite i
           LEFT JOIN app_user c ON c.id = i.created_by
           LEFT JOIN app_user u ON u.id = i.used_by
          ORDER BY i.created_at DESC
          LIMIT 200",
    )
    .fetch_all(&state.pool)
    .await?;

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
    let gone = sqlx::query("DELETE FROM invite WHERE code_hash = ? AND used_at IS NULL")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if gone.rows_affected() == 0 {
        return Err(AppError::NotFound("no open invite with that id".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Spend an invite, returning the role it grants.
///
/// Marking it used is the same statement that checks it, so two people
/// redeeming the same link at the same moment cannot both get in. If the
/// account then fails to be created, `release` puts it back.
pub async fn claim(state: &AppState, code: &str) -> AppResult<Role> {
    let hash = hash_code(code);
    let now = Utc::now().to_rfc3339();

    let taken = sqlx::query(
        "UPDATE invite SET used_at = ?
          WHERE code_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .bind(&hash)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    if taken.rows_affected() != 1 {
        return Err(AppError::Forbidden(
            "That invite link has been used already, or it has expired.".into(),
        ));
    }

    let (role,): (String,) = sqlx::query_as("SELECT role FROM invite WHERE code_hash = ?")
        .bind(&hash)
        .fetch_one(&state.pool)
        .await?;
    Ok(Role::from_db(&role))
}

/// Put a claimed invite back, for when the account could not be created.
pub async fn release(state: &AppState, code: &str) {
    let _ = sqlx::query("UPDATE invite SET used_at = NULL WHERE code_hash = ? AND used_by IS NULL")
        .bind(hash_code(code))
        .execute(&state.pool)
        .await;
}

/// Record who used it, once the account exists.
pub async fn mark_used(state: &AppState, code: &str, user_id: i64) -> AppResult<()> {
    sqlx::query("UPDATE invite SET used_by = ? WHERE code_hash = ?")
        .bind(user_id)
        .bind(hash_code(code))
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_hashed_and_whitespace_does_not_change_it() {
        let h = hash_code("abc123");
        assert_eq!(h, hash_code("  abc123  "));
        assert_ne!(h, "abc123");
        assert_eq!(h.len(), 64);
    }

    #[test]
    fn codes_do_not_repeat() {
        assert_ne!(new_code(), new_code());
        assert_eq!(new_code().len(), 32);
    }
}
