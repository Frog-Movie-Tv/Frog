//! The browser extension's side of Frog: personal tokens and `/api/ext`.
//!
//! No TMDB here: titles and episodes are written straight into the database,
//! so what is tested is Frog's own matching and logging.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use frog::config::Config;
use frog::state::AppState;

mod common;

struct Harness {
    state: AppState,
    _dir: tempfile::TempDir,
}

enum Who<'a> {
    Nobody,
    Session(&'a str),
    Token(&'a str),
}

impl Harness {
    async fn new() -> Self {
        Self::with_releases("http://127.0.0.1:9").await
    }

    /// Releases served from `releases`, for the Firefox build Settings installs.
    async fn with_releases(releases: &str) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let url = format!(
            "sqlite:{}?mode=rwc",
            dir.path()
                .join("test.db")
                .to_string_lossy()
                .replace('\\', "/")
        );
        let config = Config {
            tmdb_api_key: String::new(),
            database_url: url.clone(),
            bind: "127.0.0.1:0".parse().unwrap(),
            static_dir: dir.path().join("static"),
            region: "BE".into(),
            language: "en-US".into(),
            admin_username: None,
            admin_password: None,
            secure_cookies: false,
            session_days: 7,
            anilist: None,
            fanart: None,
            tvmaze_url: "http://127.0.0.1:9".into(),
            extension_releases: releases.into(),
            mdblist: None,
        };
        let pool = frog::db::connect(&url).await.expect("database");
        let state = AppState::new(pool, config);
        common::add_title_seed(&state).await;
        Self { state, _dir: dir }
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        who: Who<'_>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().uri(path).method(method);
        match who {
            Who::Nobody => {}
            Who::Session(cookie) => req = req.header(header::COOKIE, cookie),
            Who::Token(token) => req = req.header(header::AUTHORIZATION, format!("Bearer {token}")),
        }
        let body = match body {
            Some(b) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = frog::routes::router(self.state.clone())
            .oneshot(req.body(body).unwrap())
            .await
            .expect("response");
        let status = res.status();
        let bytes = res.into_body().collect().await.expect("body").to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// A plain GET, giving the status, the headers and the body as it is.
    async fn raw(
        &self,
        path: &str,
        cookie: Option<&str>,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let mut req = Request::builder().uri(path);
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        let res = frog::routes::router(self.state.clone())
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .expect("response");
        let (parts, body) = res.into_parts();
        let bytes = body.collect().await.expect("body").to_bytes().to_vec();
        (parts.status, parts.headers, bytes)
    }

    /// The owner, signed in. Gives the session cookie.
    async fn owner(&self) -> String {
        let res = frog::routes::router(self.state.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/auth/setup")
                    .method("POST")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "username": "mihai", "password": "a-long-enough-pass" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        cookie.split(';').next().unwrap().to_string()
    }

    /// A second account, through an invite as anyone joins.
    async fn second(&self, owner: &str) -> String {
        let (_, invite) = self
            .call(
                "POST",
                "/api/invites",
                Who::Session(owner),
                Some(json!({ "role": "user" })),
            )
            .await;
        let code = invite["code"].as_str().expect("an invite code").to_string();
        let res = frog::routes::router(self.state.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/auth/register")
                    .method("POST")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "username": "zjef", "password": "another-long-pass", "invite": code })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "the invite worked");
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        cookie.split(';').next().unwrap().to_string()
    }

    async fn token(&self, cookie: &str, name: &str) -> (i64, String) {
        let (status, body) = self
            .call(
                "POST",
                "/api/account/tokens",
                Who::Session(cookie),
                Some(json!({ "name": name })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        (
            body["token"]["id"].as_i64().unwrap(),
            body["secret"].as_str().unwrap().to_string(),
        )
    }

    async fn ext(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(
            "POST",
            &format!("/api/ext/{path}"),
            Who::Token(token),
            Some(body),
        )
        .await
    }

    async fn count(&self, sql: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
            .0
    }

    /// Severance, two seasons of three, and Arcane, both with TMDB ids; a
    /// film, Heat. Frog's ids are 1, 2 and 3.
    async fn catalogue(&self) {
        sqlx::query(
            "INSERT INTO title_seed (id, kind, tmdb_id, name, release_date, tmdb_status) VALUES
                (1, 'tv', 95396, 'Severance', '2022-02-18', 'Returning Series'),
                (2, 'tv', 94605, 'Arcane', '2021-11-06', 'Ended'),
                (3, 'movie', 949, 'Heat', '1995-12-15', 'Released')",
        )
        .execute(&self.state.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO episode (title_id, season_number, episode_number, name, air_date) VALUES
                (1, 1, 1, 'Good News About Hell', '2022-02-18'),
                (1, 1, 2, 'Half Loop', '2022-02-18'),
                (1, 1, 3, 'In Perpetuity', '2022-02-25'),
                (1, 2, 1, 'Hello, Ms. Cobel', '2025-01-17'),
                (1, 2, 2, 'Goodbye, Mrs. Selvig', '2025-01-24'),
                (1, 2, 3, 'Who Is Alive?', '2025-01-31'),
                (2, 1, 1, 'Welcome to the Playground', '2021-11-06')",
        )
        .execute(&self.state.pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn a_token_reaches_the_extension_and_nothing_else() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    assert!(token.starts_with("frog_"));

    let (status, me) = h.call("GET", "/api/ext/me", Who::Token(&token), None).await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["username"], json!("mihai"));
    assert_eq!(me["watching"], json!([]));

    // Everything outside /api/ext refuses it, reads and writes alike.
    for (method, path, body) in [
        ("GET", "/api/library", None),
        ("GET", "/api/settings", None),
        ("PUT", "/api/settings", Some(json!({ "theme": "pond" }))),
        ("GET", "/api/account", None),
        (
            "PUT",
            "/api/account/password",
            Some(json!({ "current": "a-long-enough-pass", "password": "whatever-new-pass" })),
        ),
        ("POST", "/api/auth/two-factor/start", Some(json!({}))),
        (
            "POST",
            "/api/account/tokens",
            Some(json!({ "name": "another" })),
        ),
        ("GET", "/api/invites", None),
        ("POST", "/api/invites", Some(json!({}))),
        ("GET", "/api/admin/users", None),
        ("POST", "/api/t/1/episodes/1/1/watch", Some(json!({}))),
    ] {
        let (status, body) = h.call(method, path, Who::Token(&token), body).await;
        assert!(
            status == StatusCode::FORBIDDEN
                || (path.contains("two-factor") && status != StatusCode::OK),
            "{method} {path} answered {status} to a token: {body}"
        );
    }
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_token")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "a token cannot make another");

    // And /api/ext takes nothing but a token.
    let (status, _) = h
        .call("GET", "/api/ext/me", Who::Session(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = h.call("GET", "/api/ext/me", Who::Nobody, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = h
        .call("GET", "/api/ext/me", Who::Token("frog_nope"), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_token_is_shown_once_listed_with_its_last_use_and_revoked() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (id, token) = h.token(&cookie, "  Zen  ").await;

    let (_, listed) = h
        .call("GET", "/api/account/tokens", Who::Session(&cookie), None)
        .await;
    let first = &listed["tokens"][0];
    assert_eq!(first["name"], json!("Zen"));
    assert_eq!(first["last_used_at"], Value::Null);
    assert!(
        !listed.to_string().contains(&token),
        "the list never holds the token"
    );
    let stored: (String,) = sqlx::query_as("SELECT token_hash FROM api_token")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_ne!(stored.0, token, "only its hash is kept");

    h.call("GET", "/api/ext/me", Who::Token(&token), None).await;
    let (_, listed) = h
        .call("GET", "/api/account/tokens", Who::Session(&cookie), None)
        .await;
    assert!(listed["tokens"][0]["last_used_at"].is_string());

    // Someone else cannot revoke it.
    let other = h.second(&cookie).await;
    let (status, _) = h
        .call(
            "DELETE",
            &format!("/api/account/tokens/{id}"),
            Who::Session(&other),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = h.call("GET", "/api/ext/me", Who::Token(&token), None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = h
        .call(
            "DELETE",
            &format!("/api/account/tokens/{id}"),
            Who::Session(&cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = h.call("GET", "/api/ext/me", Who::Token(&token), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "revoked means gone at once"
    );
}

#[tokio::test]
async fn a_switched_off_account_loses_its_tokens_at_once() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let other = h.second(&cookie).await;
    let (_, token) = h.token(&other, "Brave").await;
    sqlx::query("UPDATE app_user SET disabled = 1 WHERE username = 'zjef'")
        .execute(&h.state.pool)
        .await
        .unwrap();
    let (status, _) = h.call("GET", "/api/ext/me", Who::Token(&token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_tokens_are_throttled() {
    let h = Harness::new().await;
    let mut last = StatusCode::OK;
    for n in 0..25 {
        last = h
            .call(
                "GET",
                "/api/ext/me",
                Who::Token(&format!("frog_guess{n}")),
                None,
            )
            .await
            .0;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_good_token_is_not_held_back_by_its_own_use() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    for _ in 0..40 {
        let (status, _) = h.call("GET", "/api/ext/me", Who::Token(&token), None).await;
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn resolve_is_exact_from_ids_and_less_sure_from_names() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    h.catalogue().await;

    // A TMDB id, its kind and the numbers: exact, with the episode's name.
    let (status, found) = h
        .ext(
            "resolve",
            &token,
            json!({ "tmdb_id": 95396, "kind": "tv", "season": 2, "episode": 1 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["match"]["confidence"], json!("exact"));
    assert_eq!(found["match"]["title_id"], json!(1));
    assert_eq!(found["match"]["episode_name"], json!("Hello, Ms. Cobel"));

    // Numbers alone say it is a show, so a TMDB id needs no kind.
    let (_, found) = h
        .ext(
            "resolve",
            &token,
            json!({ "tmdb_id": 95396, "season": 1, "episode": 2 }),
        )
        .await;
    assert_eq!(found["match"]["confidence"], json!("exact"));

    // The extension's own rule names Frog's id.
    let (_, found) = h.ext("resolve", &token, json!({ "title_id": 3 })).await;
    assert_eq!(found["match"]["confidence"], json!("exact"));
    assert_eq!(found["match"]["kind"], json!("movie"));

    // A show with no way to tell which episode is never exact.
    let (_, found) = h
        .ext("resolve", &token, json!({ "tmdb_id": 95396, "kind": "tv" }))
        .await;
    assert_eq!(found["match"]["confidence"], json!("likely"));
    assert_eq!(found["match"]["episode"], Value::Null);

    // A name and its numbers: likely, never exact.
    let (_, found) = h
        .ext(
            "resolve",
            &token,
            json!({ "page_title": "Severance S01E03 | SomeSite - Watch online free" }),
        )
        .await;
    assert_eq!(found["match"]["title_id"], json!(1), "{found}");
    assert_eq!(found["match"]["confidence"], json!("likely"));
    assert_eq!(
        (
            found["match"]["season"].as_i64(),
            found["match"]["episode"].as_i64()
        ),
        (Some(1), Some(3))
    );

    // An absolute number, as anime sites give it, lands on the season it is in.
    let (_, found) = h
        .ext(
            "resolve",
            &token,
            json!({ "title": "Severance", "absolute": 5 }),
        )
        .await;
    assert_eq!(
        (
            found["match"]["season"].as_i64(),
            found["match"]["episode"].as_i64()
        ),
        (Some(2), Some(2))
    );

    // Nothing usable: no match, so the extension asks.
    let (status, found) = h
        .ext("resolve", &token, json!({ "page_title": "Video player" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found["match"], Value::Null);
}

#[tokio::test]
async fn a_name_two_titles_share_is_a_guess_with_alternatives() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    sqlx::query(
        "INSERT INTO title_seed (id, kind, tmdb_id, name, release_date) VALUES
            (1, 'tv', 1, 'Shogun', '1980-09-15'),
            (2, 'tv', 2, 'Shogun', '2024-02-27')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    let (_, found) = h.ext("resolve", &token, json!({ "title": "Shōgun" })).await;
    assert_eq!(found["match"]["confidence"], json!("guess"), "{found}");
    assert_eq!(found["alternatives"].as_array().unwrap().len(), 1);

    // The year tells them apart.
    let (_, found) = h
        .ext(
            "resolve",
            &token,
            json!({ "title": "Shogun", "year": 2024 }),
        )
        .await;
    assert_eq!(found["match"]["title_id"], json!(2));
    assert_eq!(found["match"]["confidence"], json!("likely"));
}

#[tokio::test]
async fn logging_is_the_checkmark_and_happens_once() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    h.catalogue().await;

    let (status, first) = h
        .ext(
            "log",
            &token,
            json!({ "title_id": 1, "season": 1, "episode": 1 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["already"], json!(false));
    assert_eq!(
        first["status"],
        json!("watching"),
        "into the library, as a tick puts it"
    );

    // A reload, a second tab: the same watch.
    let (_, again) = h
        .ext(
            "log",
            &token,
            json!({ "title_id": 1, "season": 1, "episode": 1 }),
        )
        .await;
    assert_eq!(again["already"], json!(true));
    assert_eq!(again["log_id"], first["log_id"]);
    assert_eq!(h.count("SELECT COUNT(*) FROM watch_event").await, 1);
    assert_eq!(
        h.count("SELECT COUNT(*) FROM watch_event WHERE source = 'extension' AND undated = 0")
            .await,
        1
    );

    // It reaches the diary like any dated watch.
    let (_, diary) = h
        .call("GET", "/api/diary", Who::Session(&cookie), None)
        .await;
    assert!(diary.to_string().contains("Severance"), "{diary}");

    // The popup's next episode moves on.
    let (_, me) = h.call("GET", "/api/ext/me", Who::Token(&token), None).await;
    assert_eq!(me["watching"][0]["next"]["episode_number"], json!(2));

    // Another browser's token is another watch.
    let (_, other) = h.token(&cookie, "Zen").await;
    let (_, third) = h
        .ext(
            "log",
            &other,
            json!({ "title_id": 1, "season": 1, "episode": 1 }),
        )
        .await;
    assert_eq!(third["already"], json!(false));
    assert_eq!(h.count("SELECT COUNT(*) FROM watch_event").await, 2);

    // A show needs its episode; an episode needs to exist.
    let (status, _) = h.ext("log", &token, json!({ "title_id": 1 })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = h
        .ext(
            "log",
            &token,
            json!({ "title_id": 1, "season": 9, "episode": 9 }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let later = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let (status, _) = h
        .ext(
            "log",
            &token,
            json!({ "title_id": 1, "season": 1, "episode": 2, "watched_at": later }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_last_episode_finishes_an_ended_show_as_a_tick_would() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    h.catalogue().await;
    let (_, logged) = h
        .ext(
            "log",
            &token,
            json!({ "title_id": 2, "season": 1, "episode": 1 }),
        )
        .await;
    assert_eq!(logged["status"], json!("completed"), "{logged}");
}

#[tokio::test]
async fn undo_takes_back_exactly_what_the_log_added() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    h.catalogue().await;

    // A show that was not in the library leaves it again.
    let (_, logged) = h
        .ext(
            "log",
            &token,
            json!({ "title_id": 1, "season": 1, "episode": 1 }),
        )
        .await;
    let (status, _) = h
        .ext("undo", &token, json!({ "log_id": logged["log_id"] }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h.count("SELECT COUNT(*) FROM watch_event").await, 0);
    assert_eq!(h.count("SELECT COUNT(*) FROM library_entry").await, 0);
    let (status, _) = h
        .ext("undo", &token, json!({ "log_id": logged["log_id"] }))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an undo happens once");

    // A film on the watchlist goes back to the watchlist.
    h.call(
        "PUT",
        "/api/t/3/library",
        Who::Session(&cookie),
        Some(json!({ "status": "planned" })),
    )
    .await;
    let (_, logged) = h.ext("log", &token, json!({ "title_id": 3 })).await;
    assert_eq!(logged["status"], json!("completed"));
    h.ext("undo", &token, json!({ "log_id": logged["log_id"] }))
        .await;
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM library_entry WHERE title_id = 3")
            .fetch_one(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(status, "planned");
    assert_eq!(h.count("SELECT COUNT(*) FROM watch_event").await, 0);

    // A watch ticked by hand is never the extension's to take.
    h.call(
        "POST",
        "/api/t/1/episodes/1/2/watch",
        Who::Session(&cookie),
        Some(json!({})),
    )
    .await;
    let (_, logged) = h
        .ext(
            "log",
            &token,
            json!({ "title_id": 1, "season": 1, "episode": 3 }),
        )
        .await;
    h.ext("undo", &token, json!({ "log_id": logged["log_id"] }))
        .await;
    assert_eq!(h.count("SELECT COUNT(*) FROM watch_event").await, 1);
    assert_eq!(
        h.count("SELECT COUNT(*) FROM library_entry WHERE title_id = 1")
            .await,
        1
    );

    // Nor is another account's log.
    let other = h.second(&cookie).await;
    let (_, theirs) = h.token(&other, "Theirs").await;
    let (_, logged) = h.ext("log", &token, json!({ "title_id": 3 })).await;
    let (status, _) = h
        .ext("undo", &theirs, json!({ "log_id": logged["log_id"] }))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        h.count("SELECT COUNT(*) FROM watch_event WHERE source = 'extension'")
            .await,
        1
    );
}

#[tokio::test]
async fn one_account_never_sees_another_through_the_extension() {
    let h = Harness::new().await;
    let cookie = h.owner().await;
    let (_, token) = h.token(&cookie, "Brave").await;
    h.catalogue().await;
    h.ext(
        "log",
        &token,
        json!({ "title_id": 1, "season": 1, "episode": 1 }),
    )
    .await;

    let other = h.second(&cookie).await;
    let (_, theirs) = h.token(&other, "Theirs").await;
    let (_, me) = h
        .call("GET", "/api/ext/me", Who::Token(&theirs), None)
        .await;
    assert_eq!(me["username"], json!("zjef"));
    assert_eq!(me["watching"], json!([]));
    let (_, found) = h
        .ext(
            "resolve",
            &theirs,
            json!({ "tmdb_id": 95396, "kind": "tv", "season": 1, "episode": 1 }),
        )
        .await;
    assert_eq!(found["match"]["in_library"], json!(false));
}

/// A stand-in for the release pages: the signed build for one version, and
/// how many times it was asked for.
async fn releases(file: &'static [u8]) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let hits = std::sync::Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let path = format!(
        "/download/v{}/frog-extension-firefox.xpi",
        env!("CARGO_PKG_VERSION")
    );
    let app = axum::Router::new().route(
        &path,
        axum::routing::get(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            async move { file }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), hits)
}

const XPI: &[u8] = b"PK a signed add-on";

#[tokio::test]
async fn firefox_installs_the_extension_from_frog_with_a_link_from_settings() {
    use std::sync::atomic::Ordering;
    let (base, hits) = releases(XPI).await;
    let h = Harness::with_releases(&base).await;
    let cookie = h.owner().await;

    // Only a signed-in page gets a link.
    let (status, _) = h
        .call("GET", "/api/extension/link", Who::Nobody, None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, link) = h
        .call("GET", "/api/extension/link", Who::Session(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let url = link["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("/extension/frog.xpi?key="), "{url}");

    // The link needs no cookie, since Firefox may fetch it on its own.
    let (status, headers, body) = h.raw(&url, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/x-xpinstall");
    assert_eq!(body, XPI);

    // Fetched once, then served from memory.
    let (status, _, body) = h.raw(&url, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, XPI);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Without a link from Settings there is nothing here.
    for path in ["/extension/frog.xpi", "/extension/frog.xpi?key=guessed"] {
        let (status, _, _) = h.raw(path, Some(&cookie)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn without_a_build_to_give_the_link_goes_to_the_release_page() {
    // GitHub away, or a page that is not an add-on.
    for h in [
        Harness::new().await,
        Harness::with_releases(&releases(b"<html>not found</html>").await.0).await,
    ] {
        let cookie = h.owner().await;
        let (_, link) = h
            .call("GET", "/api/extension/link", Who::Session(&cookie), None)
            .await;
        let (status, headers, _) = h.raw(link["url"].as_str().unwrap(), None).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert!(headers[header::LOCATION]
            .to_str()
            .unwrap()
            .ends_with("/latest/download/frog-extension-firefox.xpi"));
    }
}
