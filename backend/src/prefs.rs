//! Each account's preferences: one JSON object the app owns the shape of
//! (`app_setting` key `preferences`). The server stores it and never reads
//! inside it.

use chrono::Utc;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::AppResult;

const KEY: &str = "preferences";

/// The account's preferences, or an empty object when it has saved none.
pub async fn get(pool: &SqlitePool, user_id: i64) -> AppResult<Value> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = ?")
            .bind(user_id)
            .bind(KEY)
            .fetch_optional(pool)
            .await?;
    Ok(row
        .and_then(|(v,)| serde_json::from_str::<Value>(&v).ok())
        .unwrap_or_else(|| json!({})))
}

/// Keep the account's preferences as written, already checked by the caller.
pub async fn put(pool: &SqlitePool, user_id: i64, encoded: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_setting (user_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(KEY)
    .bind(encoded)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}
