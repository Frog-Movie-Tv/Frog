//! What any account can do to itself: its password, its signed-in devices,
//! whether others may see its profile, and leaving.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Auth, Role};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Prefix of a session's token hash, enough to tell sessions apart in a list.
/// It cannot be turned back into a token, so showing it gives nothing away.
const SESSION_ID_LEN: usize = 16;

/// (username, role, created_at, profile_public, has two-factor)
type AccountRow = (String, String, String, bool, bool);

pub async fn get(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (username, role, created_at, profile_public, two_factor): AccountRow = sqlx::query_as(
        "SELECT username, role, created_at, profile_public, totp_secret IS NOT NULL
           FROM app_user WHERE id = ?",
    )
    .bind(auth.user_id())
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({
        "id": auth.user_id(),
        "username": username,
        "role": role,
        "created_at": created_at,
        "profile_public": profile_public,
        "two_factor": two_factor,
    })))
}

async fn check_password(state: &AppState, user_id: i64, password: &str) -> AppResult<()> {
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM app_user WHERE id = ?")
        .bind(user_id)
        .fetch_one(&state.pool)
        .await?;
    if auth::verify_password(password, &hash) {
        Ok(())
    } else {
        Err(AppError::BadRequest("That password is not right.".into()))
    }
}

#[derive(Deserialize)]
pub struct PasswordBody {
    pub current: String,
    pub password: String,
}

/// Change the password and sign out every other device, which is the point of
/// changing it after a scare.
pub async fn change_password(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<PasswordBody>,
) -> AppResult<Json<Value>> {
    check_password(&state, auth.user_id(), &body.current).await?;
    if body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "A new password needs at least 8 characters.".into(),
        ));
    }
    sqlx::query("UPDATE app_user SET password_hash = ? WHERE id = ?")
        .bind(auth::hash_password(&body.password)?)
        .bind(auth.user_id())
        .execute(&state.pool)
        .await?;
    let signed_out =
        auth::destroy_sessions_for(&state.pool, auth.user_id(), Some(&auth.token)).await?;
    Ok(Json(json!({ "ok": true, "signed_out": signed_out })))
}

/// (token hash, created_at, expires_at, user agent)
type SessionRow = (String, String, String, Option<String>);

pub async fn sessions(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let current = auth::token_fingerprint(&auth.token);
    let rows: Vec<SessionRow> = sqlx::query_as(
        "SELECT token_hash, created_at, expires_at, user_agent
           FROM session WHERE user_id = ? AND expires_at > ?
          ORDER BY created_at DESC",
    )
    .bind(auth.user_id())
    .bind(chrono::Utc::now().to_rfc3339())
    .fetch_all(&state.pool)
    .await?;
    let sessions: Vec<Value> = rows
        .into_iter()
        .map(|(hash, created_at, expires_at, agent)| {
            json!({
                "id": &hash[..SESSION_ID_LEN.min(hash.len())],
                "created_at": created_at,
                "expires_at": expires_at,
                "device": agent.as_deref().map(describe_agent),
                "current": hash == current,
            })
        })
        .collect();
    Ok(Json(json!({ "sessions": sessions })))
}

/// "Chrome on Android" from a user-agent string. Good enough to recognise your
/// own devices; it is a label, not a fingerprint.
pub fn describe_agent(agent: &str) -> String {
    let a = agent.to_ascii_lowercase();
    let browser = if a.contains("edg/") {
        "Edge"
    } else if a.contains("firefox/") {
        "Firefox"
    } else if a.contains("samsungbrowser/") {
        "Samsung Internet"
    } else if a.contains("chrome/") || a.contains("crios/") {
        "Chrome"
    } else if a.contains("safari/") {
        "Safari"
    } else {
        "A browser"
    };
    let system = if a.contains("android") {
        "Android"
    } else if a.contains("iphone") || a.contains("ipad") {
        "iOS"
    } else if a.contains("windows") {
        "Windows"
    } else if a.contains("mac os") || a.contains("macintosh") {
        "macOS"
    } else if a.contains("linux") {
        "Linux"
    } else {
        return browser.to_string();
    };
    format!("{browser} on {system}")
}

/// Sign one of your own devices out.
pub async fn end_session(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    if id.len() != SESSION_ID_LEN || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::NotFound("no such session".into()));
    }
    let gone =
        sqlx::query("DELETE FROM session WHERE user_id = ? AND substr(token_hash, 1, ?) = ?")
            .bind(auth.user_id())
            .bind(SESSION_ID_LEN as i64)
            .bind(&id)
            .execute(&state.pool)
            .await?
            .rows_affected();
    if gone == 0 {
        return Err(AppError::NotFound("no such session".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Every device but this one.
pub async fn end_other_sessions(
    auth: Auth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let signed_out =
        auth::destroy_sessions_for(&state.pool, auth.user_id(), Some(&auth.token)).await?;
    Ok(Json(json!({ "signed_out": signed_out })))
}

#[derive(Deserialize)]
pub struct ProfileBody {
    pub public: bool,
}

pub async fn set_profile(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<ProfileBody>,
) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE app_user SET profile_public = ? WHERE id = ?")
        .bind(body.public)
        .bind(auth.user_id())
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "profile_public": body.public })))
}

#[derive(Deserialize)]
pub struct DeleteBody {
    pub password: String,
}

/// Leave the instance, taking every personal row along (the foreign keys
/// cascade). The owner cannot: the instance is theirs, and without an owner
/// nobody could invite an administrator again.
pub async fn delete(
    auth: Auth,
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<DeleteBody>,
) -> AppResult<impl IntoResponse> {
    if auth.role() == Role::Owner {
        return Err(AppError::BadRequest(
            "The owner's account cannot be deleted.".into(),
        ));
    }
    check_password(&state, auth.user_id(), &body.password).await?;
    sqlx::query("DELETE FROM app_user WHERE id = ?")
        .bind(auth.user_id())
        .execute(&state.pool)
        .await?;
    let jar = jar.add(auth::clear_cookie(state.config.secure_cookies));
    Ok((jar, Json(json!({ "ok": true }))))
}

#[cfg(test)]
mod tests {
    use super::describe_agent;

    #[test]
    fn devices_get_a_readable_name() {
        assert_eq!(
            describe_agent("Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 Chrome/155.0 Mobile Safari/537.36"),
            "Chrome on Android"
        );
        assert_eq!(
            describe_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0"
            ),
            "Firefox on Windows"
        );
        assert_eq!(
            describe_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 Version/19.0 Mobile/15E148 Safari/604.1"),
            "Safari on iOS"
        );
        assert_eq!(describe_agent("curl/8.0"), "A browser");
    }
}
