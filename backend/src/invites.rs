//! Invite links: made by an administrator, spent once, gone after their
//! expiry. Only the SHA-256 of a code is stored, as with sessions and
//! recovery codes, so the database never holds a working invitation.

use chrono::Utc;
use sqlx::SqlitePool;

use crate::auth::{hash_code, Role};
use crate::error::{AppError, AppResult};

/// (code hash, role, created by, created at, expires at, used at, used by)
pub type InviteRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
);

/// Keep a new invite by the hash of its code.
pub async fn create(
    pool: &SqlitePool,
    code: &str,
    role: Role,
    created_by: i64,
    created_at: &str,
    expires_at: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO invite (code_hash, role, created_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash_code(code))
    .bind(role.as_db())
    .bind(created_by)
    .bind(created_at)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// The newest invites, with who made and who used each.
pub async fn list(pool: &SqlitePool) -> AppResult<Vec<InviteRow>> {
    Ok(sqlx::query_as(
        "SELECT i.code_hash, i.role, c.username, i.created_at, i.expires_at,
                i.used_at, u.username
           FROM invite i
           LEFT JOIN app_user c ON c.id = i.created_by
           LEFT JOIN app_user u ON u.id = i.used_by
          ORDER BY i.created_at DESC
          LIMIT 200",
    )
    .fetch_all(pool)
    .await?)
}

/// Withdraw an invite nobody has used. Says whether there was one.
pub async fn revoke(pool: &SqlitePool, id: &str) -> AppResult<bool> {
    let gone = sqlx::query("DELETE FROM invite WHERE code_hash = ? AND used_at IS NULL")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(gone.rows_affected() > 0)
}

/// Spend an invite, returning the role it grants.
///
/// Marking it used is the same statement that checks it, so two people
/// redeeming the same link at the same moment cannot both get in. If the
/// account then fails to be created, `release` puts it back.
pub async fn claim(pool: &SqlitePool, code: &str) -> AppResult<Role> {
    let hash = hash_code(code);
    let now = Utc::now().to_rfc3339();

    let taken = sqlx::query(
        "UPDATE invite SET used_at = ?
          WHERE code_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .bind(&hash)
    .bind(&now)
    .execute(pool)
    .await?;

    if taken.rows_affected() != 1 {
        return Err(AppError::Forbidden(
            "That invite link has been used already, or it has expired.".into(),
        ));
    }

    let (role,): (String,) = sqlx::query_as("SELECT role FROM invite WHERE code_hash = ?")
        .bind(&hash)
        .fetch_one(pool)
        .await?;
    Ok(Role::from_db(&role))
}

/// Put a claimed invite back, for when the account could not be created.
pub async fn release(pool: &SqlitePool, code: &str) {
    let _ = sqlx::query("UPDATE invite SET used_at = NULL WHERE code_hash = ? AND used_by IS NULL")
        .bind(hash_code(code))
        .execute(pool)
        .await;
}

/// Record who used it, once the account exists.
pub async fn mark_used(pool: &SqlitePool, code: &str, user_id: i64) -> AppResult<()> {
    sqlx::query("UPDATE invite SET used_by = ? WHERE code_hash = ?")
        .bind(user_id)
        .bind(hash_code(code))
        .execute(pool)
        .await?;
    Ok(())
}
