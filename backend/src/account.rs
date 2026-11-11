//! One account's own settings, as Settings changes them: its name, password,
//! devices, profile and pictures. What may be written is `routes::account`'s
//! check; this reads and writes, always for the one account named.

use chrono::Utc;
use sqlx::SqlitePool;

use crate::error::AppResult;

/// (username, role, created_at, profile_public, has two-factor,
///  display name, bio, banner backdrop, pins)
pub type AccountRow = (
    String,
    String,
    String,
    bool,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

/// The account, the pictures it has uploaded, and where its banner sits.
pub async fn details(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<(AccountRow, Vec<String>, Option<String>)> {
    let row: AccountRow = sqlx::query_as(
        "SELECT username, role, created_at, profile_public, totp_secret IS NOT NULL,
                display_name, bio, banner_backdrop, pins_json
           FROM app_user WHERE id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let images: Vec<(String,)> = sqlx::query_as("SELECT kind FROM user_image WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    let (banner_position,): (Option<String>,) =
        sqlx::query_as("SELECT banner_position FROM app_user WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok((
        row,
        images.into_iter().map(|(k,)| k).collect(),
        banner_position,
    ))
}

pub async fn set_password_hash(pool: &SqlitePool, user_id: i64, hash: &str) -> AppResult<()> {
    sqlx::query("UPDATE app_user SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_username(pool: &SqlitePool, user_id: i64, name: &str) -> AppResult<()> {
    sqlx::query("UPDATE app_user SET username = ? WHERE id = ?")
        .bind(name)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// (token hash, created_at, expires_at, user agent)
pub type SessionRow = (String, String, String, Option<String>);

/// The account's devices still signed in, newest first.
pub async fn sessions(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<SessionRow>> {
    Ok(sqlx::query_as(
        "SELECT token_hash, created_at, expires_at, user_agent
           FROM session WHERE user_id = ? AND expires_at > ?
          ORDER BY created_at DESC",
    )
    .bind(user_id)
    .bind(Utc::now().to_rfc3339())
    .fetch_all(pool)
    .await?)
}

/// Sign out the device whose token hash starts so. Says how many went.
pub async fn end_session(pool: &SqlitePool, user_id: i64, prefix: &str) -> AppResult<u64> {
    Ok(
        sqlx::query("DELETE FROM session WHERE user_id = ? AND substr(token_hash, 1, ?) = ?")
            .bind(user_id)
            .bind(prefix.len() as i64)
            .bind(prefix)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

/// One profile field, by its column. Only the columns named here.
pub enum ProfileField<'a> {
    Public(bool),
    DisplayName(Option<&'a str>),
    Bio(Option<&'a str>),
    BannerPosition(Option<&'a str>),
    Pins(&'a str),
}

pub async fn set_profile_field(
    pool: &SqlitePool,
    user_id: i64,
    field: ProfileField<'_>,
) -> AppResult<()> {
    let query = match field {
        ProfileField::Public(public) => {
            sqlx::query("UPDATE app_user SET profile_public = ? WHERE id = ?").bind(public)
        }
        ProfileField::DisplayName(name) => {
            sqlx::query("UPDATE app_user SET display_name = ? WHERE id = ?").bind(name)
        }
        ProfileField::Bio(bio) => sqlx::query("UPDATE app_user SET bio = ? WHERE id = ?").bind(bio),
        ProfileField::BannerPosition(position) => {
            sqlx::query("UPDATE app_user SET banner_position = ? WHERE id = ?").bind(position)
        }
        ProfileField::Pins(pins) => {
            sqlx::query("UPDATE app_user SET pins_json = ? WHERE id = ?").bind(pins)
        }
    };
    query.bind(user_id).execute(pool).await?;
    Ok(())
}

/// A TMDB backdrop as the banner. One banner at a time: choosing a backdrop
/// drops a banner picture uploaded, which would otherwise go on being drawn
/// over it.
pub async fn set_banner_backdrop(
    pool: &SqlitePool,
    user_id: i64,
    path: Option<&str>,
) -> AppResult<()> {
    sqlx::query("UPDATE app_user SET banner_backdrop = ? WHERE id = ?")
        .bind(path)
        .bind(user_id)
        .execute(pool)
        .await?;
    if path.is_some() {
        sqlx::query("DELETE FROM user_image WHERE user_id = ? AND kind = 'banner'")
            .bind(user_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Keep an uploaded picture. And the other way round from the backdrop: a
/// banner uploaded replaces a backdrop chosen.
pub async fn put_image(
    pool: &SqlitePool,
    user_id: i64,
    kind: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO user_image (user_id, kind, mime, bytes, updated_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (user_id, kind) DO UPDATE SET
            mime = excluded.mime, bytes = excluded.bytes, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(kind)
    .bind(mime)
    .bind(bytes)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    if kind == "banner" {
        sqlx::query("UPDATE app_user SET banner_backdrop = NULL WHERE id = ?")
            .bind(user_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn remove_image(pool: &SqlitePool, user_id: i64, kind: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM user_image WHERE user_id = ? AND kind = ?")
        .bind(user_id)
        .bind(kind)
        .execute(pool)
        .await?;
    Ok(())
}
