//! Profiles: what another account on this instance is watching, for people who
//! chose to show it. Off by default, never public to the internet (every route
//! here needs a session), and a private profile answers exactly like a missing
//! one, so looking does not reveal who has an account.

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Accounts whose profile is on, plus your own.
pub async fn list(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let rows: Vec<(String, bool)> = sqlx::query_as(
        "SELECT username, id = ? FROM app_user
          WHERE disabled = 0 AND (profile_public = 1 OR id = ?)
          ORDER BY username COLLATE NOCASE",
    )
    .bind(auth.user_id())
    .bind(auth.user_id())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "profiles": rows.into_iter().map(|(username, me)| json!({ "username": username, "me": me })).collect::<Vec<_>>()
    })))
}

/// (kind, tmdb_id, name, poster_path, rating)
type CardRow = (String, i64, String, Option<String>, Option<f64>);

fn cards(rows: Vec<CardRow>) -> Vec<Value> {
    rows.into_iter()
        .map(|(kind, tmdb_id, name, poster_path, rating)| {
            json!({ "kind": kind, "tmdb_id": tmdb_id, "name": name, "poster_path": poster_path, "rating": rating })
        })
        .collect()
}

pub async fn get(
    auth: Auth,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> AppResult<Json<Value>> {
    let found: Option<(i64, String, String, bool)> = sqlx::query_as(
        "SELECT id, username, created_at, profile_public FROM app_user
          WHERE username = ? COLLATE NOCASE AND disabled = 0",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await?;
    let (user_id, username, joined, public) = match found {
        Some(row) if row.3 || row.0 == auth.user_id() => row,
        _ => return Err(AppError::NotFound("no such profile".into())),
    };

    let pool = &state.pool;
    let watching: Vec<CardRow> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, t.name, t.poster_path, l.rating
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND l.status = 'watching'
          ORDER BY l.updated_at DESC LIMIT 18",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let favourites: Vec<CardRow> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, t.name, t.poster_path, l.rating
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND l.favourite = 1
          ORDER BY l.rating IS NULL, l.rating DESC, l.updated_at DESC LIMIT 18",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let recent: Vec<CardRow> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, t.name, t.poster_path, l.rating
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           LEFT JOIN library_entry l ON l.title_id = t.id AND l.user_id = w.user_id
          WHERE w.user_id = ? AND w.undated = 0
          GROUP BY t.id
          ORDER BY MAX(w.watched_at) DESC LIMIT 18",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let (films, episodes, minutes): (i64, i64, f64) = sqlx::query_as(
        "SELECT COALESCE(SUM(w.episode_id IS NULL), 0),
                COALESCE(SUM(w.episode_id IS NOT NULL), 0),
                CAST(COALESCE(SUM(COALESCE(e.runtime, t.runtime, 0)), 0) AS REAL)
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(Json(json!({
        "username": username,
        "joined": joined,
        "public": public,
        "me": user_id == auth.user_id(),
        "stats": { "films": films, "episodes": episodes, "minutes": minutes.round() as i64 },
        "watching": cards(watching),
        "favourites": cards(favourites),
        "recent": cards(recent),
    })))
}
