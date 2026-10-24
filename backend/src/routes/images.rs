use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use chrono::Utc;

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Sizes we are willing to proxy. A whitelist keeps the cache table bounded and
/// stops the endpoint becoming an open image relay.
const SIZES: [&str; 9] = [
    "w92", "w154", "w185", "w300", "w342", "w500", "w780", "w1280", "original",
];

/// Fetch through our own origin, with a SQLite-backed cache.
///
/// Going through the backend instead of hotlinking image.tmdb.org buys three
/// things: the service worker can cache posters for offline, the canvas can
/// read pixels without CORS, and a second viewing never hits TMDB at all.
pub async fn cached_image(
    state: &AppState,
    size: &str,
    path: &str,
) -> AppResult<(String, Vec<u8>)> {
    if !SIZES.contains(&size) {
        return Err(AppError::BadRequest(format!(
            "unsupported image size {size}"
        )));
    }
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if path.contains("..") {
        return Err(AppError::BadRequest("bad image path".into()));
    }

    let key = format!("{size}{path}");
    let hit: Option<(String, Vec<u8>)> =
        sqlx::query_as("SELECT content_type, bytes FROM image_cache WHERE path = ?")
            .bind(&key)
            .fetch_optional(&state.pool)
            .await?;

    if let Some(found) = hit {
        return Ok(found);
    }

    let (content_type, bytes) = state.tmdb.fetch_image(size, &path).await?;

    let _ = sqlx::query(
        "INSERT INTO image_cache (path, content_type, bytes, fetched_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (path) DO UPDATE SET
            content_type = excluded.content_type,
            bytes = excluded.bytes,
            fetched_at = excluded.fetched_at",
    )
    .bind(&key)
    .bind(&content_type)
    .bind(&bytes)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await;

    Ok((content_type, bytes))
}

pub async fn proxy(
    _: Auth,
    State(state): State<AppState>,
    Path((size, path)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let (content_type, bytes) = cached_image(&state, &size, &path).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
    );
    // TMDB image paths are content-addressed, so these never change.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );

    Ok((StatusCode::OK, headers, bytes))
}
