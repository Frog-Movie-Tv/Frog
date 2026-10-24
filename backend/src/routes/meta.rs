use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&state.pool).await.is_ok();

    Json(json!({
        "ok": db_ok,
        "version": env!("CARGO_PKG_VERSION"),
        "tmdb_configured": state.tmdb.is_configured(),
        "region": state.config.region,
    }))
}
