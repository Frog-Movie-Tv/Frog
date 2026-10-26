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
        Self::with_tmdb_key("").await
    }

    /// A key makes the TMDB client count as configured. Requests are still
    /// answered from whatever a test puts in the cache.
    async fn with_tmdb_key(key: &str) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");
        let url = format!(
            "sqlite:{}?mode=rwc",
            db_path.to_string_lossy().replace('\\', "/")
        );

        let config = Config {
            // No real key: everything under test here is local.
            tmdb_api_key: key.to_string(),
            database_url: url.clone(),
            bind: "127.0.0.1:0".parse().unwrap(),
            static_dir: dir.path().join("static"),
            region: "BE".into(),
            language: "en-US".into(),
            admin_username: None,
            admin_password: None,
            secure_cookies: false,
            session_days: 7,
            notify_hour: 18,
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

/// Seed one show with two aired seasons and a future episode, signed in.
async fn show_with_seasons(h: &Harness) -> String {
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;

    sqlx::query("INSERT INTO title (kind, tmdb_id, name) VALUES ('tv', 48866, 'The 100')")
        .execute(&h.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO episode (title_id, season_number, episode_number, name, air_date)
         VALUES (1, 1, 1, 'Pilot',      '2014-03-19'),
                (1, 1, 2, 'Earth Skills','2014-03-26'),
                (1, 2, 1, 'The 48',     '2014-10-22'),
                (1, 2, 2, 'Inclement',  NULL),
                (1, 3, 1, 'Future',     '2099-01-01'),
                (1, 0, 1, 'Special',    '2014-01-01')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    cookie.unwrap()
}

/// Marking a back catalogue with no idea when must not flood the diary.
#[tokio::test]
async fn bulk_marking_with_an_unknown_date_stays_out_of_the_diary() {
    let h = Harness::new().await;
    let cookie = show_with_seasons(&h).await;

    let (status, body, _) = h
        .post(
            "/api/titles/tv/48866/episodes/watch",
            json!({ "watched": true, "when": "unknown" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Four aired episodes in seasons 1-2; never the future one or the special.
    assert_eq!(body["item"]["watched_episodes"], json!(4));

    let (_, diary) = h.get("/api/diary", Some(&cookie)).await;
    assert_eq!(diary["entries"], json!([]));
    assert_eq!(diary["undated"], json!(4));
}

#[tokio::test]
async fn bulk_marking_on_release_dates_uses_each_air_date() {
    let h = Harness::new().await;
    let cookie = show_with_seasons(&h).await;

    h.post(
        "/api/titles/tv/48866/episodes/1/watch",
        json!({ "watched": true, "when": "release" }),
        Some(&cookie),
    )
    .await;

    let dates: Vec<(String, i64)> =
        sqlx::query_as("SELECT watched_at, undated FROM watch_event ORDER BY watched_at")
            .fetch_all(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(
        dates,
        vec![
            ("2014-03-19T20:00:00Z".to_string(), 0),
            ("2014-03-26T20:00:00Z".to_string(), 0)
        ]
    );

    // An episode with no known air date is kept undated rather than lying.
    h.post(
        "/api/titles/tv/48866/episodes/2/watch",
        json!({ "watched": true, "when": "release" }),
        Some(&cookie),
    )
    .await;
    let (undated,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM watch_event WHERE undated = 1")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(undated, 1);
}

#[tokio::test]
async fn a_future_bulk_date_is_refused() {
    let h = Harness::new().await;
    let cookie = show_with_seasons(&h).await;

    let (status, body, _) = h
        .post(
            "/api/titles/tv/48866/episodes/watch",
            json!({ "watched": true, "when": "date", "date": "2099-05-01" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("future"));
}

/// Many episodes of one show on one day are one diary entry, and the whole
/// group can be moved out of the diary afterwards.
#[tokio::test]
async fn the_diary_groups_a_day_of_one_show_and_can_be_redated() {
    let h = Harness::new().await;
    let cookie = show_with_seasons(&h).await;

    h.post(
        "/api/titles/tv/48866/episodes/watch",
        json!({ "watched": true, "when": "date", "date": "2026-10-24" }),
        Some(&cookie),
    )
    .await;

    let (_, diary) = h.get("/api/diary", Some(&cookie)).await;
    let entries = diary["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{diary}");
    assert_eq!(entries[0]["count"], json!(4));
    assert_eq!(entries[0]["day"], json!("2026-10-24"));
    assert_eq!(entries[0]["episodes"].as_array().unwrap().len(), 4);
    assert_eq!(entries[0]["episodes"][0]["season_number"], json!(1));

    let (status, body, _) = h
        .post(
            "/api/diary/redate",
            json!({ "kind": "tv", "tmdb_id": 48866, "day": "2026-10-24", "when": "unknown" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["changed"], json!(4));

    let (_, diary) = h.get("/api/diary", Some(&cookie)).await;
    assert_eq!(diary["entries"], json!([]));
    assert_eq!(diary["undated"], json!(4));

    // Still watched: progress is untouched by dates.
    let (watched,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM watch_event")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(watched, 4);
}

/// Put a show and its seasons into the TMDB cache, so code that "fetches"
/// them works offline. Keys mirror `Tmdb::cache_key` for language en-US.
async fn cache_show(h: &Harness, id: i64, name: &str, seasons: &[(i64, i64)]) {
    let detail = json!({
        "id": id,
        "name": name,
        "first_air_date": "2013-04-07",
        "genres": [{ "id": 16, "name": "Animation" }],
        "number_of_seasons": seasons.len(),
        "seasons": seasons.iter().map(|(s, n)| json!({
            "season_number": s, "name": format!("Season {s}"), "episode_count": n
        })).collect::<Vec<_>>(),
    });
    let detail_key = format!(
        "/tv/{id}?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids&include_image_language=en,null"
    );
    let mut rows = vec![(detail_key, detail)];
    for (s, n) in seasons {
        let episodes: Vec<Value> = (1..=*n)
            .map(|e| {
                json!({
                    "season_number": s, "episode_number": e,
                    "name": format!("Episode {e}"), "air_date": "2015-01-01"
                })
            })
            .collect();
        rows.push((
            format!("/tv/{id}/season/{s}?lang=en-US"),
            json!({ "season_number": s, "episodes": episodes }),
        ));
    }
    for (key, body) in rows {
        sqlx::query(
            "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
             VALUES (?, ?, '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
        )
        .bind(key)
        .bind(body.to_string())
        .execute(&h.state.pool)
        .await
        .unwrap();
    }
}

/// An anime tracker says "Season 2, 7 episodes in": exactly those episodes
/// are ticked, undated, and a completed season ticks all of its own.
#[tokio::test]
async fn importing_an_anime_season_ticks_that_season_undated() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();
    cache_show(&h, 1429, "Attack on Titan", &[(1, 25), (2, 12), (3, 22)]).await;

    let (status, body, _) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [
                { "kind": "tv", "tmdb_id": 1429, "status": "completed", "season": 1 },
                { "kind": "tv", "tmdb_id": 1429, "status": "watching", "season": 2, "progress": 7 },
                { "kind": "tv", "tmdb_id": 1429, "status": "planned", "season": 3 }
            ]}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let per_season: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT e.season_number, COUNT(*) FROM watch_event w JOIN episode e ON e.id = w.episode_id
          GROUP BY e.season_number ORDER BY e.season_number",
    )
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(per_season, vec![(1, 25), (2, 7)]);

    let (dated,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM watch_event WHERE undated = 0")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(dated, 0, "imported progress has no real dates");

    // The later season's status is the one the library keeps.
    let (status,): (String,) = sqlx::query_as("SELECT status FROM library_entry")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(status, "watching");
}

/// "Season 3 Part 2, 4 episodes in" is TMDB season 3 episodes 13 to 16.
#[tokio::test]
async fn a_later_part_of_a_season_is_offset_onto_tmdb_numbering() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();
    cache_show(&h, 1429, "Attack on Titan", &[(1, 25), (2, 12), (3, 22)]).await;

    let (status, body, _) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [
                { "kind": "tv", "tmdb_id": 1429, "status": "completed", "season": 3,
                  "episode_offset": 0, "episode_total": 12 },
                { "kind": "tv", "tmdb_id": 1429, "status": "watching", "season": 3,
                  "episode_offset": 12, "progress": 4 }
            ]}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let ticked: Vec<(i64,)> = sqlx::query_as(
        "SELECT e.episode_number FROM watch_event w JOIN episode e ON e.id = w.episode_id
          WHERE e.season_number = 3 ORDER BY e.episode_number",
    )
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    let numbers: Vec<i64> = ticked.into_iter().map(|(n,)| n).collect();
    assert_eq!(numbers, (1..=16).collect::<Vec<_>>());
}

/// Two-factor sign-in: setup needs a working code, sign-in then asks for one,
/// a code works once, recovery codes work once, and the password turns it off.
#[tokio::test]
async fn two_factor_sign_in_end_to_end() {
    let h = Harness::new().await;
    let creds = json!({ "username": "mihai", "password": "a-long-enough-pass" });
    let (_, _, cookie) = h.post("/api/auth/setup", creds.clone(), None).await;
    let cookie = cookie.unwrap();

    let (status, _, _) = h
        .post("/api/auth/two-factor/start", json!({}), Some(&cookie))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (pending,): (String,) = sqlx::query_as("SELECT totp_pending FROM app_user")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    let secret = frog::totp::base32_decode(&pending).unwrap();
    let step = frog::totp::step_at(chrono::Utc::now().timestamp());
    let code = |s: i64| format!("{:06}", frog::totp::code_at(&secret, s));

    let (status, _, _) = h
        .post(
            "/api/auth/two-factor/confirm",
            json!({ "code": "000000" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a wrong code must not turn it on"
    );

    let (status, body, _) = h
        .post(
            "/api/auth/two-factor/confirm",
            json!({ "code": code(step) }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let recovery: Vec<String> = serde_json::from_value(body["recovery_codes"].clone()).unwrap();
    assert_eq!(recovery.len(), 10);

    // Password alone is no longer enough, and says why.
    let (status, body, cookie_out) = h.post("/api/auth/login", creds.clone(), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["two_factor"], true);
    assert!(cookie_out.is_none());

    let with = |c: &str| {
        let mut v = creds.clone();
        v["code"] = json!(c);
        v
    };
    let (status, body, _) = h.post("/api/auth/login", with("123456"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["two_factor"], true);

    // The setup code's step is spent; the next step's code works once.
    let (status, _, _) = h.post("/api/auth/login", with(&code(step)), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a code cannot be replayed"
    );
    let (status, _, signed_in) = h.post("/api/auth/login", with(&code(step + 1)), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(signed_in.is_some());
    let (status, _, _) = h.post("/api/auth/login", with(&code(step + 1)), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Recovery codes, typed loosely, work exactly once.
    let loose = recovery[0].to_uppercase().replace('-', " ");
    let (status, _, _) = h.post("/api/auth/login", with(&loose), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = h.post("/api/auth/login", with(&recovery[0]), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, body) = h.get("/api/auth/two-factor", Some(&cookie)).await;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["recovery_codes_left"], 9);

    let (status, _, _) = h
        .post(
            "/api/auth/two-factor/disable",
            json!({ "password": "wrong-password" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = h
        .post(
            "/api/auth/two-factor/disable",
            json!({ "password": "a-long-enough-pass" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = h.post("/api/auth/login", creds, None).await;
    assert_eq!(status, StatusCode::OK);
}

/// Put a film with credits into the TMDB cache, keyed like `Tmdb::movie`.
async fn cache_film(h: &Harness, id: i64, name: &str, year: &str, actors: &[(i64, &str)]) {
    let detail = json!({
        "id": id,
        "title": name,
        "release_date": format!("{year}-05-01"),
        "runtime": 100,
        "genres": [{ "id": 18, "name": "Drama" }],
        "credits": {
            "cast": actors.iter().enumerate().map(|(i, (pid, pname))| json!({
                "id": pid, "name": pname, "order": i
            })).collect::<Vec<_>>(),
            "crew": [{ "id": 900, "name": "Same Director", "job": "Director" }]
        },
        "production_companies": [{ "id": 50, "name": "Pond Pictures" }]
    });
    let key = format!(
        "/movie/{id}?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids,release_dates&include_image_language=en,null"
    );
    sqlx::query(
        "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
         VALUES (?, ?, '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
    )
    .bind(key)
    .bind(detail.to_string())
    .execute(&h.state.pool)
    .await
    .unwrap();
}

/// Credits are stored when a title is added, and stats rank people, genres,
/// decades and ratings over all time or over one year's dated watches.
#[tokio::test]
async fn stats_cover_all_time_and_a_single_year() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    cache_film(
        &h,
        11,
        "Older Film",
        "1994",
        &[(1, "Lead Actor"), (2, "Other")],
    )
    .await;
    cache_film(
        &h,
        12,
        "Newer Film",
        "2021",
        &[(1, "Lead Actor"), (3, "Third")],
    )
    .await;

    for (id, rating) in [(11, 95.0), (12, 70.0)] {
        let req = axum::http::Request::builder()
            .uri(format!("/api/titles/movie/{id}/library"))
            .method("PUT")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &cookie)
            .body(Body::from(
                json!({ "status": "completed", "rating": rating }).to_string(),
            ))
            .unwrap();
        let (status, body, _) = h.call(req).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let (people,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM title_person")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    // 2 + 2 cast, a director each, a studio each.
    assert_eq!(people, 8);

    // Finishing a film records that you watched it -- once, however often
    // the status is set again.
    let put = |id: i64, body: Value| {
        axum::http::Request::builder()
            .uri(format!("/api/titles/movie/{id}/library"))
            .method("PUT")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &cookie)
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    h.call(put(11, json!({ "status": "completed" }))).await;
    h.call(put(11, json!({ "rating": 95.0 }))).await;
    let (watches,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM watch_event WHERE episode_id IS NULL AND undated = 0")
            .fetch_one(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(watches, 2, "one watch per finished film");
    sqlx::query("DELETE FROM watch_event")
        .execute(&h.state.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO watch_event (title_id, episode_id, watched_at, undated)
         VALUES (1, NULL, '2025-12-30T20:00:00Z', 0),
                (1, NULL, '2025-12-31T20:00:00Z', 0),
                (2, NULL, '2026-03-02T20:00:00Z', 0),
                (2, NULL, '2020-01-01T00:00:00Z', 1)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (status, all) = h.get("/api/stats", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(all["years"], json!([2026, 2025]));
    assert_eq!(
        all["totals"]["films"],
        json!(4),
        "undated watches count all time"
    );
    assert_eq!(all["totals"]["minutes"], json!(400));
    assert_eq!(all["totals"]["days"], json!(3));
    assert_eq!(all["genres"][0]["name"], json!("Drama"));
    assert_eq!(all["genres"][0]["titles"], json!(2));
    assert_eq!(
        all["decades"],
        json!([{ "decade": 1990, "titles": 1 }, { "decade": 2020, "titles": 1 }])
    );
    assert_eq!(all["ratings"][9], json!(1));
    assert_eq!(all["ratings"][7], json!(1));
    assert_eq!(all["streaks"]["longest"]["days"], json!(2));
    assert_eq!(all["people"]["actors"][0]["name"], json!("Lead Actor"));
    assert_eq!(all["people"]["actors"][0]["titles"], json!(2));
    assert_eq!(all["people"]["directors"][0]["titles"], json!(2));
    assert_eq!(all["people"]["studios"][0]["name"], json!("Pond Pictures"));
    assert_eq!(
        all["top_films"][0]["name"],
        json!("Older Film"),
        "highest rated first"
    );
    assert_eq!(
        all["timeline"].as_array().unwrap().len(),
        2,
        "one point per year"
    );

    let (status, y) = h.get("/api/stats?year=2025", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{y}");
    assert_eq!(y["totals"]["films"], json!(2));
    assert_eq!(y["timeline"].as_array().unwrap().len(), 12);
    assert_eq!(y["timeline"][11]["films"], json!(2));
    assert_eq!(y["people"]["actors"][0]["titles"], json!(1));
    assert_eq!(y["ratings"][7], json!(0), "only titles watched that year");
    assert_eq!(y["decades"], json!([{ "decade": 1990, "titles": 1 }]));

    let (status, _) = h.get("/api/stats?year=12", Some(&cookie)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = h.get("/api/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Films finished before a watch was recorded get one, undated, so they count
/// all time without appearing in the diary.
#[tokio::test]
async fn finished_films_without_a_watch_get_an_undated_one() {
    let h = Harness::new().await;
    sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name) VALUES
            ('movie', 1, 'Finished'), ('movie', 2, 'Planned'), ('movie', 3, 'Watched')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (title_id, status, finished_at, added_at, updated_at) VALUES
            (1, 'completed', '2024-02-02T10:00:00Z', '2024-01-01T00:00:00Z', '2024-02-02T10:00:00Z'),
            (2, 'planned', NULL, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z'),
            (3, 'completed', '2024-03-03T10:00:00Z', '2024-01-01T00:00:00Z', '2024-03-03T10:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO watch_event (title_id, watched_at) VALUES (3, '2023-12-24T20:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!("../migrations/0006_film_watches.sql"))
        .execute(&h.state.pool)
        .await
        .unwrap();

    let rows: Vec<(i64, i64)> =
        sqlx::query_as("SELECT title_id, undated FROM watch_event ORDER BY title_id")
            .fetch_all(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(rows, vec![(1, 1), (3, 0)]);
}

/// Titles stored before credits were kept are filled in by the worker.
#[tokio::test]
async fn the_worker_backfills_credits_for_older_titles() {
    let h = Harness::with_tmdb_key("test-key").await;
    cache_film(&h, 21, "Old Entry", "2001", &[(5, "Someone")]).await;
    sqlx::query("INSERT INTO title (kind, tmdb_id, name) VALUES ('movie', 21, 'Old Entry')")
        .execute(&h.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (title_id, status, added_at, updated_at)
         VALUES (1, 'completed', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    assert_eq!(frog::jobs::backfill_credits_once(&h.state).await, 1);
    assert_eq!(
        frog::jobs::backfill_credits_once(&h.state).await,
        0,
        "done once"
    );
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM title_person WHERE name = 'Someone'")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

/// A browser subscription with real P-256 keys, as `PushSubscription.toJSON()`.
fn browser_subscription(endpoint: &str) -> Value {
    use base64::Engine;
    let rng = ring::rand::SystemRandom::new();
    let key =
        ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::ECDH_P256, &rng).unwrap();
    let public = key.compute_public_key().unwrap();
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    json!({
        "endpoint": endpoint,
        "keys": { "p256dh": b64.encode(public.as_ref()), "auth": b64.encode([7u8; 16]) }
    })
}

#[tokio::test]
async fn push_subscriptions_are_validated_and_counted() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    let (status, body) = h.get("/api/push", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let key = body["public_key"].as_str().unwrap().to_string();
    assert_eq!(key.len(), 87, "65 bytes, base64url without padding");
    let (_, again) = h.get("/api/push", Some(&cookie)).await;
    assert_eq!(again["public_key"], json!(key), "the key is generated once");

    let (status, _, _) = h
        .post(
            "/api/push/subscribe",
            browser_subscription("https://127.0.0.1/steal"),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "never a local address");

    let mut broken = browser_subscription("https://fcm.googleapis.com/fcm/send/x");
    broken["keys"]["p256dh"] = json!("nope");
    let (status, _, _) = h.post("/api/push/subscribe", broken, Some(&cookie)).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "keys we cannot encrypt for"
    );

    let endpoint = "https://fcm.googleapis.com/fcm/send/device-1";
    let (status, body, _) = h
        .post(
            "/api/push/subscribe",
            browser_subscription(endpoint),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = h.get("/api/push", Some(&cookie)).await;
    assert_eq!(body["devices"], json!(1));

    h.post(
        "/api/push/unsubscribe",
        json!({ "endpoint": endpoint }),
        Some(&cookie),
    )
    .await;
    let (_, body) = h.get("/api/push", Some(&cookie)).await;
    assert_eq!(body["devices"], json!(0));

    let (status, _) = h.get("/api/push", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// What gets announced: today's and yesterday's new episodes of followed
/// shows, once, never what you watched or switched off.
#[tokio::test]
async fn new_episodes_are_announced_once_per_show() {
    let h = Harness::new().await;
    let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();

    sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name, poster_path) VALUES
            ('tv', 1, 'Followed', '/f.jpg'), ('tv', 2, 'Muted', NULL), ('tv', 3, 'Dropped', NULL)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (title_id, status, notify, added_at, updated_at) VALUES
            (1, 'watching', 1, 'x', 'x'), (2, 'watching', 0, 'x', 'x'), (3, 'dropped', 1, 'x', 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO episode (title_id, season_number, episode_number, name, air_date) VALUES
            (1, 2, 1, 'Premiere',  '2026-10-25'),
            (1, 2, 2, 'Second',    '2026-10-26'),
            (1, 2, 3, 'Tomorrow',  '2026-10-27'),
            (1, 2, 4, 'Last week', '2026-10-19'),
            (1, 0, 1, 'Special',   '2026-10-26'),
            (2, 1, 1, 'Muted',     '2026-10-26'),
            (3, 1, 1, 'Dropped',   '2026-10-26')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    let due = frog::jobs::due_announcements(&h.state.pool, today)
        .await
        .unwrap();
    assert_eq!(due.len(), 1, "{due:?}");
    assert_eq!(due[0].episode_ids.len(), 2);
    let payload = due[0].payload();
    assert_eq!(payload["title"], json!("Followed"));
    assert_eq!(
        payload["body"],
        json!("2 new episodes, S02E01–S02E02, are out today")
    );
    assert_eq!(payload["url"], json!("/title/tv/1"));
    assert_eq!(payload["image"], json!("/img/w342/f.jpg"));

    // Watching one and announcing nothing else leaves a single-episode line.
    sqlx::query("INSERT INTO watch_event (title_id, episode_id, watched_at) VALUES (1, 1, 'x')")
        .execute(&h.state.pool)
        .await
        .unwrap();
    let due = frog::jobs::due_announcements(&h.state.pool, today)
        .await
        .unwrap();
    assert_eq!(
        due[0].payload()["body"],
        json!("S02E02 · Second is out today")
    );

    sqlx::query("INSERT INTO push_sent (episode_id, sent_at) VALUES (2, 'x')")
        .execute(&h.state.pool)
        .await
        .unwrap();
    assert!(frog::jobs::due_announcements(&h.state.pool, today)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_show_can_be_muted_from_its_library_entry() {
    let h = Harness::new().await;
    show_with_seasons(&h).await;
    sqlx::query(
        "INSERT INTO library_entry (title_id, status, added_at, updated_at)
         VALUES (1, 'watching', 'x', 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    // The title is not in the TMDB cache, so go through the store directly.
    let patch = frog::store::LibraryPatch {
        notify: Some(false),
        ..Default::default()
    };
    let entry = frog::store::apply_patch(&h.state.pool, 1, &patch)
        .await
        .unwrap();
    assert!(!entry.notify);
    assert_eq!(entry.status, "watching", "nothing else changes");
}

impl Harness {
    /// Any method with a JSON body, for routes beyond GET and POST.
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Value,
        cookie: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .uri(path)
            .method(method)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, cookie)
            .body(Body::from(body.to_string()))
            .unwrap();
        let (s, v, _) = self.call(req).await;
        (s, v)
    }
}

/// Five titles: three shows at different points, two films (one seen).
async fn shelf_library(h: &Harness) -> String {
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    sqlx::query(
        "INSERT INTO title (id, kind, tmdb_id, name, release_date, genres_json, poster_path) VALUES
            (1, 'tv', 101, 'Half Watched', '2019-01-01', '[\"Drama\"]', '/a.jpg'),
            (2, 'tv', 102, 'Untouched',    '2021-01-01', '[\"Comedy\"]', '/b.jpg'),
            (3, 'tv', 103, 'Finished',     '2010-01-01', '[\"Drama\"]', '/c.jpg'),
            (4, 'movie', 201, 'Seen Film',  '1999-01-01', '[\"Drama\"]', NULL),
            (5, 'movie', 202, 'Unseen Film','2024-01-01', '[\"Horror\"]', '/e.jpg')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (title_id, status, rating, favourite, added_at, updated_at) VALUES
            (1, 'watching', 85.0, 1, '2026-01-01', 'x'),
            (2, 'planned', 90.0, 0, '2026-01-02', 'x'),
            (3, 'completed', 80.0, 0, '2026-01-03', 'x'),
            (4, 'completed', 70.0, 0, '2026-01-04', 'x'),
            (5, 'planned', NULL, 0, '2026-01-05', 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO episode (id, title_id, season_number, episode_number) VALUES
            (1, 1, 1, 1), (2, 1, 1, 2), (3, 2, 1, 1), (4, 3, 1, 1), (5, 3, 0, 1)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO watch_event (title_id, episode_id, watched_at) VALUES
            (1, 1, 'x'), (3, 4, 'x'), (4, NULL, 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    cookie.unwrap()
}

fn names(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn a_manual_shelf_keeps_your_order_and_comes_back_after_delete() {
    let h = Harness::new().await;
    let cookie = shelf_library(&h).await;

    let (status, body, _) = h
        .post(
            "/api/shelves",
            json!({ "name": "  Comfort  ", "kind": "manual" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["shelf"]["name"], json!("Comfort"), "trimmed");
    let id = body["shelf"]["id"].as_i64().unwrap();

    for tmdb in [103, 101, 202] {
        let kind = if tmdb > 200 { "movie" } else { "tv" };
        let (s, b, _) = h
            .post(
                &format!("/api/shelves/{id}/items"),
                json!({ "kind": kind, "tmdb_id": tmdb }),
                Some(&cookie),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }
    // Adding twice changes nothing.
    h.post(
        &format!("/api/shelves/{id}/items"),
        json!({ "kind": "tv", "tmdb_id": 101 }),
        Some(&cookie),
    )
    .await;

    let (_, body) = h.get(&format!("/api/shelves/{id}"), Some(&cookie)).await;
    assert_eq!(names(&body), ["Finished", "Half Watched", "Unseen Film"]);

    let (s, _) = h
        .send(
            "PUT",
            &format!("/api/shelves/{id}/items/order"),
            json!({ "title_ids": [5, 3, 1] }),
            &cookie,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, body) = h.get(&format!("/api/shelves/{id}"), Some(&cookie)).await;
    assert_eq!(names(&body), ["Unseen Film", "Finished", "Half Watched"]);

    let (_, list) = h.get("/api/shelves", Some(&cookie)).await;
    assert_eq!(list["shelves"][0]["count"], json!(3));
    assert_eq!(
        list["shelves"][0]["posters"],
        json!(["/e.jpg", "/c.jpg", "/a.jpg"])
    );

    let (_, membership) = h.get("/api/titles/tv/101/shelves", Some(&cookie)).await;
    assert_eq!(membership["shelves"][0]["on"], json!(true));
    let (_, membership) = h.get("/api/titles/tv/102/shelves", Some(&cookie)).await;
    assert_eq!(membership["shelves"][0]["on"], json!(false));

    let req = Request::builder()
        .uri(format!("/api/shelves/{id}/items/tv/103"))
        .method("DELETE")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    h.call(req).await;
    let (_, body) = h.get(&format!("/api/shelves/{id}"), Some(&cookie)).await;
    assert_eq!(names(&body), ["Unseen Film", "Half Watched"]);

    // Delete hands back enough to undo it.
    let req = Request::builder()
        .uri(format!("/api/shelves/{id}"))
        .method("DELETE")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let (s, gone, _) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    let (_, list) = h.get("/api/shelves", Some(&cookie)).await;
    assert_eq!(list["shelves"], json!([]));
    let (items_left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM shelf_item")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(items_left, 0, "items go with the shelf");

    let (_, back, _) = h.post("/api/shelves", gone, Some(&cookie)).await;
    let back_id = back["shelf"]["id"].as_i64().unwrap();
    let (_, body) = h
        .get(&format!("/api/shelves/{back_id}"), Some(&cookie))
        .await;
    assert_eq!(
        names(&body),
        ["Unseen Film", "Half Watched"],
        "undo restores order"
    );
}

#[tokio::test]
async fn smart_shelves_follow_their_rules() {
    let h = Harness::new().await;
    let cookie = shelf_library(&h).await;

    let smart = |rules: Value| json!({ "name": "Smart", "kind": "smart", "rules": rules });
    let make = |rules: Value| {
        let h = &h;
        let cookie = cookie.clone();
        async move {
            let (s, body, _) = h.post("/api/shelves", smart(rules), Some(&cookie)).await;
            assert_eq!(s, StatusCode::OK, "{body}");
            let id = body["shelf"]["id"].as_i64().unwrap();
            let (_, body) = h.get(&format!("/api/shelves/{id}"), Some(&cookie)).await;
            let mut n = names(&body);
            n.sort();
            n
        }
    };

    // The roadmap's example: shows in progress rated 8 or more.
    assert_eq!(
        make(json!({ "kind": "tv", "progress": "started", "min_rating": 80 })).await,
        ["Half Watched"]
    );
    assert_eq!(
        make(json!({ "progress": "unstarted" })).await,
        ["Unseen Film", "Untouched"]
    );
    assert_eq!(
        make(json!({ "progress": "finished" })).await,
        ["Finished", "Seen Film"],
        "a special left unwatched does not keep a show unfinished"
    );
    assert_eq!(
        make(json!({ "progress": "unfinished", "kind": "tv" })).await,
        ["Half Watched", "Untouched"]
    );
    assert_eq!(
        make(json!({ "genres": ["Drama", "Horror"], "to_year": 2015 })).await,
        ["Finished", "Seen Film"]
    );
    assert_eq!(
        make(json!({ "statuses": ["planned"], "max_rating": 95 })).await,
        ["Untouched"],
        "an unrated title has no rating to compare"
    );
    assert_eq!(make(json!({ "favourite": true })).await, ["Half Watched"]);
    assert_eq!(make(json!({})).await.len(), 5, "no rules is everything");

    // Sorting and limits follow the library's own keys.
    let (_, body, _) = h
        .post(
            "/api/shelves",
            smart(json!({ "sort": "rating", "order": "desc", "limit": 2 })),
            Some(&cookie),
        )
        .await;
    let id = body["shelf"]["id"].as_i64().unwrap();
    let (_, body) = h.get(&format!("/api/shelves/{id}"), Some(&cookie)).await;
    assert_eq!(names(&body), ["Untouched", "Half Watched"]);

    for bad in [
        json!({ "statuses": ["binged"] }),
        json!({ "progress": "halfway" }),
        json!({ "min_rating": 140 }),
        json!({ "kind": "book" }),
    ] {
        let (s, _, _) = h
            .post("/api/shelves", smart(bad.clone()), Some(&cookie))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (s, _, _) = h
        .post(
            "/api/shelves",
            json!({ "name": "   ", "kind": "manual" }),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a name is required");

    // A smart shelf cannot be filled by hand.
    let (s, _, _) = h
        .post(
            &format!("/api/shelves/{id}/items"),
            json!({ "kind": "tv", "tmdb_id": 101 }),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, body) = h
        .send(
            "PUT",
            &format!("/api/shelves/{id}"),
            json!({ "name": "Top two", "rules": { "kind": "movie" } }),
            &cookie,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["shelf"]["name"], json!("Top two"));
    assert_eq!(body["shelf"]["count"], json!(2));
}

/// Cache a film whose TMDB recommendations are the given (id, poster) pairs.
async fn cache_recommending(h: &Harness, id: i64, recs: &[(i64, Option<&str>)]) {
    let detail = json!({
        "id": id,
        "title": format!("Seed {id}"),
        "recommendations": { "results": recs.iter().map(|(rid, poster)| json!({
            "id": rid, "title": format!("Pick {rid}"), "poster_path": poster, "release_date": "2020-01-01"
        })).collect::<Vec<_>>() }
    });
    let key = format!(
        "/movie/{id}?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids,release_dates&include_image_language=en,null"
    );
    sqlx::query(
        "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
         VALUES (?, ?, '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
    )
    .bind(key)
    .bind(detail.to_string())
    .execute(&h.state.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn because_you_watched_skips_your_library_and_never_repeats() {
    let h = Harness::with_tmdb_key("test-key").await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    // Two seeds rated highly, one rated low (never a seed), and one tracked
    // title that the first seed recommends.
    sqlx::query(
        "INSERT INTO title (id, kind, tmdb_id, name) VALUES
            (1, 'movie', 10, 'Seed 10'), (2, 'movie', 20, 'Seed 20'),
            (3, 'movie', 30, 'Meh'), (4, 'movie', 101, 'Pick 101')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (title_id, status, rating, added_at, updated_at) VALUES
            (1, 'completed', 95, 'x', 'x'), (2, 'completed', 80, 'x', 'x'),
            (3, 'completed', 40, 'x', 'x'), (4, 'planned', NULL, 'x', 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    cache_recommending(
        &h,
        10,
        &[
            (101, Some("/p.jpg")),
            (102, None),
            (103, Some("/p.jpg")),
            (104, Some("/p.jpg")),
            (105, Some("/p.jpg")),
            (106, Some("/p.jpg")),
        ],
    )
    .await;
    // The second seed mostly repeats the first; what is left is too few for
    // a row of its own.
    cache_recommending(
        &h,
        20,
        &[
            (103, Some("/p.jpg")),
            (104, Some("/p.jpg")),
            (105, Some("/p.jpg")),
            (201, Some("/p.jpg")),
        ],
    )
    .await;

    let (status, body) = h.get("/api/home/because", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(
        rows[0]["seed"]["name"],
        json!("Seed 10"),
        "the best rated leads"
    );
    let ids: Vec<i64> = rows[0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["tmdb_id"].as_i64().unwrap())
        .collect();
    assert_eq!(
        ids,
        [103, 104, 105, 106],
        "not what you track, not what has no poster"
    );

    let plain = Harness::new().await;
    let (_, _, c) = plain
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let (_, body) = plain.get("/api/home/because", c.as_deref()).await;
    assert_eq!(body["rows"], json!([]), "no key, no rows, no error");
}

/// A database migrated from a checkout with other line endings still starts;
/// a migration whose SQL really changed is still refused.
#[tokio::test]
async fn migrations_differing_only_in_line_endings_are_accepted() {
    use sha2::{Digest, Sha384};

    let h = Harness::new().await;
    let pool = &h.state.pool;
    let migrator = sqlx::migrate!("./migrations");
    let first = migrator.iter().next().unwrap();

    // Record migration 1 as if it had been applied from the other ending.
    let other = if first.sql.contains("\r\n") {
        first.sql.replace("\r\n", "\n")
    } else {
        first.sql.replace('\n', "\r\n")
    };
    let foreign = Sha384::digest(other.as_bytes()).to_vec();
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
        .bind(&foreign)
        .bind(first.version)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        migrator.run(pool).await.is_err(),
        "without reconciling, sqlx refuses"
    );

    frog::db::heal_line_endings(pool, &migrator).await.unwrap();
    migrator
        .run(pool)
        .await
        .expect("reconciled, it starts again");

    // A real edit is not something to paper over.
    let edited = Sha384::digest(b"DROP TABLE title;").to_vec();
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
        .bind(&edited)
        .bind(first.version)
        .execute(pool)
        .await
        .unwrap();
    frog::db::heal_line_endings(pool, &migrator).await.unwrap();
    assert!(migrator.run(pool).await.is_err());
}
