//! What Administration reads and changes: every account, and how big the
//! instance is. Who may do what to whom is `routes::admin`'s check.

use sqlx::SqlitePool;

use crate::auth::Role;
use crate::error::AppResult;

/// (id, username, role, disabled, created_at, titles tracked, last sign-in)
pub type UserRow = (i64, String, String, bool, String, i64, Option<String>);

/// Every account, the owner first, then administrators, then by name.
pub async fn users(pool: &SqlitePool) -> AppResult<Vec<UserRow>> {
    Ok(sqlx::query_as(
        "SELECT u.id, u.username, u.role, u.disabled, u.created_at,
                (SELECT COUNT(*) FROM library_entry l WHERE l.user_id = u.id),
                u.last_sign_in_at
           FROM app_user u
          ORDER BY CASE u.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
                   u.username COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?)
}

/// An account's role and whether it is switched off, when it exists.
pub async fn standing(pool: &SqlitePool, id: i64) -> AppResult<Option<(Role, bool)>> {
    let row: Option<(String, bool)> =
        sqlx::query_as("SELECT role, disabled FROM app_user WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(role, disabled)| (Role::from_db(&role), disabled)))
}

pub async fn set_role(pool: &SqlitePool, id: i64, role: Role) -> AppResult<()> {
    sqlx::query("UPDATE app_user SET role = ? WHERE id = ?")
        .bind(role.as_db())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_disabled(pool: &SqlitePool, id: i64, disabled: bool) -> AppResult<()> {
    sqlx::query("UPDATE app_user SET disabled = ? WHERE id = ?")
        .bind(disabled)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove an account; everything it owns goes with it (the keys cascade).
pub async fn delete_account(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM app_user WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// How big the instance is.
pub struct Figures {
    pub database_bytes: i64,
    pub image_cache_bytes: i64,
    pub accounts: i64,
    /// Titles this Frog knows, by its own id: all, films, shows, manga. One
    /// folded into another counts once.
    pub titles: (i64, i64, i64, i64),
}

pub async fn figures(pool: &SqlitePool) -> AppResult<Figures> {
    let (pages, page_size): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT page_count FROM pragma_page_count()), (SELECT page_size FROM pragma_page_size())",
    )
    .fetch_one(pool)
    .await?;
    let (images,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(length(bytes)), 0) FROM image_cache")
            .fetch_one(pool)
            .await?;
    let titles: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(kind = 'movie'), 0), COALESCE(SUM(kind = 'tv'), 0),
                COALESCE(SUM(kind = 'manga'), 0)
           FROM title WHERE merged_into IS NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(Figures {
        database_bytes: pages * page_size,
        image_cache_bytes: images,
        accounts: crate::auth::account_count(pool).await?,
        titles,
    })
}
