//! Running the instance: who has an account, what they may do, and how the
//! instance itself is doing. Everything here sits behind `AdminAuth`.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, AdminAuth, Role};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// (id, username, role, disabled, created_at, titles tracked, last sign-in)
type UserRow = (i64, String, String, bool, String, i64, Option<String>);

pub async fn users(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let rows: Vec<UserRow> = sqlx::query_as(
        "SELECT u.id, u.username, u.role, u.disabled, u.created_at,
                (SELECT COUNT(*) FROM library_entry l WHERE l.user_id = u.id),
                (SELECT MAX(s.created_at) FROM session s WHERE s.user_id = u.id)
           FROM app_user u
          ORDER BY CASE u.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
                   u.username COLLATE NOCASE",
    )
    .fetch_all(&state.pool)
    .await?;
    let users: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, username, role, disabled, created_at, titles, last_sign_in)| {
                json!({
                    "id": id,
                    "username": username,
                    "role": role,
                    "disabled": disabled,
                    "created_at": created_at,
                    "titles": titles,
                    "last_sign_in": last_sign_in,
                })
            },
        )
        .collect();
    Ok(Json(json!({ "users": users })))
}

#[derive(Deserialize)]
pub struct UserPatch {
    /// admin | user. Making an owner is not something an API call does.
    pub role: Option<String>,
    pub disabled: Option<bool>,
}

/// Change someone's role or switch their account off.
///
/// Nobody changes themselves here (that is how an instance ends up with no
/// administrator), the owner cannot be touched, only the owner makes or
/// unmakes administrators, and an administrator manages ordinary accounts.
pub async fn update_user(
    AdminAuth(me): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UserPatch>,
) -> AppResult<Json<Value>> {
    if id == me.user_id() {
        return Err(AppError::BadRequest(
            "Change your own account from Settings.".into(),
        ));
    }
    let target: Option<(String,)> = sqlx::query_as("SELECT role FROM app_user WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    let Some((target_role,)) = target else {
        return Err(AppError::NotFound("no such account".into()));
    };
    let target_role = Role::from_db(&target_role);
    if target_role == Role::Owner {
        return Err(AppError::Forbidden(
            "The owner's account cannot be changed.".into(),
        ));
    }
    if target_role == Role::Admin && me.role() < Role::Owner {
        return Err(AppError::Forbidden(
            "Only the owner can change an administrator.".into(),
        ));
    }

    if let Some(role) = &patch.role {
        let role = match role.as_str() {
            "admin" => Role::Admin,
            "user" => Role::User,
            _ => return Err(AppError::BadRequest("role must be admin or user".into())),
        };
        if role != target_role && me.role() < Role::Owner {
            return Err(AppError::Forbidden(
                "Only the owner can make or remove administrators.".into(),
            ));
        }
        sqlx::query("UPDATE app_user SET role = ? WHERE id = ?")
            .bind(role.as_db())
            .bind(id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(disabled) = patch.disabled {
        sqlx::query("UPDATE app_user SET disabled = ? WHERE id = ?")
            .bind(disabled)
            .bind(id)
            .execute(&state.pool)
            .await?;
        // Sign-in checks the flag on every request anyway; dropping the sessions
        // as well means the devices list never shows a session that cannot work.
        if disabled {
            auth::destroy_sessions_for(&state.pool, id, None).await?;
        }
    }

    let (role, disabled): (String, bool) =
        sqlx::query_as("SELECT role, disabled FROM app_user WHERE id = ?")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(
        json!({ "id": id, "role": role, "disabled": disabled }),
    ))
}

/// How the instance is doing: version, space used, and the last backup the
/// server's backup script recorded next to the database.
pub async fn instance(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let (pages, page_size): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT page_count FROM pragma_page_count()), (SELECT page_size FROM pragma_page_size())",
    )
    .fetch_one(&state.pool)
    .await?;
    let (images,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(length(bytes)), 0) FROM image_cache")
            .fetch_one(&state.pool)
            .await?;
    let (accounts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM app_user")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "database_bytes": pages * page_size,
        "image_cache_bytes": images,
        "accounts": accounts,
        "last_backup": last_backup(&state.config.database_url),
    })))
}

/// `deploy/backup.sh` writes `last-backup` beside the database after every
/// snapshot: one line, `<UTC time> <label> <pushed|local>`. Running without
/// the deploy scripts there is simply no file.
pub fn last_backup(database_url: &str) -> Option<Value> {
    let path = crate::db::sqlite_path(database_url)?;
    let file = std::path::Path::new(&path).parent()?.join("last-backup");
    let text = std::fs::read_to_string(file).ok()?;
    let mut parts = text.split_whitespace();
    let at = parts.next()?.to_string();
    let label = parts.next().unwrap_or("").to_string();
    let pushed = parts.next() == Some("pushed");
    Some(json!({ "at": at, "label": label, "pushed": pushed }))
}
