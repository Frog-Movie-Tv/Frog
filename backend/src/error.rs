use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthorised")]
    Unauthorised,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("TMDB is not configured: set TMDB_API_KEY")]
    TmdbUnconfigured,
    #[error("TMDB request failed: {0}")]
    Tmdb(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::Unauthorised => StatusCode::UNAUTHORIZED,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::TmdbUnconfigured => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Tmdb(_) => StatusCode::BAD_GATEWAY,
            AppError::Db(sqlx::Error::RowNotFound) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Internal faults go to the log, never to the screen: a SQL type
        // error means nothing to the person using the app and reads like a
        // crash. Everything the user can actually act on keeps its message.
        let message = match &self {
            AppError::Db(_) | AppError::Other(_) => {
                tracing::error!(error = ?self, "request failed");
                "Something went wrong inside Frog. The details are in the server log.".to_string()
            }
            AppError::Tmdb(detail) => {
                tracing::error!(%detail, "TMDB request failed");
                "Could not reach TMDB. It may be down, or the API key may be wrong.".to_string()
            }
            other => other.to_string(),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
