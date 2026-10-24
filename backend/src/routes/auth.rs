use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::auth as session;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

async fn account_exists(state: &AppState) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT username FROM app_user WHERE id = 1")
        .fetch_optional(&state.pool)
        .await?;
    Ok(row.map(|r| r.0))
}

/// The one endpoint the SvelteKit shell calls before anything else: it decides
/// between the setup screen, the login screen and the app.
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let username = account_exists(&state).await?;
    let authenticated = match session::token_from_headers(&headers) {
        Some(token) => session::session_is_valid(&state.pool, &token).await,
        None => false,
    };

    Ok(Json(json!({
        "needs_setup": username.is_none(),
        "authenticated": authenticated,
        "username": username,
        "tmdb_configured": state.tmdb.is_configured(),
        "region": state.config.region,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

/// First-run account creation. Closed for good the moment an account exists, so
/// exposing the instance on Tailscale before you have set it up is the only
/// window of risk, and that window is seconds long.
pub async fn setup(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> AppResult<impl IntoResponse> {
    if account_exists(&state).await?.is_some() {
        return Err(AppError::Conflict(
            "this instance already has an account".into(),
        ));
    }
    validate(&body)?;

    create_account(&state, &body.username, &body.password).await?;
    issue(state, jar, headers).await
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> AppResult<impl IntoResponse> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT username, password_hash FROM app_user WHERE id = 1")
            .fetch_optional(&state.pool)
            .await?;

    let Some((username, hash)) = row else {
        return Err(AppError::BadRequest(
            "no account yet — run setup first".into(),
        ));
    };

    // Same message either way: do not leak whether the username was right.
    if !username.eq_ignore_ascii_case(body.username.trim())
        || !session::verify_password(&body.password, &hash)
    {
        return Err(AppError::Unauthorised);
    }

    issue(state, jar, headers).await
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> AppResult<impl IntoResponse> {
    if let Some(c) = jar.get(session::COOKIE_NAME) {
        session::destroy_session(&state.pool, c.value()).await?;
    }
    let jar = jar.add(session::clear_cookie(state.config.secure_cookies));
    Ok((jar, Json(json!({ "ok": true }))))
}

async fn issue(
    state: AppState,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<(CookieJar, Json<serde_json::Value>)> {
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    let token = session::create_session(&state.pool, state.config.session_days, ua).await?;
    let jar = jar.add(session::session_cookie(
        token,
        state.config.secure_cookies,
        state.config.session_days,
    ));
    Ok((jar, Json(json!({ "ok": true }))))
}

fn validate(c: &Credentials) -> AppResult<()> {
    if c.username.trim().len() < 2 {
        return Err(AppError::BadRequest(
            "username needs at least 2 characters".into(),
        ));
    }
    if c.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password needs at least 8 characters".into(),
        ));
    }
    Ok(())
}

pub async fn create_account(state: &AppState, username: &str, password: &str) -> AppResult<()> {
    let hash = session::hash_password(password)?;
    sqlx::query(
        "INSERT INTO app_user (id, username, password_hash, created_at) VALUES (1, ?, ?, ?)",
    )
    .bind(username.trim())
    .bind(hash)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// Called at startup: if the env hands us credentials and there is no account,
/// create it so a fresh container is usable without a browser round trip.
pub async fn bootstrap_from_env(state: &AppState) -> AppResult<()> {
    let (Some(user), Some(pass)) = (
        state.config.admin_username.clone(),
        state.config.admin_password.clone(),
    ) else {
        return Ok(());
    };

    if account_exists(state).await?.is_some() {
        return Ok(());
    }
    if pass.len() < 8 {
        tracing::warn!("FROG_ADMIN_PASSWORD is shorter than 8 characters; skipping bootstrap");
        return Ok(());
    }

    create_account(state, &user, &pass).await?;
    tracing::info!(username = %user, "created the account from environment variables");
    Ok(())
}
