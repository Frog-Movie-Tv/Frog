use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::auth::{self as session, ClientAddr, Role};
use crate::error::{AppError, AppResult};
use crate::routes::{invites, two_factor};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    /// Keep me signed in on this device. Off means a cookie that dies with
    /// the browser and a session the server forgets within a day.
    #[serde(default)]
    pub remember: bool,
    /// The six digits from an authenticator app, or a recovery code.
    #[serde(default)]
    pub code: Option<String>,
}

/// Joining with an invite: the same fields, plus the code from the link.
#[derive(Deserialize)]
pub struct Registration {
    #[serde(flatten)]
    pub credentials: Credentials,
    pub invite: String,
}

/// Failed sign-ins allowed before the door shuts for a while.
const MAX_FAILURES: usize = 8;
const FAILURE_WINDOW: Duration = Duration::from_secs(10 * 60);

/// Who to throttle. Both buckets are checked: the account stops someone
/// guessing one password from everywhere, and the address stops one caller
/// working through every account.
fn buckets(username: &str, client: &str) -> [String; 2] {
    [
        format!("user:{}", username.trim().to_lowercase()),
        format!("ip:{client}"),
    ]
}

/// The caller's address. `X-Forwarded-For` is trusted because the only
/// supported deployment puts Caddy directly in front of Frog; without a proxy
/// the socket address is used. A spoofed header can only dodge the per-address
/// bucket, never the per-account one.
fn client_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    for name in ["x-forwarded-for", "x-real-ip"] {
        if let Some(raw) = headers.get(name).and_then(|v| v.to_str().ok()) {
            if let Some(first) = raw.split(',').next() {
                let trimmed = first.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    peer.map(|p| p.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Returns the seconds to wait when either bucket has failed too often.
fn throttled_for(state: &AppState, keys: &[String; 2]) -> Option<u64> {
    let mut failures = state.login_failures.lock().ok()?;
    let now = Instant::now();
    let mut wait = None;

    for key in keys {
        let times = failures.entry(key.clone()).or_default();
        times.retain(|t| now.duration_since(*t) < FAILURE_WINDOW);
        if times.len() >= MAX_FAILURES {
            if let Some(oldest) = times.first().copied() {
                let left = (FAILURE_WINDOW - now.duration_since(oldest))
                    .as_secs()
                    .max(1);
                wait = Some(wait.map_or(left, |w: u64| w.max(left)));
            }
        }
    }
    wait
}

fn record_failure(state: &AppState, keys: &[String; 2]) {
    if let Ok(mut failures) = state.login_failures.lock() {
        for key in keys {
            failures
                .entry(key.clone())
                .or_default()
                .push(Instant::now());
        }
    }
}

fn clear_failures(state: &AppState, keys: &[String; 2]) {
    if let Ok(mut failures) = state.login_failures.lock() {
        for key in keys {
            failures.remove(key);
        }
    }
}

/// True while the instance has no accounts at all, which is the only time the
/// setup screen is reachable.
pub async fn needs_setup(state: &AppState) -> AppResult<bool> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM app_user")
        .fetch_one(&state.pool)
        .await?;
    Ok(count == 0)
}

/// The one endpoint the SvelteKit shell calls before anything else: it decides
/// between the setup screen, the login screen and the app.
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let identity = match session::token_from_headers(&headers) {
        Some(token) => session::session_user(&state.pool, &token).await,
        None => None,
    };

    Ok(Json(json!({
        "needs_setup": needs_setup(&state).await?,
        "authenticated": identity.is_some(),
        "username": identity.as_ref().map(|i| i.username.clone()),
        "role": identity.as_ref().map(|i| i.role),
        "is_admin": identity.as_ref().is_some_and(|i| i.role >= Role::Admin),
        "tmdb_configured": state.tmdb.is_configured(),
        "region": state.config.region,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

/// First-run account creation, which makes the owner. Closed for good the
/// moment any account exists; everyone after the owner arrives by invitation.
pub async fn setup(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> AppResult<impl IntoResponse> {
    if !needs_setup(&state).await? {
        return Err(AppError::Conflict(
            "this instance already has an account".into(),
        ));
    }
    validate(&body)?;

    let user_id = create_account(&state, &body.username, &body.password, Role::Owner).await?;
    issue(state, jar, headers, user_id, true).await
}

/// Join with an invite link. There is no open sign-up: without a live invite
/// this is indistinguishable from a wrong password.
pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<Registration>,
) -> AppResult<impl IntoResponse> {
    validate(&body.credentials)?;

    let role = invites::claim(&state, &body.invite).await?;
    let user_id = match create_account(
        &state,
        &body.credentials.username,
        &body.credentials.password,
        role,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            // The invite is only spent once an account actually exists, so a
            // clash over the username does not burn the link.
            invites::release(&state, &body.invite).await;
            return Err(e);
        }
    };
    invites::mark_used(&state, &body.invite, user_id).await?;

    issue(state, jar, headers, user_id, body.credentials.remember).await
}

/// (id, username, password hash, totp secret, disabled)
type LoginRow = (i64, String, String, Option<String>, i64);

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ClientAddr(peer): ClientAddr,
    Json(body): Json<Credentials>,
) -> AppResult<impl IntoResponse> {
    let client = client_key(&headers, peer);
    let keys = buckets(&body.username, &client);

    if let Some(seconds) = throttled_for(&state, &keys) {
        return Err(AppError::TooManyRequests(format!(
            "too many failed sign-ins; try again in {} minutes",
            seconds.div_ceil(60)
        )));
    }

    // NOCASE so signing in is not a spelling test, and one row per username.
    let row: Option<LoginRow> = sqlx::query_as(
        "SELECT id, username, password_hash, totp_secret, disabled
           FROM app_user WHERE username = ? COLLATE NOCASE",
    )
    .bind(body.username.trim())
    .fetch_optional(&state.pool)
    .await?;

    // Same message whichever part was wrong: never leak which usernames exist.
    let Some((user_id, _, hash, totp_secret, disabled)) = row else {
        record_failure(&state, &keys);
        return Err(AppError::Unauthorised);
    };
    if !session::verify_password(&body.password, &hash) {
        record_failure(&state, &keys);
        return Err(AppError::Unauthorised);
    }
    if disabled != 0 {
        return Err(AppError::Forbidden(
            "This account has been turned off. Ask an administrator.".into(),
        ));
    }

    if let Some(secret) = totp_secret {
        let code = body.code.as_deref().map(str::trim).unwrap_or("");
        if code.is_empty() {
            // Not a failure: the password was right and the app asks next.
            return Err(AppError::TwoFactor(
                "Enter the code from your authenticator app.".into(),
            ));
        }
        if !second_factor_ok(&state, user_id, &secret, code).await? {
            record_failure(&state, &keys);
            return Err(AppError::TwoFactor("That code didn't work.".into()));
        }
    }

    clear_failures(&state, &keys);
    issue(state, jar, headers, user_id, body.remember).await
}

/// Checks an authenticator code, or else spends one of this account's
/// recovery codes.
async fn second_factor_ok(
    state: &AppState,
    user_id: i64,
    secret: &str,
    code: &str,
) -> AppResult<bool> {
    if let Some(step) = crate::totp::verify(secret, code, Utc::now().timestamp()) {
        // Only a step later than the last accepted one, so a code seen over
        // someone's shoulder cannot be used again.
        let used = sqlx::query(
            "UPDATE app_user SET totp_last_step = ? WHERE id = ? AND totp_last_step < ?",
        )
        .bind(step)
        .bind(user_id)
        .bind(step)
        .execute(&state.pool)
        .await?;
        return Ok(used.rows_affected() == 1);
    }

    let spent = sqlx::query(
        "UPDATE recovery_code SET used_at = ?
          WHERE user_id = ? AND code_hash = ? AND used_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(user_id)
    .bind(two_factor::hash_recovery(code))
    .execute(&state.pool)
    .await?;
    Ok(spent.rows_affected() == 1)
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
    user_id: i64,
    remember: bool,
) -> AppResult<(CookieJar, Json<serde_json::Value>)> {
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    let days = if remember {
        state.config.session_days
    } else {
        1
    };

    let token = session::create_session(&state.pool, user_id, days, ua).await?;
    let jar = jar.add(session::session_cookie(
        token,
        state.config.secure_cookies,
        remember.then_some(days),
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

/// Create an account and return its id. The username is unique regardless of
/// case, so two people cannot pick names that only a reader can tell apart.
pub async fn create_account(
    state: &AppState,
    username: &str,
    password: &str,
    role: Role,
) -> AppResult<i64> {
    let name = username.trim();
    let (taken,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM app_user WHERE username = ? COLLATE NOCASE")
            .bind(name)
            .fetch_one(&state.pool)
            .await?;
    if taken > 0 {
        return Err(AppError::Conflict("that username is taken".into()));
    }

    let hash = session::hash_password(password)?;
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO app_user (username, password_hash, role, created_at)
         VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(hash)
    .bind(role.as_db())
    .bind(Utc::now().to_rfc3339())
    .fetch_one(&state.pool)
    .await?;
    Ok(row.0)
}

/// Called at startup: if the env hands us credentials and there is no account,
/// create the owner so a fresh container is usable without a browser round
/// trip.
pub async fn bootstrap_from_env(state: &AppState) -> AppResult<()> {
    let (Some(user), Some(pass)) = (
        state.config.admin_username.clone(),
        state.config.admin_password.clone(),
    ) else {
        return Ok(());
    };

    if !needs_setup(state).await? {
        return Ok(());
    }
    if pass.len() < 8 {
        tracing::warn!("FROG_ADMIN_PASSWORD is shorter than 8 characters; skipping bootstrap");
        return Ok(());
    }

    create_account(state, &user, &pass, Role::Owner).await?;
    tracing::info!(username = %user, "created the owner account from environment variables");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_account_and_address_gets_its_own_bucket() {
        let a = buckets("Zjef", "10.0.0.1");
        let b = buckets("zjef", "10.0.0.2");
        // Case does not make a second account bucket...
        assert_eq!(a[0], b[0]);
        // ...but a different address is throttled separately.
        assert_ne!(a[1], b[1]);
    }

    #[test]
    fn a_proxy_header_names_the_caller_and_the_socket_is_the_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        assert_eq!(client_key(&headers, None), "203.0.113.9");
        assert_eq!(client_key(&HeaderMap::new(), None), "unknown");
    }
}
