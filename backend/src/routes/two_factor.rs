//! Turning two-factor sign-in on and off. The login check itself lives in
//! `routes::auth`.

use axum::extract::State;
use axum::Json;
use chrono::Utc;
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth::{verify_password, Auth};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::totp;

const RECOVERY_CODES: usize = 10;

/// Recovery codes are compared without case, spaces or dashes, so they can
/// be typed however they were written down.
pub fn hash_recovery(code: &str) -> String {
    let clean: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    hex::encode(Sha256::digest(clean.as_bytes()))
}

fn new_recovery_code() -> String {
    // No 0/o or 1/l, which are easy to misread on paper.
    const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::rngs::OsRng;
    let mut pick = |n: usize| -> String {
        (0..n)
            .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
            .collect()
    };
    format!("{}-{}", pick(5), pick(5))
}

pub async fn status(_: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (enabled,): (bool,) =
        sqlx::query_as("SELECT totp_secret IS NOT NULL FROM app_user WHERE id = 1")
            .fetch_one(&state.pool)
            .await?;
    let (left,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM recovery_code WHERE used_at IS NULL")
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(
        json!({ "enabled": enabled, "recovery_codes_left": left }),
    ))
}

/// A new secret to scan. It only takes effect once a code from it is confirmed.
pub async fn start(_: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (username, enabled): (String, bool) =
        sqlx::query_as("SELECT username, totp_secret IS NOT NULL FROM app_user WHERE id = 1")
            .fetch_one(&state.pool)
            .await?;
    if enabled {
        return Err(AppError::Conflict(
            "Two-factor sign-in is already on.".into(),
        ));
    }

    let secret = totp::generate_secret();
    sqlx::query("UPDATE app_user SET totp_pending = ? WHERE id = 1")
        .bind(&secret)
        .execute(&state.pool)
        .await?;

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
    _: Auth,
    State(state): State<AppState>,
    Json(body): Json<ConfirmBody>,
) -> AppResult<Json<Value>> {
    let (pending,): (Option<String>,) =
        sqlx::query_as("SELECT totp_pending FROM app_user WHERE id = 1")
            .fetch_one(&state.pool)
            .await?;
    let Some(secret) = pending else {
        return Err(AppError::BadRequest("Start setup again.".into()));
    };
    let Some(step) = totp::verify(&secret, &body.code, Utc::now().timestamp()) else {
        return Err(AppError::BadRequest(
            "That code didn't work. Check the time on your phone and try the next one.".into(),
        ));
    };

    let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE app_user SET totp_secret = totp_pending, totp_pending = NULL, totp_last_step = ?
          WHERE id = 1",
    )
    .bind(step)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM recovery_code")
        .execute(&mut *tx)
        .await?;
    for code in &codes {
        sqlx::query("INSERT INTO recovery_code (code_hash) VALUES (?)")
            .bind(hash_recovery(code))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    Ok(Json(json!({ "recovery_codes": codes })))
}

#[derive(Deserialize)]
pub struct PasswordBody {
    pub password: String,
}

async fn check_password(state: &AppState, password: &str) -> AppResult<()> {
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM app_user WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;
    if verify_password(password, &hash) {
        Ok(())
    } else {
        Err(AppError::BadRequest("That password is not right.".into()))
    }
}

pub async fn disable(
    _: Auth,
    State(state): State<AppState>,
    Json(body): Json<PasswordBody>,
) -> AppResult<Json<Value>> {
    check_password(&state, &body.password).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE app_user SET totp_secret = NULL, totp_pending = NULL, totp_last_step = 0
          WHERE id = 1",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM recovery_code")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// A fresh set of recovery codes; the old ones stop working.
pub async fn new_recovery_codes(
    _: Auth,
    State(state): State<AppState>,
    Json(body): Json<PasswordBody>,
) -> AppResult<Json<Value>> {
    check_password(&state, &body.password).await?;
    let (enabled,): (bool,) =
        sqlx::query_as("SELECT totp_secret IS NOT NULL FROM app_user WHERE id = 1")
            .fetch_one(&state.pool)
            .await?;
    if !enabled {
        return Err(AppError::BadRequest(
            "Turn on two-factor sign-in first.".into(),
        ));
    }
    let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM recovery_code")
        .execute(&mut *tx)
        .await?;
    for code in &codes {
        sqlx::query("INSERT INTO recovery_code (code_hash) VALUES (?)")
            .bind(hash_recovery(code))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "recovery_codes": codes })))
}
