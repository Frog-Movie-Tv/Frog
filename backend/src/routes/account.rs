//! What any account can do to itself: its password, its signed-in devices,
//! whether others may see its profile, and leaving.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Auth, Role};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store;

/// Prefix of a session's token hash, enough to tell sessions apart in a list.
/// It cannot be turned back into a token, so showing it gives nothing away.
const SESSION_ID_LEN: usize = 16;

/// (username, role, created_at, profile_public, has two-factor,
///  display name, bio, banner backdrop, pins)
type AccountRow = (
    String,
    String,
    String,
    bool,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

pub async fn get(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let row: AccountRow = sqlx::query_as(
        "SELECT username, role, created_at, profile_public, totp_secret IS NOT NULL,
                display_name, bio, banner_backdrop, pins_json
           FROM app_user WHERE id = ?",
    )
    .bind(auth.user_id())
    .fetch_one(&state.pool)
    .await?;
    let images: Vec<(String,)> = sqlx::query_as("SELECT kind FROM user_image WHERE user_id = ?")
        .bind(auth.user_id())
        .fetch_all(&state.pool)
        .await?;
    let has = |kind: &str| images.iter().any(|(k,)| k == kind);
    let (banner_position,): (Option<String>,) =
        sqlx::query_as("SELECT banner_position FROM app_user WHERE id = ?")
            .bind(auth.user_id())
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(json!({
        "id": auth.user_id(),
        "username": row.0,
        "role": row.1,
        "created_at": row.2,
        "profile_public": row.3,
        "two_factor": row.4,
        "display_name": row.5,
        "bio": row.6,
        "banner_backdrop": row.7,
        "banner_position": banner_position,
        "pins": serde_json::from_str::<Value>(&row.8).unwrap_or(json!([])),
        "avatar": has("avatar"),
        "banner": has("banner"),
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

/// Only the fields sent change. For the text fields an empty string clears.
#[derive(Deserialize)]
pub struct ProfileBody {
    pub public: Option<bool>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub banner_backdrop: Option<String>,
    /// "x% y%", which part of the banner shows. Empty resets it.
    pub banner_position: Option<String>,
    pub pins: Option<Vec<Pin>>,
}

#[derive(Deserialize, serde::Serialize)]
pub struct Pin {
    /// Frog's id for a title in this account's library.
    pub title_id: i64,
}

const NAME_MAX: usize = 40;

/// Two percentages from 0 to 100, as CSS writes a position: "50% 30%".
fn is_position(value: &str) -> bool {
    let parts: Vec<&str> = value.split(' ').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            p.strip_suffix('%')
                .and_then(|n| n.parse::<f64>().ok())
                .is_some_and(|n| (0.0..=100.0).contains(&n))
        })
}
const BIO_MAX: usize = 600;
const PINS_MAX: usize = 4;

/// Trimmed text, `None` for empty, refused past `max` characters.
fn text(value: &str, max: usize, what: &str) -> AppResult<Option<String>> {
    let value = value.trim();
    if value.chars().count() > max {
        return Err(AppError::BadRequest(format!(
            "Keep {what} under {max} characters."
        )));
    }
    Ok((!value.is_empty()).then(|| value.to_string()))
}

pub async fn set_profile(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<ProfileBody>,
) -> AppResult<Json<Value>> {
    let pool = &state.pool;
    let me = auth.user_id();
    if let Some(public) = body.public {
        sqlx::query("UPDATE app_user SET profile_public = ? WHERE id = ?")
            .bind(public)
            .bind(me)
            .execute(pool)
            .await?;
    }
    if let Some(name) = &body.display_name {
        sqlx::query("UPDATE app_user SET display_name = ? WHERE id = ?")
            .bind(text(name, NAME_MAX, "your name")?)
            .bind(me)
            .execute(pool)
            .await?;
    }
    if let Some(bio) = &body.bio {
        sqlx::query("UPDATE app_user SET bio = ? WHERE id = ?")
            .bind(text(bio, BIO_MAX, "your about text")?)
            .bind(me)
            .execute(pool)
            .await?;
    }
    if let Some(position) = &body.banner_position {
        let position = position.trim();
        if !position.is_empty() && !is_position(position) {
            return Err(AppError::BadRequest(
                "That banner position cannot be used.".into(),
            ));
        }
        sqlx::query("UPDATE app_user SET banner_position = ? WHERE id = ?")
            .bind((!position.is_empty()).then_some(position))
            .bind(me)
            .execute(pool)
            .await?;
    }
    if let Some(path) = &body.banner_backdrop {
        // Only a TMDB image path: it is drawn through Frog's own image proxy.
        let path = path.trim();
        let valid = path.is_empty()
            || (path.starts_with('/')
                && path.len() < 80
                && path
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-')));
        if !valid {
            return Err(AppError::BadRequest("That banner cannot be used.".into()));
        }
        sqlx::query("UPDATE app_user SET banner_backdrop = ? WHERE id = ?")
            .bind((!path.is_empty()).then_some(path))
            .bind(me)
            .execute(pool)
            .await?;
    }
    if let Some(pins) = &body.pins {
        if pins.len() > PINS_MAX {
            return Err(AppError::BadRequest(format!(
                "Pin at most {PINS_MAX} titles."
            )));
        }
        for pin in pins {
            if store::get_entry(pool, me, pin.title_id).await?.is_none() {
                return Err(AppError::BadRequest(
                    "Only titles in your library can be pinned.".into(),
                ));
            }
        }
        sqlx::query("UPDATE app_user SET pins_json = ? WHERE id = ?")
            .bind(serde_json::to_string(pins)?)
            .bind(me)
            .execute(pool)
            .await?;
    }
    get(auth, State(state)).await
}

/// Pictures people upload: a profile picture and a banner.
const IMAGE_KINDS: [&str; 2] = ["avatar", "banner"];
pub const IMAGE_MAX_BYTES: usize = 3 * 1024 * 1024;

/// What the bytes are, from their first few, never from what the browser
/// claims. Only plain raster formats: no SVG, which can carry script.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub async fn set_image(
    auth: Auth,
    State(state): State<AppState>,
    Path(kind): Path<String>,
    body: Bytes,
) -> AppResult<Json<Value>> {
    if !IMAGE_KINDS.contains(&kind.as_str()) {
        return Err(AppError::NotFound("no such picture".into()));
    }
    if body.len() > IMAGE_MAX_BYTES {
        return Err(AppError::BadRequest(
            "That picture is too large; keep it under 3 MB.".into(),
        ));
    }
    let Some(mime) = sniff_image(&body) else {
        return Err(AppError::BadRequest(
            "Use a JPEG, PNG, WebP or GIF picture.".into(),
        ));
    };
    sqlx::query(
        "INSERT INTO user_image (user_id, kind, mime, bytes, updated_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (user_id, kind) DO UPDATE SET
            mime = excluded.mime, bytes = excluded.bytes, updated_at = excluded.updated_at",
    )
    .bind(auth.user_id())
    .bind(&kind)
    .bind(mime)
    .bind(body.to_vec())
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_image(
    auth: Auth,
    State(state): State<AppState>,
    Path(kind): Path<String>,
) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM user_image WHERE user_id = ? AND kind = ?")
        .bind(auth.user_id())
        .bind(&kind)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
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

/// Typed by hand before the library goes. Deliberately a phrase rather than a
/// button: this cannot be undone and there is no backup of one account.
pub const CLEAR_PHRASE: &str = "delete my library";

#[derive(Deserialize)]
pub struct ClearLibraryBody {
    pub password: String,
    pub confirm: String,
}

/// Empty this account's library and keep the account.
///
/// Goes: every title tracked, every watch, every episode rating and note, and
/// every shelf. Stays: the account itself, its settings, its devices and its
/// profile. Scoped to the caller throughout, so one account emptying its own
/// library cannot touch another's.
pub async fn clear_library(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<ClearLibraryBody>,
) -> AppResult<Json<Value>> {
    if body.confirm.trim().to_lowercase() != CLEAR_PHRASE {
        return Err(AppError::BadRequest(format!(
            "Type {CLEAR_PHRASE} to confirm."
        )));
    }
    check_password(&state, auth.user_id(), &body.password).await?;

    let gone = store::empty_library(&state.pool, auth.user_id()).await?;
    tracing::info!(
        user_id = auth.user_id(),
        titles = gone.titles,
        watches = gone.watches,
        shelves = gone.shelves,
        "library emptied"
    );
    Ok(Json(json!({
        "titles": gone.titles,
        "watches": gone.watches,
        "shelves": gone.shelves,
    })))
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
