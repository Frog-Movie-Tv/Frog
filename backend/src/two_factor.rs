//! Two-factor sign-in: an authenticator's secret per account, and its
//! recovery codes, stored as hashes. Turning it on and off is
//! `routes::two_factor`; checking a code at sign-in is `second_factor_ok`.

use chrono::Utc;
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
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

/// Whether two-factor sign-in is on, and how many recovery codes are unused.
pub async fn status(pool: &SqlitePool, user_id: i64) -> AppResult<(bool, i64)> {
    let (enabled,): (bool,) =
        sqlx::query_as("SELECT totp_secret IS NOT NULL FROM app_user WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    let (left,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM recovery_code WHERE user_id = ? AND used_at IS NULL")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok((enabled, left))
}

/// A new secret to scan, kept aside until a code from it is confirmed.
/// Returns the account's name, for the authenticator to show, and the secret.
pub async fn begin(pool: &SqlitePool, user_id: i64) -> AppResult<(String, String)> {
    let (username, enabled): (String, bool) =
        sqlx::query_as("SELECT username, totp_secret IS NOT NULL FROM app_user WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    if enabled {
        return Err(AppError::Conflict(
            "Two-factor sign-in is already on.".into(),
        ));
    }
    let secret = totp::generate_secret();
    sqlx::query("UPDATE app_user SET totp_pending = ? WHERE id = ?")
        .bind(&secret)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok((username, secret))
}

/// Replace the account's recovery codes, inside a transaction the caller holds.
async fn write_codes(
    tx: &mut sqlx::SqliteConnection,
    user_id: i64,
    codes: &[String],
) -> AppResult<()> {
    sqlx::query("DELETE FROM recovery_code WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for code in codes {
        sqlx::query("INSERT INTO recovery_code (user_id, code_hash) VALUES (?, ?)")
            .bind(user_id)
            .bind(hash_recovery(code))
            .execute(&mut *tx)
            .await?;
    }
    Ok(())
}

/// Turn it on with a code from the pending secret. Returns the recovery codes,
/// which are shown once.
pub async fn confirm(pool: &SqlitePool, user_id: i64, code: &str) -> AppResult<Vec<String>> {
    let (pending,): (Option<String>,) =
        sqlx::query_as("SELECT totp_pending FROM app_user WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    let Some(secret) = pending else {
        return Err(AppError::BadRequest("Start setup again.".into()));
    };
    let Some(step) = totp::verify(&secret, code, Utc::now().timestamp()) else {
        return Err(AppError::BadRequest(
            "That code didn't work. Check the time on your phone and try the next one.".into(),
        ));
    };

    let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE app_user SET totp_secret = totp_pending, totp_pending = NULL, totp_last_step = ?
          WHERE id = ?",
    )
    .bind(step)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    write_codes(&mut tx, user_id, &codes).await?;
    tx.commit().await?;
    Ok(codes)
}

pub async fn disable(pool: &SqlitePool, user_id: i64) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE app_user SET totp_secret = NULL, totp_pending = NULL, totp_last_step = 0
          WHERE id = ?",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM recovery_code WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// A fresh set of recovery codes; the old ones stop working.
pub async fn new_codes(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<String>> {
    let (enabled, _) = status(pool, user_id).await?;
    if !enabled {
        return Err(AppError::BadRequest(
            "Turn on two-factor sign-in first.".into(),
        ));
    }
    let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
    let mut tx = pool.begin().await?;
    write_codes(&mut tx, user_id, &codes).await?;
    tx.commit().await?;
    Ok(codes)
}

/// Checks an authenticator code, or else spends one of this account's
/// recovery codes.
pub async fn second_factor_ok(
    pool: &SqlitePool,
    user_id: i64,
    secret: &str,
    code: &str,
) -> AppResult<bool> {
    if let Some(step) = totp::verify(secret, code, Utc::now().timestamp()) {
        // Only a step later than the last accepted one, so a code seen over
        // someone's shoulder cannot be used again.
        let used = sqlx::query(
            "UPDATE app_user SET totp_last_step = ? WHERE id = ? AND totp_last_step < ?",
        )
        .bind(step)
        .bind(user_id)
        .bind(step)
        .execute(pool)
        .await?;
        return Ok(used.rows_affected() == 1);
    }

    let spent = sqlx::query(
        "UPDATE recovery_code SET used_at = ?
          WHERE user_id = ? AND code_hash = ? AND used_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(user_id)
    .bind(hash_recovery(code))
    .execute(pool)
    .await?;
    Ok(spent.rows_affected() == 1)
}
