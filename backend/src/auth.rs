use argon2::password_hash::rand_core::OsRng as ArgonRng;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, PasswordHash};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const COOKIE_NAME: &str = "frog_session";

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut ArgonRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Other(anyhow::anyhow!("hashing failed: {e}")))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Issue a session and return the raw token. Only its SHA-256 is persisted, so
/// a leaked database file does not hand over live sessions.
pub async fn create_session(
    pool: &SqlitePool,
    days: i64,
    user_agent: Option<&str>,
) -> AppResult<String> {
    let mut raw = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let now = Utc::now();
    let expires = now + Duration::days(days.max(1));

    sqlx::query(
        "INSERT INTO session (token_hash, created_at, expires_at, user_agent) VALUES (?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .bind(user_agent)
    .execute(pool)
    .await?;

    Ok(token)
}

pub async fn destroy_session(pool: &SqlitePool, token: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM session WHERE token_hash = ?")
        .bind(hash_token(token))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn purge_expired_sessions(pool: &SqlitePool) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM session WHERE expires_at < ?")
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn session_is_valid(pool: &SqlitePool, token: &str) -> bool {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT expires_at FROM session WHERE token_hash = ?")
            .bind(hash_token(token))
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    match row {
        Some((expires,)) => chrono::DateTime::parse_from_rfc3339(&expires)
            .map(|e| e > Utc::now())
            .unwrap_or(false),
        None => false,
    }
}

pub fn session_cookie(token: String, secure: bool, days: i64) -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, token);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(secure);
    cookie.set_path("/");
    cookie.set_max_age(::time::Duration::days(days.max(1)));
    cookie
}

pub fn clear_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, "");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(secure);
    cookie.set_path("/");
    cookie.set_max_age(::time::Duration::seconds(0));
    cookie
}

pub fn token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| {
            Cookie::split_parse(raw.to_string())
                .filter_map(|c| c.ok())
                .find(|c| c.name() == COOKIE_NAME)
                .map(|c| c.value().to_string())
        })
}

/// Extractor that gates every private route. Rejecting with 401 rather than a
/// redirect keeps the API honest; the SvelteKit shell turns that into a trip to
/// the login screen.
pub struct Auth;

impl FromRequestParts<AppState> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = token_from_headers(&parts.headers).ok_or(AppError::Unauthorised)?;
        if session_is_valid(&state.pool, &token).await {
            Ok(Auth)
        } else {
            Err(AppError::Unauthorised)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn token_hash_is_stable_and_not_the_token() {
        let h = hash_token("abc");
        assert_eq!(h, hash_token("abc"));
        assert_ne!(h, "abc");
        assert_eq!(h.len(), 64);
    }
}
