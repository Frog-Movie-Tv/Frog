//! Stats over HTTP; `crate::stats` counts.

use axum::extract::{Query, State};
use axum::Json;
use serde_json::Value;

use crate::auth::Auth;
use crate::error::AppResult;
use crate::state::AppState;
use crate::stats::{self, NarrowQuery, StatsQuery};

pub async fn stats(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> AppResult<Json<Value>> {
    stats::stats(&state, auth.user_id(), &q).await.map(Json)
}

/// The manga half of Stats.
pub async fn manga(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> AppResult<Json<Value>> {
    stats::manga(&state, auth.user_id(), &q).await.map(Json)
}

/// Days, streaks and people for the titles a pick keeps.
pub async fn narrow(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<NarrowQuery>,
) -> AppResult<Json<Value>> {
    stats::narrow(&state, auth.user_id(), &q).await.map(Json)
}
