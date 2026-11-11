use axum::extract::State;
use axum::Json;
use serde_json::Value;

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::prefs;
use crate::state::AppState;

/// Preferences are stored as one opaque JSON blob.
///
/// The server has no opinion about what is inside it: the client owns the
/// shape, and adding a new option never needs a migration. The only rules are
/// that it must be a JSON object and must stay small enough that it is clearly
/// a settings document rather than someone using the instance as a database.
const MAX_BYTES: usize = 128 * 1024;

pub async fn get(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(prefs::get(&state.pool, auth.user_id()).await?))
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
    prefs::put(&state.pool, auth.user_id(), &encoded).await?;

    // A zone chosen in Settings is also the one notifications count midnight
    // in; left empty, the zone the browser reports stands.
    if let Some(zone) = body["timeZone"].as_str().filter(|z| !z.is_empty()) {
        crate::announce::set_time_zone(&state.pool, auth.user_id(), zone).await?;
    }

    Ok(Json(body))
}
