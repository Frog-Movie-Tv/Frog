//! End-to-end exercise of the HTTP surface that does not need TMDB: first-run
//! setup, login, the auth wall, and the shape of an empty library.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use frog::config::Config;
use frog::state::AppState;

struct Harness {
    state: AppState,
    _dir: tempfile::TempDir,
}

impl Harness {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");
        let url = format!(
            "sqlite:{}?mode=rwc",
            db_path.to_string_lossy().replace('\\', "/")
        );

        let config = Config {
            // Deliberately no TMDB key: everything under test here is local.
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
        };

        let pool = frog::db::connect(&url).await.expect("database");
        Self {
            state: AppState::new(pool, config),
            _dir: dir,
        }
    }

    fn router(&self) -> axum::Router {
        frog::routes::router(self.state.clone())
    }

    async fn call(&self, req: Request<Body>) -> (StatusCode, Value, Option<String>) {
        let res = self.router().oneshot(req).await.expect("response");
        let status = res.status();
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).to_string());
        let bytes = res.into_body().collect().await.expect("body").to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value, cookie)
    }

    async fn get(&self, path: &str, cookie: Option<&str>) -> (StatusCode, Value) {
        let mut req = Request::builder().uri(path).method("GET");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        let (s, v, _) = self.call(req.body(Body::empty()).unwrap()).await;
        (s, v)
    }

    async fn post(
        &self,
        path: &str,
        body: Value,
        cookie: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder()
            .uri(path)
            .method("POST")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        self.call(req.body(Body::from(body.to_string())).unwrap())
            .await
    }
}

#[tokio::test]
async fn health_is_public_and_reports_the_database() {
    let h = Harness::new().await;
    let (status, body) = h.get("/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["tmdb_configured"], json!(false));
    assert_eq!(body["region"], json!("BE"));
}

#[tokio::test]
async fn a_fresh_instance_asks_for_setup_then_signs_you_in() {
    let h = Harness::new().await;

    let (_, status) = h.get("/api/auth/status", None).await;
    assert_eq!(status["needs_setup"], json!(true));
    assert_eq!(status["authenticated"], json!(false));

    let (code, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let cookie = cookie.expect("setup hands back a session cookie");
    assert!(cookie.starts_with("frog_session="));

    let (_, status) = h.get("/api/auth/status", Some(&cookie)).await;
    assert_eq!(status["needs_setup"], json!(false));
    assert_eq!(status["authenticated"], json!(true));
    assert_eq!(status["username"], json!("mihai"));
}

#[tokio::test]
async fn setup_runs_once_and_validates_input() {
    let h = Harness::new().await;

    let (code, body, _) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "m", "password": "short" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("username"));

    let (code, _, _) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);

    // Second attempt is refused, so an exposed instance cannot be taken over.
    let (code, _, _) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "someone-else", "password": "another-long-pass" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT);
}

#[tokio::test]
async fn login_rejects_a_wrong_password_and_accepts_the_right_one() {
    let h = Harness::new().await;
    h.post(
        "/api/auth/setup",
        json!({ "username": "mihai", "password": "a-long-enough-pass" }),
        None,
    )
    .await;

    let (code, _, _) = h
        .post(
            "/api/auth/login",
            json!({ "username": "mihai", "password": "nope-nope-nope" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    let (code, _, cookie) = h
        .post(
            "/api/auth/login",
            json!({ "username": "MIHAI", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "username match is case-insensitive");
    assert!(cookie.is_some());
}

#[tokio::test]
async fn private_routes_are_closed_without_a_session() {
    let h = Harness::new().await;
    for path in [
        "/api/library",
        "/api/library/stats",
        "/api/search?q=heat",
        "/api/titles/movie/550",
        "/img/w342/abc.jpg",
    ] {
        let (status, _) = h.get(path, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} should need auth");
    }
}

#[tokio::test]
async fn logging_out_invalidates_the_cookie() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    let (status, _) = h.get("/api/library", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);

    h.post("/api/auth/logout", json!({}), Some(&cookie)).await;

    let (status, _) = h.get("/api/library", Some(&cookie)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_empty_library_returns_empty_lists_not_errors() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    let (status, body) = h.get("/api/library", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"], json!([]));

    let (status, body) = h.get("/api/library/stats", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["films_watched"], json!(0));
    assert_eq!(body["episodes_watched"], json!(0));
    assert_eq!(body["minutes_watched"], json!(0));
}

/// Regression: SUM() over INTEGER runtimes came back as INTEGER and blew up
/// decoding into f64, and MAX() over no rows returned a row holding NULL
/// rather than no row. Both produced a 500 on pages that must always work.
#[tokio::test]
async fn library_and_stats_survive_real_rows() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    // A film that was watched, and a show that never was: the second one is
    // what made MAX(watched_at) decode a NULL.
    sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name, runtime, genres_json)
         VALUES ('movie', 1, 'Watched Film', 139, '[\"Drama\"]'),
                ('tv', 2, 'Untouched Show', 42, '[]')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO library_entry (title_id, status, rating, added_at, updated_at)
         VALUES (1, 'completed', 90.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (2, 'planned', NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO watch_event (title_id, episode_id, watched_at, is_rewatch, source)
         VALUES (1, NULL, '2026-01-02T20:00:00Z', 0, 'manual')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (status, body) = h.get("/api/library", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "library failed: {body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 2);

    let (status, body) = h.get("/api/library/stats", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {body}");
    assert_eq!(body["films_watched"], json!(1));
    assert_eq!(body["minutes_watched"], json!(139));
    assert_eq!(body["mean_rating"], json!(90.0));
}

/// An internal fault must never put a database error on the screen.
#[tokio::test]
async fn internal_errors_are_not_shown_verbatim() {
    use axum::response::IntoResponse;
    use frog::error::AppError;

    let res = AppError::Other(anyhow::anyhow!("connection pool exploded")).into_response();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let shown = body["error"].as_str().unwrap();
    assert!(!shown.contains("exploded"), "leaked internals: {shown}");
    assert!(shown.contains("server log"), "unhelpful message: {shown}");
}

/// Marking a whole season watched must skip episodes that have not aired.
#[tokio::test]
async fn bulk_season_marking_skips_unaired_episodes() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    sqlx::query("INSERT INTO title (kind, tmdb_id, name) VALUES ('tv', 42, 'Part Aired')")
        .execute(&h.state.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO episode (title_id, season_number, episode_number, name, air_date)
         VALUES (1, 1, 1, 'Aired',   '2020-01-01'),
                (1, 1, 2, 'Aired',   '2020-01-08'),
                (1, 1, 3, 'Future',  '2099-01-01'),
                (1, 1, 4, 'Unknown', NULL)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (status, body, _) = h
        .post(
            "/api/titles/tv/42/episodes/1/watch",
            json!({ "watched": true }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (watched,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM watch_event")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(
        watched, 3,
        "two aired plus the undated one, never the future one"
    );
}

#[tokio::test]
async fn bad_filters_are_rejected_rather_than_ignored() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    let (status, body) = h.get("/api/library?status=nonsense", Some(&cookie)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("nonsense"));
}

#[tokio::test]
async fn without_a_tmdb_key_search_says_so_instead_of_failing_opaquely() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    let (status, body) = h.get("/api/search?q=heat", Some(&cookie)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["error"].as_str().unwrap().contains("TMDB_API_KEY"));

    let (status, _) = h.get("/api/search?q=", Some(&cookie)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
