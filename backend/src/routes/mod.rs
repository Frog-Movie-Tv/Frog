pub mod account;
pub mod admin;
pub mod anilist;
pub mod auth;
pub mod episodes;
pub mod home;
pub mod images;
pub mod invites;
pub mod library;
pub mod meta;
pub mod profiles;
pub mod push;
pub mod search;
pub mod settings;
pub mod shelves;
pub mod stats;
pub mod timeline;
pub mod titles;
pub mod transfer;
pub mod two_factor;

use axum::extract::DefaultBodyLimit;
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
        .route("/auth/register", post(auth::register))
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
        .route("/home/because", get(home::because))
        .route("/calendar", get(timeline::calendar))
        .route("/diary", get(timeline::diary))
        .route("/library", get(library::list))
        .route("/library/stats", get(library::stats))
        .route("/stats", get(stats::stats))
        .route("/shelves", get(shelves::list).post(shelves::create))
        .route("/shelves/order", put(shelves::reorder))
        .route(
            "/shelves/{id}",
            get(shelves::get)
                .put(shelves::update)
                .delete(shelves::remove),
        )
        .route("/shelves/{id}/items", post(shelves::add_item))
        .route("/shelves/{id}/items/order", put(shelves::reorder_items))
        .route(
            "/shelves/{id}/items/{kind}/{tmdb_id}",
            delete(shelves::remove_item),
        )
        .route("/titles/{kind}/{tmdb_id}/shelves", get(shelves::for_title))
        .route("/push", get(push::status))
        .route("/push/subscribe", post(push::subscribe))
        .route("/push/unsubscribe", post(push::unsubscribe))
        .route("/push/test", post(push::test))
        .route("/settings", get(settings::get))
        .route("/settings", put(settings::put))
        .route("/account", get(account::get).delete(account::delete))
        .route("/account/password", put(account::change_password))
        .route("/account/profile", put(account::set_profile))
        .route(
            "/account/images/{kind}",
            put(account::set_image)
                .delete(account::remove_image)
                // Room for a picture; the handler refuses anything over 3 MB.
                .layer(DefaultBodyLimit::max(account::IMAGE_MAX_BYTES + 64 * 1024)),
        )
        .route("/account/sessions", get(account::sessions))
        .route(
            "/account/sessions/others",
            delete(account::end_other_sessions),
        )
        .route("/account/sessions/{id}", delete(account::end_session))
        .route("/admin/users", get(admin::users))
        .route(
            "/admin/users/{id}",
            put(admin::update_user).delete(admin::delete_user),
        )
        .route("/admin/instance", get(admin::instance))
        .route("/anilist", get(anilist::status).delete(anilist::disconnect))
        .route("/anilist/connect", post(anilist::connect))
        .route("/anilist/callback", get(anilist::callback))
        .route("/anilist/sync", post(anilist::sync_now))
        .route("/anilist/entries", get(anilist::entries))
        .route("/anilist/entries/{media_id}", put(anilist::choose))
        .route("/profiles", get(profiles::list))
        .route("/profiles/{username}", get(profiles::get))
        .route("/profiles/{username}/images/{kind}", get(profiles::image))
        .route("/invites", get(invites::list).post(invites::create))
        .route("/invites/{id}", delete(invites::revoke))
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
        .route("/healthz", get(meta::healthz))
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
