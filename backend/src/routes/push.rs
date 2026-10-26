use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::push::{self, Vapid};
use crate::state::AppState;

/// The key a browser needs to subscribe, and how many devices already have.
pub async fn status(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let vapid = Vapid::load_or_create(&state.pool).await?;
    let (devices,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM push_subscription WHERE user_id = ?")
            .bind(auth.user_id())
            .fetch_one(&state.pool)
            .await?;
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

/// The shape of `PushSubscription.toJSON()`.
#[derive(Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    pub keys: Keys,
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
    sqlx::query(
        "INSERT INTO push_subscription (user_id, endpoint, p256dh, auth, user_agent, created_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (user_id, endpoint) DO UPDATE SET
            p256dh = excluded.p256dh,
            auth = excluded.auth,
            user_agent = excluded.user_agent,
            failures = 0",
    )
    .bind(auth.user_id())
    .bind(&sub.endpoint)
    .bind(&sub.keys.p256dh)
    .bind(&sub.keys.auth)
    .bind(agent)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;
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
    sqlx::query("DELETE FROM push_subscription WHERE user_id = ? AND endpoint = ?")
        .bind(auth.user_id())
        .bind(&body.endpoint)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// Send a test notification to every device, so turning it on can be checked.
pub async fn test(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let result = push::broadcast(
        &state,
        auth.user_id(),
        &json!({
            "title": "Frog",
            "body": "Notifications are on. New episodes of the shows you follow will show up here.",
            "url": "/calendar",
            "tag": "frog-test",
        }),
    )
    .await?;
    if result.sent == 0 {
        let why = if result.failed > 0 {
            "The push service turned the notification down. Turn notifications off and on again on this device."
        } else {
            "This browser no longer accepts notifications from Frog. Turn them on again."
        };
        return Err(AppError::BadRequest(why.into()));
    }
    Ok(Json(json!(result)))
}
