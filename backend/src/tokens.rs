//! Personal tokens, for the browser extension.
//!
//! A token is made in Settings → Connections, shown once, and stored as its
//! SHA-256 like a session. It is good for `/api/ext` and nothing else: only
//! `TokenAuth` reads one, and `Auth` refuses a request that carries one
//! instead of a session, so a token can never change settings, the account,
//! two-factor, invites or anything an administrator does.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::Utc;
use rand::RngCore;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::auth::{self, Identity, Role};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::throttle;

/// Every token starts with this, so one pasted somewhere it should not be is
/// easy to recognise.
pub const PREFIX: &str = "frog_";

/// More than anyone needs: one per browser, with room to replace them.
pub const MAX_PER_ACCOUNT: i64 = 20;
pub const NAME_MAX: usize = 40;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Token {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

fn new_raw() -> String {
    let mut raw = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    format!("{PREFIX}{}", hex::encode(raw))
}

/// A token's name, trimmed, or why it cannot be one.
pub fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Give the token a name.".into()));
    }
    if name.chars().count() > NAME_MAX {
        return Err(AppError::BadRequest(format!(
            "A name can be at most {NAME_MAX} characters."
        )));
    }
    Ok(name.to_string())
}

/// Make a token and hand back the only copy of it that will ever exist.
pub async fn create(pool: &SqlitePool, user_id: i64, name: &str) -> AppResult<(Token, String)> {
    let name = clean_name(name)?;
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_token WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    if count >= MAX_PER_ACCOUNT {
        return Err(AppError::BadRequest(
            "That is as many tokens as an account can have. Revoke one first.".into(),
        ));
    }
    let raw = new_raw();
    let token: Token = sqlx::query_as(
        "INSERT INTO api_token (user_id, name, token_hash, created_at)
         VALUES (?, ?, ?, ?)
         RETURNING id, name, created_at, last_used_at",
    )
    .bind(user_id)
    .bind(&name)
    .bind(auth::token_fingerprint(&raw))
    .bind(Utc::now().to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok((token, raw))
}

pub async fn list(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Token>> {
    Ok(sqlx::query_as(
        "SELECT id, name, created_at, last_used_at FROM api_token
          WHERE user_id = ? ORDER BY created_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

/// True when the account had that token.
pub async fn revoke(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<bool> {
    let done = sqlx::query("DELETE FROM api_token WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// (token id, user id, username, role, disabled)
type TokenRow = (i64, i64, String, String, i64);

/// The account a raw token belongs to, and marks it used. None for an unknown
/// token, and for a switched-off account, which has to stop at once.
pub async fn identify(pool: &SqlitePool, raw: &str) -> AppResult<Option<(i64, Identity)>> {
    let row: Option<TokenRow> = sqlx::query_as(
        "SELECT t.id, u.id, u.username, u.role, u.disabled
           FROM api_token t JOIN app_user u ON u.id = t.user_id
          WHERE t.token_hash = ?",
    )
    .bind(auth::token_fingerprint(raw.trim()))
    .fetch_optional(pool)
    .await?;
    let Some((token_id, user_id, username, role, disabled)) = row else {
        return Ok(None);
    };
    if disabled != 0 {
        return Ok(None);
    }
    sqlx::query("UPDATE api_token SET last_used_at = ? WHERE id = ? AND user_id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(token_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(Some((
        token_id,
        Identity {
            user_id,
            username,
            role: Role::from_db(&role),
        },
    )))
}

/// The extractor for `/api/ext`: `Authorization: Bearer <token>`, nothing
/// else. A session cookie does not get in here, and a token gets in nowhere
/// else. Wrong tokens are counted per caller address.
pub struct TokenAuth {
    pub identity: Identity,
    pub token_id: i64,
}

impl TokenAuth {
    pub fn user_id(&self) -> i64 {
        self.identity.user_id
    }
}

impl FromRequestParts<AppState> for TokenAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = auth::bearer_token(&parts.headers).ok_or(AppError::Unauthorised)?;
        let peer = parts
            .extensions
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0);
        let keys = throttle::token_keys(&auth::client_key(&parts.headers, peer));
        state
            .token_failures
            .admit(&keys)
            .map_err(auth::too_many_tries)?;
        match identify(&state.pool, &raw).await? {
            Some((token_id, identity)) => {
                state.token_failures.release(&keys);
                Ok(TokenAuth { identity, token_id })
            }
            None => Err(AppError::Unauthorised),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_long_random_and_recognisable() {
        let (a, b) = (new_raw(), new_raw());
        assert_ne!(a, b);
        assert!(a.starts_with(PREFIX));
        assert_eq!(a.len(), PREFIX.len() + 64);
    }

    #[test]
    fn a_name_is_needed_and_kept_short() {
        assert!(clean_name("  ").is_err());
        assert_eq!(clean_name("  Brave ").unwrap(), "Brave");
        assert!(clean_name(&"x".repeat(NAME_MAX)).is_ok());
        assert!(clean_name(&"x".repeat(NAME_MAX + 1)).is_err());
    }
}
