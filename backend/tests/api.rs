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
