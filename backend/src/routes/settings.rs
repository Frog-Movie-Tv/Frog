use axum::extract::State;
use axum::Json;
use chrono::Utc;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const KEY: &str = "preferences";

/// Preferences are stored as one opaque JSON blob.
///
/// The server has no opinion about what is inside it: the client owns the
/// shape, and adding a new option never needs a migration. The only rules are
/// that it must be a JSON object and must stay small enough that it is clearly
/// a settings document rather than someone using the instance as a database.
const MAX_BYTES: usize = 128 * 1024;

pub async fn get(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = ?")
            .bind(auth.user_id())
            .bind(KEY)
            .fetch_optional(&state.pool)
            .await?;

    let stored = row
        .and_then(|(v,)| serde_json::from_str::<Value>(&v).ok())
        .unwrap_or_else(|| json!({}));

    Ok(Json(stored))
}

pub async fn put(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    if !body.is_object() {
        return Err(AppError::BadRequest("preferences must be an object".into()));
    }

    let encoded = body.to_string();
    if encoded.len() > MAX_BYTES {
        return Err(AppError::BadRequest(
            "preferences are too large; trim custom CSS or saved themes".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO app_setting (user_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at",
    )
    .bind(auth.user_id())
    .bind(KEY)
    .bind(&encoded)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;

    Ok(Json(body))
}
