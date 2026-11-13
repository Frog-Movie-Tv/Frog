//! What any account can do to itself: its password, its signed-in devices,
//! whether others may see its profile, and leaving.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::account::{self, ProfileField};
use crate::auth::{self, Auth, Role};
use crate::error::{AppError, AppResult};
use crate::profiles;
use crate::reset;
use crate::state::AppState;
use crate::store;

/// Prefix of a session's token hash, enough to tell sessions apart in a list.
/// It cannot be turned back into a token, so showing it gives nothing away.
const SESSION_ID_LEN: usize = 16;

pub async fn get(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (row, images, look) = account::details(&state.pool, auth.user_id()).await?;
    let has = |kind: &str| images.iter().any(|k| k == kind);
    // Each pin as a card, so Settings can show and unpin any of them, wherever
    // it sits in the library.
    let pinned: Vec<Value> = profiles::pinned(&state.pool, auth.user_id(), &row.8)
        .await?
        .into_iter()
        .map(|(kind, id, name, poster_path, _)| {
            json!({ "id": id, "kind": kind, "name": name, "poster_path": poster_path })
        })
        .collect();
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
        "banner_position": look.banner_position,
        "profile_color": look.profile_color,
        "profile_shade": look.profile_shade,
        "banner_layout": look.banner_layout,
        "banner_height": look.banner_height,
        "pins": serde_json::from_str::<Value>(&row.8).unwrap_or(json!([])),
        "pinned": pinned,
        "avatar": has("avatar"),
        "banner": has("banner"),
    })))
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
    auth::check_password(&state.pool, auth.user_id(), &body.current).await?;
    if !auth::password_long_enough(&body.password) {
        return Err(AppError::BadRequest(
            "A new password needs at least 8 characters.".into(),
        ));
    }
    account::set_password_hash(
        &state.pool,
        auth.user_id(),
        &auth::hash_password(&body.password)?,
    )
    .await?;
    let signed_out =
        auth::destroy_sessions_for(&state.pool, auth.user_id(), Some(&auth.token)).await?;
    reset::withdraw(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "ok": true, "signed_out": signed_out })))
}

#[derive(Deserialize)]
pub struct UsernameBody {
    pub username: String,
    pub password: String,
}

/// Take another name, under the same rules as a new account. Nothing is keyed by
/// a name: an account's rows go by its id, and the addresses that do use one
/// (a profile, its pictures) are built from the current name on each request.
pub async fn change_username(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<UsernameBody>,
) -> AppResult<Json<Value>> {
    auth::check_password(&state.pool, auth.user_id(), &body.password).await?;
    auth::check_username(&body.username)?;
    let name = body.username.trim();
    let _one_at_a_time = state.names.lock().await;
    if auth::username_taken(&state.pool, name, Some(auth.user_id())).await? {
        return Err(AppError::Conflict("that username is taken".into()));
    }
    account::set_username(&state.pool, auth.user_id(), name).await?;
    Ok(Json(json!({ "username": name })))
}

pub async fn sessions(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let current = auth::token_fingerprint(&auth.token);
    let rows = account::sessions(&state.pool, auth.user_id()).await?;
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
    let gone = account::end_session(&state.pool, auth.user_id(), &id).await?;
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
    /// A colour from the list, or empty for the theme's own.
    pub profile_color: Option<String>,
    pub profile_shade: Option<String>,
    /// "classic" or "vertical"; empty goes back to classic.
    pub banner_layout: Option<String>,
    /// "short", "tall" or "full"; empty goes back to short.
    pub banner_height: Option<String>,
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
        account::set_profile_field(pool, me, ProfileField::Public(public)).await?;
    }
    if let Some(name) = &body.display_name {
        let name = text(name, NAME_MAX, "your name")?;
        account::set_profile_field(pool, me, ProfileField::DisplayName(name.as_deref())).await?;
    }
    if let Some(bio) = &body.bio {
        let bio = text(bio, BIO_MAX, "your about text")?;
        account::set_profile_field(pool, me, ProfileField::Bio(bio.as_deref())).await?;
    }
    if let Some(position) = &body.banner_position {
        let position = position.trim();
        if !position.is_empty() && !is_position(position) {
            return Err(AppError::BadRequest(
                "That banner position cannot be used.".into(),
            ));
        }
        let position = (!position.is_empty()).then_some(position);
        account::set_profile_field(pool, me, ProfileField::BannerPosition(position)).await?;
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
        account::set_banner_backdrop(pool, me, (!path.is_empty()).then_some(path)).await?;
    }
    if body.profile_color.is_some() || body.profile_shade.is_some() {
        let (color, shade) =
            profiles::check_colour(body.profile_color.as_deref(), body.profile_shade.as_deref())?;
        let field = match (&body.profile_color, &shade) {
            (Some(_), _) => Some(ProfileField::Colour(color.as_deref(), shade.as_deref())),
            (None, Some(shade)) => Some(ProfileField::Shade(shade)),
            (None, None) => None,
        };
        if let Some(field) = field {
            account::set_profile_field(pool, me, field).await?;
        }
    }
    if let Some(layout) = &body.banner_layout {
        let layout = profiles::check_choice(layout, &profiles::LAYOUTS, "banner layout")?;
        account::set_profile_field(pool, me, ProfileField::BannerLayout(layout.as_deref())).await?;
    }
    if let Some(height) = &body.banner_height {
        let height = profiles::check_choice(height, &profiles::HEIGHTS, "banner height")?;
        account::set_profile_field(pool, me, ProfileField::BannerHeight(height.as_deref())).await?;
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
        let pins = serde_json::to_string(pins)?;
        account::set_profile_field(pool, me, ProfileField::Pins(&pins)).await?;
    }
    get(auth, State(state)).await
}

/// Pictures people upload: a profile picture and a banner.
const IMAGE_KINDS: [&str; 2] = ["avatar", "banner"];
pub const IMAGE_MAX_BYTES: usize = 3 * 1024 * 1024;

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
    let Some(mime) = crate::images::sniff(&body) else {
        return Err(AppError::BadRequest(
            "Use a JPEG, PNG, WebP or GIF picture.".into(),
        ));
    };
    account::put_image(&state.pool, auth.user_id(), &kind, mime, body.to_vec()).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_image(
    auth: Auth,
    State(state): State<AppState>,
    Path(kind): Path<String>,
) -> AppResult<Json<Value>> {
    account::remove_image(&state.pool, auth.user_id(), &kind).await?;
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
    auth::check_password(&state.pool, auth.user_id(), &body.password).await?;
    crate::admin::delete_account(&state.pool, auth.user_id()).await?;
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
    auth::check_password(&state.pool, auth.user_id(), &body.password).await?;

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

/// This account's merges between titles, newest first, for Settings.
pub async fn merges(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({ "merges": store::merges(&state.pool, auth.user_id()).await? }),
    ))
}

/// Undo one merge: this account's data goes back on both titles.
pub async fn undo_merge(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    crate::anilist::undo_merge(&state, auth.user_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
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
