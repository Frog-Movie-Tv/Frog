//! Badges over HTTP. Making, editing and giving them out sits behind
//! `AdminAuth`; arranging the ones you hold is yours, behind `Auth`. Their
//! pictures are served through a profile (`routes::profiles::badge_image`).

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{AdminAuth, Auth};
use crate::badges::{self, Pictures, Removed, Size};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Room for the three pictures as base64, at their largest.
pub const BODY_MAX_BYTES: usize = 20 * 1024 * 1024;

fn missing() -> AppError {
    AppError::NotFound("no such badge".into())
}

fn decode(field: &str, text: &str) -> AppResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(text.as_bytes())
        .map_err(|_| AppError::BadRequest(format!("The {field} picture could not be read.")))
}

/// Every badge, and who holds each.
pub async fn list(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let waiting: Vec<Value> = crate::awards::give::waiting(&state.pool)
        .await?
        .into_iter()
        .map(|e| json!({ "key": e.key, "name": e.name, "description": e.about }))
        .collect();
    Ok(Json(json!({
        "badges": badges::all(&state.pool).await?,
        "waiting": waiting,
    })))
}

pub async fn get(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let (badge, settings) = badges::one(&state.pool, id).await?.ok_or_else(missing)?;
    let settings: Value = serde_json::from_str(&settings).unwrap_or_else(|_| json!({}));
    Ok(Json(json!({ "badge": badge, "settings": settings })))
}

/// A badge as the editor sends it. The pictures are base64, finished in the
/// browser; on an edit they may be left out to keep the ones there are.
#[derive(Deserialize)]
pub struct BadgeBody {
    pub name: String,
    pub description: Option<String>,
    pub settings: Option<Value>,
    pub large: Option<String>,
    pub small: Option<String>,
    pub original: Option<String>,
}

impl BadgeBody {
    fn pictures(&self) -> AppResult<Option<Pictures>> {
        match (&self.large, &self.small, &self.original) {
            (None, None, None) => Ok(None),
            (Some(large), Some(small), Some(original)) => Ok(Some(Pictures::check(
                decode("large", large)?,
                decode("small", small)?,
                decode("original", original)?,
            )?)),
            _ => Err(AppError::BadRequest(
                "Send the badge's pictures together.".into(),
            )),
        }
    }

    fn settings(&self) -> AppResult<Option<String>> {
        self.settings
            .as_ref()
            .map(|s| badges::check_settings(&s.to_string()))
            .transpose()
    }
}

pub async fn create(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Json(body): Json<BadgeBody>,
) -> AppResult<Json<Value>> {
    let name = badges::check_name(&body.name)?;
    let description = badges::check_description(body.description.as_deref())?;
    let settings = body.settings()?.unwrap_or_else(|| "{}".into());
    let Some(pictures) = body.pictures()? else {
        return Err(AppError::BadRequest("A badge needs its picture.".into()));
    };
    let id = badges::create(
        &state.pool,
        &name,
        description.as_deref(),
        &settings,
        &pictures,
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn update(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<BadgeBody>,
) -> AppResult<Json<Value>> {
    let name = badges::check_name(&body.name)?;
    let description = badges::check_description(body.description.as_deref())?;
    let settings = body.settings()?;
    let pictures = body.pictures()?;
    let found = badges::update(
        &state.pool,
        id,
        &name,
        description.as_deref(),
        settings.as_deref(),
        pictures.as_ref(),
    )
    .await?;
    if !found {
        return Err(missing());
    }
    Ok(Json(json!({ "id": id })))
}

/// Remove a badge from everyone. The answer is everything it was, which is
/// what undo sends back.
pub async fn remove(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let removed = badges::remove(&state.pool, id).await?.ok_or_else(missing)?;
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Deserialize)]
pub struct RestoreBody {
    pub removed: Removed,
}

pub async fn restore(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Json(body): Json<RestoreBody>,
) -> AppResult<Json<Value>> {
    let id = badges::restore(&state.pool, &body.removed).await?;
    Ok(Json(json!({ "id": id })))
}

/// A finished picture, for Administration.
pub async fn picture(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path((id, size)): Path<(i64, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let size = Size::parse(&size).ok_or_else(missing)?;
    let (bytes, version) = badges::picture(&state.pool, id, size)
        .await?
        .ok_or_else(missing)?;
    Ok(png(bytes, "image/png", &version, &headers))
}

/// What was uploaded, for the editor to start again from.
pub async fn original(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (bytes, mime) = badges::original(&state.pool, id)
        .await?
        .ok_or_else(missing)?;
    let mut response = png(bytes, &mime, "", &headers);
    // Changes whenever the badge does, and is only ever read by the editor.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// A picture, cached for good: its address carries its version, so a new
/// picture is a new address.
pub fn png(bytes: Vec<u8>, mime: &str, version: &str, headers: &HeaderMap) -> Response {
    let etag = format!("\"{}\"", version.replace(['"', ' '], ""));
    let fresh = !version.is_empty()
        && headers
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
            == Some(etag.as_str());
    let mut response = if fresh {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        bytes.into_response()
    };
    let h = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    if let (false, Ok(v)) = (version.is_empty(), HeaderValue::from_str(&etag)) {
        h.insert(header::ETAG, v);
    }
    // Private: behind a sign-in, never in a shared cache.
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    response
}

/// Give a badge to someone. They hear about it on any device they asked for
/// notifications on; giving it again says nothing.
pub async fn award(
    AdminAuth(me): AdminAuth,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(i64, i64)>,
) -> AppResult<Json<Value>> {
    let (badge, _) = badges::one(&state.pool, id).await?.ok_or_else(missing)?;
    let Some(username) = badges::holder_name(&state.pool, user_id).await? else {
        return Err(AppError::NotFound("no such account".into()));
    };
    if badges::award(&state.pool, id, user_id, me.user_id()).await? {
        let payload = json!({
            "title": "A new badge",
            "body": match &badge.description {
                Some(d) => format!("{} · {d}", badge.name),
                None => badge.name.clone(),
            },
            "url": format!("/people/{}", urlencoding::encode(&username)),
            "tag": format!("frog-badge-{id}"),
        });
        crate::inbox::tell(
            &state,
            user_id,
            &crate::inbox::Event {
                kind: crate::inbox::BADGE,
                actor_id: None,
                payload: json!({ "badge_id": id, "name": badge.name, "username": username }),
                ref_key: Some(format!("badge:{id}:{}", chrono::Utc::now().timestamp())),
                ..Default::default()
            },
            Some(payload),
        )
        .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn revoke(
    AdminAuth(_): AdminAuth,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(i64, i64)>,
) -> AppResult<Json<Value>> {
    badges::revoke(&state.pool, id, user_id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// The badges you hold, hidden ones too, for Settings.
pub async fn mine(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let held = badges::held(&state.pool, auth.user_id(), true).await?;
    let username = badges::holder_name(&state.pool, auth.user_id())
        .await?
        .unwrap_or_default();
    Ok(Json(json!({ "badges": shaped(&username, &held) })))
}

/// Everything there is to earn on one profile: the badges held, as the
/// viewer may see them, then every badge Frog gives on its own that is still
/// to come, with how far along the holder is when the viewer is the holder.
/// A badge held out of sight is neither, so it never reads as still to come.
pub async fn everything(
    auth: Auth,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> AppResult<Json<Value>> {
    let pool = &state.pool;
    let row = crate::profiles::visible(pool, auth.user_id(), &username).await?;
    let (user_id, username) = (row.0, row.1);
    let own = user_id == auth.user_id();
    let held = badges::held(pool, user_id, true).await?;
    let holding: std::collections::HashSet<i64> = held.iter().map(|b| b.id).collect();
    let earned: Vec<badges::Held> = held.into_iter().filter(|b| own || b.shown).collect();
    let progress = if own {
        crate::awards::progress(pool, user_id).await?
    } else {
        Default::default()
    };
    let mut rows = badges::automatic(pool).await?;
    let place = |key: &str| {
        crate::awards::catalogue::all()
            .iter()
            .position(|e| e.key == key)
            .unwrap_or(usize::MAX)
    };
    rows.sort_by_key(|b| place(&b.rule));
    let total = rows.len() + earned.iter().filter(|b| b.rule.is_none()).count();
    let locked: Vec<Value> = rows
        .iter()
        .filter(|b| !holding.contains(&b.id))
        .map(|b| {
            let entry = crate::awards::catalogue::get(&b.rule);
            let step = entry.and_then(|e| e.step.as_ref());
            let address = |size: &str| {
                format!(
                    "/api/badges/{}/{size}?v={}",
                    b.id,
                    urlencoding::encode(&b.version)
                )
            };
            json!({
                "id": b.id,
                "name": b.name,
                "description": b.description,
                "small": address("small"),
                "large": address("large"),
                "moment": entry.is_some_and(|e| e.kind == crate::awards::catalogue::Kind::Moment),
                "ladder": step.map(|s| s.ladder.as_str()),
                "step": step.map(|s| s.index),
                "progress": progress.get(&b.rule).map(|(have, need)| json!({
                    "have": have.floor(),
                    "need": need,
                })),
            })
        })
        .collect();
    Ok(Json(json!({
        "earned": shaped(&username, &earned),
        "locked": locked,
        "total": total,
    })))
}

/// The picture of a badge Frog gives on its own, for the list of what there
/// is to earn. Badges made by hand are only seen through a holder.
pub async fn automatic_picture(
    _auth: Auth,
    State(state): State<AppState>,
    Path((id, size)): Path<(i64, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let size = Size::parse(&size).ok_or_else(missing)?;
    if !badges::is_automatic(&state.pool, id).await? {
        return Err(missing());
    }
    let (bytes, version) = badges::picture(&state.pool, id, size)
        .await?
        .ok_or_else(missing)?;
    Ok(png(bytes, "image/png", &version, &headers))
}

/// Badges as pages draw them, with their pictures' addresses.
pub fn shaped(username: &str, held: &[badges::Held]) -> Vec<Value> {
    held.iter()
        .map(|b| {
            let step = b
                .rule
                .as_deref()
                .and_then(crate::awards::catalogue::get)
                .and_then(|e| e.step.as_ref());
            json!({
                "id": b.id,
                "name": b.name,
                "description": b.description,
                "awarded_at": b.awarded_at,
                "shown": b.shown,
                "small": badges::address(username, b, Size::Small),
                "large": badges::address(username, b, Size::Large),
                "automatic": b.rule.is_some(),
                "earned_on": b.earned_on,
                "earned_with": b.earned_with,
                // A ladder's steps, so a profile can show only the highest.
                "ladder": step.map(|s| s.ladder.as_str()),
                "step": step.map(|s| s.index),
            })
        })
        .collect()
}

#[derive(Deserialize)]
pub struct Arranged {
    pub id: i64,
    pub shown: bool,
}

#[derive(Deserialize)]
pub struct ArrangeBody {
    pub badges: Vec<Arranged>,
}

/// Your order, and which show. Only badges you hold are touched.
pub async fn arrange(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<ArrangeBody>,
) -> AppResult<Json<Value>> {
    if body.badges.len() > 500 {
        return Err(AppError::BadRequest(
            "That is more badges than exist.".into(),
        ));
    }
    let order: Vec<(i64, bool)> = body.badges.iter().map(|b| (b.id, b.shown)).collect();
    badges::arrange(&state.pool, auth.user_id(), &order).await?;
    mine(auth, State(state)).await
}
