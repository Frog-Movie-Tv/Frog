//! Running the instance: who has an account, what they may do, and how the
//! instance itself is doing. Everything here sits behind `AdminAuth`.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::admin;
use crate::auth::{self, AdminAuth, Role};
use crate::error::{AppError, AppResult};
use crate::reset;
use crate::state::AppState;

pub async fn users(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let rows = admin::users(&state.pool).await?;
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
    let Some((target_role, _)) = admin::standing(&state.pool, id).await? else {
        return Err(AppError::NotFound("no such account".into()));
    };
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
        admin::set_role(&state.pool, id, role).await?;
    }

    if let Some(disabled) = patch.disabled {
        admin::set_disabled(&state.pool, id, disabled).await?;
        // Sign-in checks the flag on every request anyway; dropping the sessions
        // as well means the devices list never shows a session that cannot work.
        if disabled {
            auth::destroy_sessions_for(&state.pool, id, None).await?;
        }
    }

    // A link made for one role must not outlive it, and one for a switched-off
    // account has nothing left to open.
    if patch.role.is_some() || patch.disabled.is_some() {
        reset::withdraw(&state.pool, id).await?;
    }

    let (role, disabled) = admin::standing(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("no such account".into()))?;
    Ok(Json(
        json!({ "id": id, "role": role.as_db(), "disabled": disabled }),
    ))
}

/// Make a single-use link that sets a new password for someone who forgot
/// theirs. The same rules as changing an account: never yourself (Settings has
/// that), never the owner, and only the owner acts on an administrator. The code
/// comes back once.
pub async fn reset_link(
    AdminAuth(me): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    if id == me.user_id() {
        return Err(AppError::BadRequest(
            "Change your own password from Settings.".into(),
        ));
    }
    let Some((target_role, disabled)) = admin::standing(&state.pool, id).await? else {
        return Err(AppError::NotFound("no such account".into()));
    };
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
    if disabled {
        return Err(AppError::BadRequest("Switch this account on first.".into()));
    }

    let (code, expires) = reset::issue(&state.pool, id, me.user_id()).await?;
    tracing::info!(account = id, by = me.user_id(), "password reset link made");
    Ok(Json(json!({
        "code": code,
        "path": format!("/reset?code={code}"),
        "expires_at": expires.to_rfc3339(),
    })))
}

/// Remove someone from this Frog for good: their library, diary, shelves,
/// settings and sessions all go with the account (the foreign keys cascade).
///
/// The same rules as changing an account: never yourself (Settings has that),
/// never the owner, and only the owner removes an administrator.
pub async fn delete_user(
    AdminAuth(me): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    if id == me.user_id() {
        return Err(AppError::BadRequest(
            "Delete your own account from Settings.".into(),
        ));
    }
    let Some((target_role, _)) = admin::standing(&state.pool, id).await? else {
        return Err(AppError::NotFound("no such account".into()));
    };
    if target_role == Role::Owner {
        return Err(AppError::Forbidden(
            "The owner's account cannot be deleted.".into(),
        ));
    }
    if target_role == Role::Admin && me.role() < Role::Owner {
        return Err(AppError::Forbidden(
            "Only the owner can delete an administrator.".into(),
        ));
    }

    auth::destroy_sessions_for(&state.pool, id, None).await?;
    admin::delete_account(&state.pool, id).await?;
    tracing::info!(
        deleted = id,
        by = me.user_id(),
        "account deleted by an administrator"
    );
    Ok(Json(json!({ "ok": true })))
}

/// How the instance is doing: version, space used, and the last backup the
/// server's backup script recorded next to the database.
pub async fn instance(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let f = admin::figures(&state.pool).await?;
    let (titles, films, shows, manga) = f.titles;

    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "database_bytes": f.database_bytes,
        "image_cache_bytes": f.image_cache_bytes,
        "accounts": f.accounts,
        "titles": { "all": titles, "films": films, "shows": shows, "manga": manga },
        "last_backup": last_backup(&state.config.database_url),
    })))
}

/// What the server's own jobs last did: backups, updates, rollbacks. Written
/// by the deploy scripts beside the database; see `crate::ops`.
pub async fn server(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let events = crate::ops::read(&state.config.database_url);
    let mut out = crate::ops::summary(&events);
    out["recorded"] = json!(!events.is_empty());
    Ok(Json(out))
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
