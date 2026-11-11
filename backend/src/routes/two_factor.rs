//! Turning two-factor sign-in on and off. The code check at sign-in is
//! `two_factor::second_factor_ok`.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::totp;
use crate::two_factor;

pub async fn status(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (enabled, left) = two_factor::status(&state.pool, auth.user_id()).await?;
    Ok(Json(
        json!({ "enabled": enabled, "recovery_codes_left": left }),
    ))
}

/// A new secret to scan. It only takes effect once a code from it is confirmed.
pub async fn start(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (username, secret) = two_factor::begin(&state.pool, auth.user_id()).await?;

    let uri = totp::provisioning_uri(&secret, &username);
    let qr = qrcode::QrCode::new(uri.as_bytes())
        .map_err(|e| AppError::Other(anyhow::anyhow!("QR encoding failed: {e}")))?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();

    // Grouped in fours for people typing it into an app by hand.
    let grouped = secret
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(Json(json!({ "secret": grouped, "uri": uri, "qr_svg": qr })))
}

#[derive(Deserialize)]
pub struct ConfirmBody {
    pub code: String,
}

pub async fn confirm(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<ConfirmBody>,
) -> AppResult<Json<Value>> {
    let codes = two_factor::confirm(&state.pool, auth.user_id(), &body.code).await?;
    Ok(Json(json!({ "recovery_codes": codes })))
}

#[derive(Deserialize)]
pub struct PasswordBody {
    pub password: String,
}

pub async fn disable(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<PasswordBody>,
) -> AppResult<Json<Value>> {
    crate::auth::check_password(&state.pool, auth.user_id(), &body.password).await?;
    two_factor::disable(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "ok": true })))
}

/// A fresh set of recovery codes; the old ones stop working.
pub async fn new_recovery_codes(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<PasswordBody>,
) -> AppResult<Json<Value>> {
    crate::auth::check_password(&state.pool, auth.user_id(), &body.password).await?;
    let codes = two_factor::new_codes(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "recovery_codes": codes })))
}
