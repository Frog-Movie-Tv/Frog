//! Profiles: what another account on this instance is watching, for people who
//! chose to show it. Off by default, never public to the internet (every route
//! here needs a session), and a private profile answers exactly like a missing
//! one, so looking does not reveal who has an account.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{NaiveDate, Utc};
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::routes::stats::streaks;
use crate::state::AppState;

/// (username, display name, me, has a picture)
type ListRow = (String, Option<String>, bool, bool);

/// Accounts whose profile is on, plus your own.
pub async fn list(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let rows: Vec<ListRow> = sqlx::query_as(
        "SELECT u.username, u.display_name, u.id = ?,
                EXISTS (SELECT 1 FROM user_image i WHERE i.user_id = u.id AND i.kind = 'avatar')
           FROM app_user u
          WHERE u.disabled = 0 AND (u.profile_public = 1 OR u.id = ?)
          ORDER BY u.username COLLATE NOCASE",
    )
    .bind(auth.user_id())
    .bind(auth.user_id())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "profiles": rows.into_iter().map(|(username, display_name, me, avatar)| json!({
            "username": username, "display_name": display_name, "me": me, "avatar": avatar,
        })).collect::<Vec<_>>()
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

/// (id, username, created_at, profile_public, display name, bio, banner backdrop, pins)
type UserRow = (
    i64,
    String,
    String,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

/// Find an account whose profile this viewer may see.
async fn visible(state: &AppState, auth: &Auth, username: &str) -> AppResult<UserRow> {
    let found: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, created_at, profile_public, display_name, bio, banner_backdrop,
                pins_json
           FROM app_user WHERE username = ? COLLATE NOCASE AND disabled = 0",
    )
    .bind(username)
    .fetch_optional(&state.pool)
    .await?;
    match found {
        Some(row) if row.3 || row.0 == auth.user_id() => Ok(row),
        _ => Err(AppError::NotFound("no such profile".into())),
    }
}

/// (films, episodes, shows, minutes)
type TotalsRow = (i64, i64, i64, f64);

/// (in library, completed, favourites, mean rating)
type LibraryRow = (i64, i64, i64, Option<f64>);

pub async fn get(
    auth: Auth,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> AppResult<Json<Value>> {
    let (user_id, username, joined, public, display_name, bio, banner_backdrop, pins_json) =
        visible(&state, &auth, &username).await?;
    let pool = &state.pool;
    let (banner_position,): (Option<String>,) =
        sqlx::query_as("SELECT banner_position FROM app_user WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

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
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = w.user_id
          WHERE w.user_id = ? AND w.undated = 0
          GROUP BY t.id
          ORDER BY MAX(w.watched_at) DESC LIMIT 18",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    // Pinned titles, in the order chosen, skipping any no longer tracked.
    let pins: Vec<(String, i64)> = serde_json::from_str::<Vec<Value>>(&pins_json)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| Some((p["kind"].as_str()?.to_string(), p["tmdb_id"].as_i64()?)))
        .collect();
    let mut pinned = Vec::new();
    for (kind, tmdb_id) in pins {
        let row: Option<CardRow> = sqlx::query_as(
            "SELECT t.kind, t.tmdb_id, t.name, t.poster_path, l.rating
               FROM library_entry l JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ? AND t.kind = ? AND t.tmdb_id = ?",
        )
        .bind(user_id)
        .bind(&kind)
        .bind(tmdb_id)
        .fetch_optional(pool)
        .await?;
        pinned.extend(row);
    }

    // The same figures as the stats page: watches of titles in the library.
    let (films, episodes, shows, minutes): TotalsRow = sqlx::query_as(
        "SELECT COALESCE(SUM(w.episode_id IS NULL), 0),
                COALESCE(SUM(w.episode_id IS NOT NULL), 0),
                COUNT(DISTINCT CASE WHEN w.episode_id IS NOT NULL THEN w.title_id END),
                CAST(COALESCE(SUM(COALESCE(e.runtime, t.runtime, 0)), 0) AS REAL)
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let (in_library, completed, favourite_count, mean_rating): LibraryRow = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(status = 'completed'), 0), COALESCE(SUM(favourite), 0),
                AVG(rating)
           FROM library_entry WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let genres: Vec<(String, i64)> = sqlx::query_as(
        "SELECT g.value, COUNT(*) FROM library_entry l
           JOIN title t ON t.id = l.title_id, json_each(t.genres_json) g
          WHERE l.user_id = ? AND l.status IN ('watching', 'completed')
          GROUP BY g.value ORDER BY 2 DESC, 1 LIMIT 5",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let days: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT substr(watched_at, 1, 10) FROM watch_event
          WHERE user_id = ? AND undated = 0 ORDER BY 1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let days: Vec<NaiveDate> = days
        .into_iter()
        .filter_map(|(d,)| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .collect();
    let (longest, current) = streaks(&days, Utc::now().date_naive());

    let images: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, updated_at FROM user_image WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    let image_url = |kind: &str| {
        images.iter().find(|(k, _)| k == kind).map(|(_, at)| {
            // The time in the address makes a new picture show at once.
            format!(
                "/api/profiles/{}/images/{kind}?v={}",
                urlencoding::encode(&username),
                urlencoding::encode(at)
            )
        })
    };

    Ok(Json(json!({
        "username": username,
        "display_name": display_name,
        "bio": bio,
        "joined": joined,
        "public": public,
        "me": user_id == auth.user_id(),
        "avatar": image_url("avatar"),
        "banner": image_url("banner"),
        "banner_backdrop": banner_backdrop,
        "banner_position": banner_position,
        "stats": {
            "films": films,
            "episodes": episodes,
            "shows": shows,
            "minutes": minutes.round() as i64,
            "library": in_library,
            "completed": completed,
            "favourites": favourite_count,
            "mean_rating": mean_rating,
            "streak": current.days,
            "longest_streak": longest.days,
        },
        "genres": genres.into_iter().map(|(name, titles)| json!({ "name": name, "titles": titles })).collect::<Vec<_>>(),
        "pinned": cards(pinned),
        "watching": cards(watching),
        "favourites": cards(favourites),
        "recent": cards(recent),
    })))
}

/// A profile picture or banner, to whoever may see the profile.
pub async fn image(
    auth: Auth,
    State(state): State<AppState>,
    Path((username, kind)): Path<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let user = visible(&state, &auth, &username).await?;
    let row: Option<(String, Vec<u8>, String)> = sqlx::query_as(
        "SELECT mime, bytes, updated_at FROM user_image WHERE user_id = ? AND kind = ?",
    )
    .bind(user.0)
    .bind(&kind)
    .fetch_optional(&state.pool)
    .await?;
    let Some((mime, bytes, updated_at)) = row else {
        return Err(AppError::NotFound("no picture".into()));
    };
    let etag = format!("\"{}\"", updated_at.replace(['"', ' '], ""));
    let mut response = if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        bytes.into_response()
    };
    let h = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    // Private: these sit behind a sign-in and must never land in a shared cache.
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    Ok(response)
}
