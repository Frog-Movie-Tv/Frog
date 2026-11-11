use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;

use crate::auth::{self as session, ClientAddr, Role};
use crate::error::{AppError, AppResult};
use crate::reset;
use crate::state::AppState;
use crate::store;
use crate::throttle;

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

/// The wait a throttle asked for, as a person reads it.
fn too_many_tries(seconds: u64) -> AppError {
    let minutes = seconds.div_ceil(60);
    AppError::TooManyRequests(format!(
        "Too many tries. Try again in {minutes} {}.",
        if minutes == 1 { "minute" } else { "minutes" }
    ))
}

/// True while the instance has no accounts at all, which is the only time the
/// setup screen is reachable.
pub async fn needs_setup(state: &AppState) -> AppResult<bool> {
    Ok(session::account_count(&state.pool).await? == 0)
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

    // The profile picture's address, so the top bar draws it without asking
    // for a picture that is not there.
    let avatar = match &identity {
        Some(i) => session::avatar_changed_at(&state.pool, i.user_id)
            .await?
            .map(|at| store::picture_address(&i.username, "avatar", &at)),
        None => None,
    };

    Ok(Json(json!({
        "needs_setup": needs_setup(&state).await?,
        "authenticated": identity.is_some(),
        "avatar": avatar,
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

    let role = crate::invites::claim(&state.pool, &body.invite).await?;
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
            crate::invites::release(&state.pool, &body.invite).await;
            return Err(e);
        }
    };
    crate::invites::mark_used(&state.pool, &body.invite, user_id).await?;

    issue(state, jar, headers, user_id, body.credentials.remember).await
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ClientAddr(peer): ClientAddr,
    Json(body): Json<Credentials>,
) -> AppResult<impl IntoResponse> {
    // Nothing about a name this long is worth keeping, and no account has one.
    if session::sign_in_name_too_long(&body.username) {
        return Err(AppError::WrongCredentials);
    }

    let client = client_key(&headers, peer);
    let keys = throttle::sign_in_keys(&body.username, &client);

    state.login_failures.admit(&keys).map_err(too_many_tries)?;

    // NOCASE so signing in is not a spelling test, and one row per username.
    let row = session::sign_in_row(&state.pool, &body.username).await?;

    // Same message whichever part was wrong: never leak which usernames exist.
    // The password work happens either way, so the answer takes as long for a
    // name nobody has as for a real one.
    let Some((user_id, _, hash, totp_secret, disabled)) = row else {
        session::spend_password_work(&body.password);
        return Err(AppError::WrongCredentials);
    };
    if !session::verify_password(&body.password, &hash) {
        return Err(AppError::WrongCredentials);
    }
    if disabled != 0 {
        state.login_failures.release(&keys);
        return Err(AppError::Forbidden(
            "This account has been turned off. Ask an administrator.".into(),
        ));
    }

    if let Some(secret) = totp_secret {
        let code = body.code.as_deref().map(str::trim).unwrap_or("");
        if code.is_empty() {
            // Not a failure: the password was right and the app asks next.
            state.login_failures.release(&keys);
            return Err(AppError::TwoFactor(
                "Enter the code from your authenticator app.".into(),
            ));
        }
        if !crate::two_factor::second_factor_ok(&state.pool, user_id, &secret, code).await? {
            return Err(AppError::TwoFactor("That code didn't work.".into()));
        }
    }

    state.login_failures.clear(&keys);
    issue(state, jar, headers, user_id, body.remember).await
}

#[derive(Deserialize)]
pub struct ResetBody {
    pub code: String,
    pub password: String,
}

/// Set a new password with a reset link.
///
/// Public, because the person using it cannot sign in. What keeps that safe:
/// the code is 128 random bits, only its hash is stored, it works once and
/// expires, and attempts are throttled by address. Every way a code can be wrong
/// gets one answer, after the same password work, so this says nothing about
/// which codes exist or who they are for. Nothing is signed in here: the person
/// signs in with the new password, which is also where two-factor asks.
pub async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    ClientAddr(peer): ClientAddr,
    Json(body): Json<ResetBody>,
) -> AppResult<impl IntoResponse> {
    let client = client_key(&headers, peer);
    let keys = throttle::reset_keys(&client);
    state.reset_failures.admit(&keys).map_err(too_many_tries)?;

    // Says nothing about the code, so it can come first and costs no try.
    if !session::password_long_enough(&body.password) {
        state.reset_failures.release(&keys);
        return Err(AppError::BadRequest(
            "password needs at least 8 characters".into(),
        ));
    }

    let Some(username) = reset::redeem(&state.pool, &body.code, &body.password).await? else {
        return Err(AppError::Forbidden(
            "That link has been used already, or it has expired.".into(),
        ));
    };
    state.reset_failures.release(&keys);
    // Whoever forgot a password has usually just used up their tries.
    state
        .login_failures
        .clear(&throttle::sign_in_keys(&username, &client));
    tracing::info!("a password was changed with a reset link");
    Ok(Json(json!({ "ok": true })))
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
    session::check_username(&c.username)?;
    if !session::password_long_enough(&c.password) {
        return Err(AppError::BadRequest(
            "password needs at least 8 characters".into(),
        ));
    }
    Ok(())
}

/// Create an account and return its id.
pub async fn create_account(
    state: &AppState,
    username: &str,
    password: &str,
    role: Role,
) -> AppResult<i64> {
    let name = username.trim();
    // Before the lock: the slow part need not make anyone else wait.
    let hash = session::hash_password(password)?;

    let _one_at_a_time = state.names.lock().await;
    if session::username_taken(&state.pool, name, None).await? {
        return Err(AppError::Conflict("that username is taken".into()));
    }

    session::insert_user(&state.pool, name, &hash, role).await
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
    // Refused the way a short password is, so an owner made here obeys the same
    // rules as one made on the setup screen, which stays open for it. Only a
    // database with no accounts gets this far, so nobody is ever renamed.
    if let Err(e) = session::check_username(&user) {
        tracing::warn!(error = %e, "FROG_ADMIN_USERNAME cannot be used; skipping bootstrap");
        return Ok(());
    }
    if !session::password_long_enough(&pass) {
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
    fn a_proxy_header_names_the_caller_and_the_socket_is_the_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        assert_eq!(client_key(&headers, None), "203.0.113.9");
        assert_eq!(client_key(&HeaderMap::new(), None), "unknown");
    }
}
