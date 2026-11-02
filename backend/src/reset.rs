//! Password reset links. Nobody can sign back in after forgetting a password,
//! so an administrator makes a link, the same way an invitation works: it works
//! once, it expires, and only the SHA-256 of its code is stored.
//!
//! A link changes the password and signs out every device. It leaves two-factor
//! as it is: a second factor exists to survive a lost password, and a link
//! handed around on a chat app is not a reason to drop it.

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use crate::auth::{self, Role};
use crate::error::AppResult;

/// Long enough to pass a link on and have it read, short enough that one left
/// in a chat does not stay a way in.
const LIFETIME_HOURS: i64 = 24;

/// Make a link for one account, replacing any it already has. The code comes
/// back once and is never retrievable again.
pub async fn issue(
    pool: &SqlitePool,
    user_id: i64,
    created_by: i64,
) -> AppResult<(String, DateTime<Utc>)> {
    let code = auth::new_code();
    let now = Utc::now();
    let expires = now + Duration::hours(LIFETIME_HOURS);

    withdraw(pool, user_id).await?;
    sqlx::query(
        "INSERT INTO password_reset (code_hash, user_id, created_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(auth::hash_code(&code))
    .bind(user_id)
    .bind(created_by)
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .execute(pool)
    .await?;
    Ok((code, expires))
}

/// A link for the owner, for the one case no administrator can help with. Only
/// reachable from a shell on the server, which is what proves it is the owner.
pub async fn issue_for_owner(pool: &SqlitePool) -> AppResult<Option<(String, DateTime<Utc>)>> {
    let owner: Option<(i64,)> = sqlx::query_as("SELECT id FROM app_user WHERE role = ?")
        .bind(Role::Owner.as_db())
        .fetch_optional(pool)
        .await?;
    match owner {
        Some((id,)) => Ok(Some(issue(pool, id, id).await?)),
        None => Ok(None),
    }
}

/// Drop any link an account still has open: its password or role changed since
/// the link was made, so it no longer says what it did.
pub async fn withdraw(pool: &SqlitePool, user_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM password_reset WHERE user_id = ? AND used_at IS NULL")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Spend a code on a new password, signing out every device of that account.
/// `None` for a code that is unknown, used or expired, or whose account has been
/// switched off, which the caller answers alike. Otherwise the account's name.
///
/// The password is hashed first, so a bad code costs what a good one does.
/// Marking the code used is the statement that checks it, so two people using
/// one link at the same moment cannot both change the password.
pub async fn redeem(
    pool: &SqlitePool,
    code: &str,
    new_password: &str,
) -> AppResult<Option<String>> {
    let password_hash = auth::hash_password(new_password)?;
    let code_hash = auth::hash_code(code);
    let now = Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;
    let claimed = sqlx::query(
        "UPDATE password_reset SET used_at = ?
          WHERE code_hash = ? AND used_at IS NULL AND expires_at > ?
            AND user_id IN (SELECT id FROM app_user WHERE disabled = 0)",
    )
    .bind(&now)
    .bind(&code_hash)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    if claimed.rows_affected() != 1 {
        return Ok(None);
    }

    let (user_id,): (i64,) =
        sqlx::query_as("SELECT user_id FROM password_reset WHERE code_hash = ?")
            .bind(&code_hash)
            .fetch_one(&mut *tx)
            .await?;
    let (username,): (String,) = sqlx::query_as("SELECT username FROM app_user WHERE id = ?")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE app_user SET password_hash = ? WHERE id = ?")
        .bind(&password_hash)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM session WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Some(username))
}

/// Links past their expiry, used or not, are of no use to anyone.
pub async fn purge_expired(pool: &SqlitePool) -> AppResult<u64> {
    let gone = sqlx::query("DELETE FROM password_reset WHERE expires_at < ?")
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(gone.rows_affected())
}
