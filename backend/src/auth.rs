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

/// What an account is allowed to do. Ordered, so a check reads
/// `role >= Role::Admin` rather than listing every role that qualifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
    Owner,
}

impl Role {
    pub fn from_db(raw: &str) -> Self {
        match raw {
            "owner" => Role::Owner,
            "admin" => Role::Admin,
            _ => Role::User,
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::User => "user",
        }
    }
}

/// Columns a request needs about whoever is making it.
type SessionRow = (i64, String, String, i64);

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
    user_id: i64,
    days: i64,
    user_agent: Option<&str>,
) -> AppResult<String> {
    let mut raw = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let now = Utc::now();
    let expires = now + Duration::days(days.max(1));

    sqlx::query(
        "INSERT INTO session (token_hash, user_id, created_at, expires_at, user_agent)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .bind(user_agent)
    .execute(pool)
    .await?;

    // Every session starts with a sign-in, and this outlives the session.
    sqlx::query("UPDATE app_user SET last_sign_in_at = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(token)
}

/// Sign out every device for one account, optionally sparing the session making
/// the request so "sign out everywhere" does not sign you out of here too.
pub async fn destroy_sessions_for(
    pool: &SqlitePool,
    user_id: i64,
    except: Option<&str>,
) -> AppResult<u64> {
    let res = match except {
        Some(token) => {
            sqlx::query("DELETE FROM session WHERE user_id = ? AND token_hash <> ?")
                .bind(user_id)
                .bind(hash_token(token))
                .execute(pool)
                .await?
        }
        None => {
            sqlx::query("DELETE FROM session WHERE user_id = ?")
                .bind(user_id)
                .execute(pool)
                .await?
        }
    };
    Ok(res.rows_affected())
}

/// Hash of a raw token, so a caller can mark which listed session is this one
/// without the token ever being compared in the clear elsewhere.
pub fn token_fingerprint(token: &str) -> String {
    hash_token(token)
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

/// Resolve a raw session token to the account it belongs to. Returns None for
/// an unknown or expired token, and for a disabled account — disabling has to
/// take effect on the next request, not at the next sign-in.
pub async fn session_user(pool: &SqlitePool, token: &str) -> Option<Identity> {
    let row: Option<SessionRow> = sqlx::query_as(
        "SELECT u.id, u.username, u.role, u.disabled
           FROM session s
           JOIN app_user u ON u.id = s.user_id
          WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(hash_token(token))
    .bind(Utc::now().to_rfc3339())
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((_, _, _, disabled)) if disabled != 0 => None,
        Some((id, username, role, _)) => Some(Identity {
            user_id: id,
            username,
            role: Role::from_db(&role),
        }),
        None => None,
    }
}

/// `days` of `None` makes a session cookie: it dies with the browser, which
/// is what "do not remember this device" has to mean to be worth offering.
pub fn session_cookie(token: String, secure: bool, days: Option<i64>) -> Cookie<'static> {
    let mut cookie = Cookie::new(COOKIE_NAME, token);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(secure);
    cookie.set_path("/");
    if let Some(days) = days {
        cookie.set_max_age(::time::Duration::days(days.max(1)));
    }
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

/// The caller's socket address, when the server was started with connect info
/// and something is actually listening on a socket. It is None under the test
/// harness, so this extractor never fails a request.
pub struct ClientAddr(pub Option<std::net::SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientAddr(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|c| c.0),
        ))
    }
}

/// Who is making this request.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Identity {
    pub user_id: i64,
    pub username: String,
    pub role: Role,
}

/// Extractor that gates every private route. Rejecting with 401 rather than a
/// redirect keeps the API honest; the SvelteKit shell turns that into a trip to
/// the login screen.
///
/// It carries the account, so a handler cannot read or write anyone's data
/// without first saying whose it is: `auth.user_id` is the only way in.
pub struct Auth {
    pub identity: Identity,
    /// The raw token of this request, for "sign out everywhere but here".
    pub token: String,
}

impl Auth {
    pub fn user_id(&self) -> i64 {
        self.identity.user_id
    }

    pub fn role(&self) -> Role {
        self.identity.role
    }

    pub fn is_admin(&self) -> bool {
        self.identity.role >= Role::Admin
    }

    /// Guard for anything that reaches another account's data or settings.
    pub fn require_admin(&self) -> AppResult<()> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "This needs an administrator account.".into(),
            ))
        }
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = token_from_headers(&parts.headers).ok_or(AppError::Unauthorised)?;
        let identity = session_user(&state.pool, &token)
            .await
            .ok_or(AppError::Unauthorised)?;
        Ok(Auth { identity, token })
    }
}

/// Same as `Auth`, but refuses anyone below administrator. Using it in the
/// route table means an admin-only endpoint cannot forget the check.
pub struct AdminAuth(pub Auth);

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = Auth::from_request_parts(parts, state).await?;
        auth.require_admin()?;
        Ok(AdminAuth(auth))
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
