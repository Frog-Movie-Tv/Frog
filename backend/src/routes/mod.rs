pub mod auth;
pub mod episodes;
pub mod images;
pub mod library;
pub mod meta;
pub mod search;
pub mod titles;

use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        // Public: enough for the shell to decide between setup, login and app.
        .route("/health", get(meta::health))
        .route("/auth/status", get(auth::status))
        .route("/auth/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        // Everything below needs a session.
        .route("/search", get(search::search))
        .route("/discover/trending", get(search::trending))
        .route("/library", get(library::list))
        .route("/library/stats", get(library::stats))
        .route("/person/{person_id}", get(titles::person))
        .route("/titles/{kind}/{tmdb_id}", get(titles::detail))
        .route(
            "/titles/{kind}/{tmdb_id}/seasons/{season}",
            get(titles::season),
        )
        .route("/titles/{kind}/{tmdb_id}/library", put(library::upsert))
        .route("/titles/{kind}/{tmdb_id}/library", delete(library::remove))
        .route(
            "/titles/{kind}/{tmdb_id}/episodes/{season}/{episode}/watch",
            post(episodes::set_watched),
        )
        .route(
            "/titles/{kind}/{tmdb_id}/episodes/{season}/{episode}/state",
            put(episodes::set_state),
        )
        .route(
            "/titles/{kind}/{tmdb_id}/episodes/{season}/watch",
            post(episodes::set_season_watched),
        );

    Router::new()
        .nest("/api", api)
        .route("/img/{size}/{*path}", get(images::proxy))
        .fallback_service(spa(&state))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Serve the built SvelteKit app, falling back to index.html so client-side
/// routes survive a hard refresh.
fn spa(state: &AppState) -> ServeDir<ServeFile> {
    let dir = &state.config.static_dir;
    ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))
}
