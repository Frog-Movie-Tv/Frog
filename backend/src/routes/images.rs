//! Pictures through Frog's own origin. The cache and its budget are
//! `crate::images`.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;

use crate::auth::Auth;
use crate::error::AppResult;
use crate::state::AppState;

pub async fn proxy(
    _: Auth,
    State(state): State<AppState>,
    Path((size, path)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let (content_type, bytes) = crate::images::cached_image(&state, &size, &path).await?;

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
