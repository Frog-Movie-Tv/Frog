use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::push::{self, Vapid};
use crate::state::AppState;

/// The key a browser needs to subscribe, and how many devices already have.
pub async fn status(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let vapid = Vapid::load_or_create(&state.pool).await?;
    let devices = push::devices(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({
        "public_key": vapid.public_key(),
        "devices": devices,
    })))
}

#[derive(Deserialize)]
pub struct Keys {
    pub p256dh: String,
    pub auth: String,
}

/// The shape of `PushSubscription.toJSON()`, with the browser's time zone,
/// which decides when midnight is for a day with no time.
#[derive(Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    pub keys: Keys,
    #[serde(default)]
    pub time_zone: Option<String>,
}

pub async fn subscribe(
    auth: Auth,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(sub): Json<Subscription>,
) -> AppResult<Json<Value>> {
    if !push::endpoint_is_acceptable(&sub.endpoint) {
        return Err(AppError::BadRequest(
            "that is not a browser push service".into(),
        ));
    }
    // Refuse keys we could never encrypt for, now rather than at the first send.
    push::encrypt(&sub.keys.p256dh, &sub.keys.auth, b"check")?;

    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(200).collect::<String>());
    push::subscribe(
        &state.pool,
        auth.user_id(),
        &sub.endpoint,
        &sub.keys.p256dh,
        &sub.keys.auth,
        agent.as_deref(),
    )
    .await?;
    if let Some(zone) = sub.time_zone.as_deref() {
        // A zone this server does not know keeps the one it had.
        crate::announce::set_time_zone(&state.pool, auth.user_id(), zone).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct Endpoint {
    pub endpoint: String,
}

pub async fn unsubscribe(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<Endpoint>,
) -> AppResult<Json<Value>> {
    push::unsubscribe(&state.pool, auth.user_id(), &body.endpoint).await?;
    Ok(Json(json!({ "ok": true })))
}
