use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

/// Liveness for the container healthcheck and for the deploy script, which
/// rolls back when this does not come back 200. Deliberately outside `/api`
/// and outside the auth wall: a load balancer cannot sign in, and it tells
/// an unauthenticated caller nothing but the version.
pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&state.pool).await.is_ok();
    let code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({
            "ok": db_ok,
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&state.pool).await.is_ok();

    Json(json!({
        "ok": db_ok,
        "version": env!("CARGO_PKG_VERSION"),
        "tmdb_configured": state.tmdb.is_configured(),
        "region": state.config.region,
    }))
}
