pub mod auth;
pub mod episodes;
pub mod home;
pub mod images;
pub mod library;
pub mod meta;
pub mod search;
pub mod settings;
pub mod timeline;
pub mod titles;
pub mod transfer;
pub mod two_factor;

use axum::http::{header, HeaderValue};
use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
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
        .route("/auth/two-factor", get(two_factor::status))
        .route("/auth/two-factor/start", post(two_factor::start))
        .route("/auth/two-factor/confirm", post(two_factor::confirm))
        .route("/auth/two-factor/disable", post(two_factor::disable))
        .route(
            "/auth/two-factor/recovery-codes",
            post(two_factor::new_recovery_codes),
        )
        // Everything below needs a session.
        .route("/search", get(search::search))
        .route("/discover/trending", get(search::trending))
        .route("/home", get(home::home))
        .route("/calendar", get(timeline::calendar))
        .route("/diary", get(timeline::diary))
        .route("/library", get(library::list))
        .route("/library/stats", get(library::stats))
        .route("/settings", get(settings::get))
        .route("/settings", put(settings::put))
        .route("/export/library.json", get(transfer::export_json))
        .route("/export/library.csv", get(transfer::export_csv))
        .route("/import/match", post(transfer::match_titles))
        .route("/import/apply", post(transfer::apply))
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
        )
        .route(
            "/titles/{kind}/{tmdb_id}/episodes/watch",
            post(episodes::set_show_watched),
        )
        .route("/diary/redate", post(timeline::redate));

    Router::new()
        .nest("/api", api)
        .route("/img/{size}/{*path}", get(images::proxy))
        .route("/robots.txt", get(robots))
        .fallback_service(spa(&state))
        // This is one person's private watch history. Even behind Tailscale,
        // say so to anything that crawls.
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow, noarchive, noimageindex"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn robots() -> ([(header::HeaderName, HeaderValue); 1], &'static str) {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        "User-agent: *
Disallow: /
",
    )
}

/// Serve the built SvelteKit app, falling back to index.html so client-side
/// routes survive a hard refresh.
fn spa(state: &AppState) -> ServeDir<ServeFile> {
    let dir = &state.config.static_dir;
    ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))
}
