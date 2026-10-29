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
            anilist: None,
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

    async fn put(
        &self,
        path: &str,
        body: Value,
        cookie: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder()
            .uri(path)
            .method("PUT")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        self.call(req.body(Body::from(body.to_string())).unwrap())
            .await
    }

    async fn delete(
        &self,
        path: &str,
        cookie: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder().uri(path).method("DELETE");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        self.call(req.body(Body::empty()).unwrap()).await
    }

    /// A DELETE carrying a body, for the ones that ask you to confirm.
    async fn delete_with(
        &self,
        path: &str,
        body: Value,
        cookie: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder()
            .uri(path)
            .method("DELETE")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        self.call(req.body(Body::from(body.to_string())).unwrap())
            .await
    }

    /// A POST that claims to come from a particular address, for the sign-in
    /// throttle: under the test harness there is no socket, so the caller is
    /// only ever named by the proxy header.
    async fn post_from(
        &self,
        path: &str,
        body: Value,
        ip: &str,
    ) -> (StatusCode, Value, Option<String>) {
        let req = Request::builder()
            .uri(path)
            .method("POST")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", ip)
            .body(Body::from(body.to_string()))
            .unwrap();
        self.call(req).await
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
        "INSERT INTO library_entry (user_id, title_id, status, rating, added_at, updated_at)
         VALUES (1, 1, 'completed', 90.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (1, 2, 'planned', NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch, source)
         VALUES (1, 1, NULL, '2026-01-02T20:00:00Z', 0, 'manual')",
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

/// The library filter ignores case and accents: "pokemon" finds "Pokémon".
/// It still only ever looks in your own library.
#[tokio::test]
async fn the_library_filter_ignores_accents_and_case() {
    let h = Harness::new().await;
    let (owner, other) = two_accounts(&h).await;

    sqlx::query(
        "INSERT INTO title (id, kind, tmdb_id, name, original_name) VALUES
            (1, 'tv', 60572, 'Pokémon', 'ポケットモンスター'),
            (2, 'tv', 31910, 'Naruto Shippūden', 'NARUTO -ナルト- 疾風伝'),
            (3, 'tv', 1396, 'Breaking Bad', 'Breaking Bad'),
            (4, 'movie', 11, '100% Wolf', NULL),
            (5, 'movie', 12, 'Pokémon: The First Movie', NULL)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    frog::db::fill_search_names(&h.state.pool).await.unwrap();

    // The owner tracks the first four; the other account has the film.
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         SELECT u.id, t.id, 'planned', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
           FROM app_user u, title t
          WHERE (u.username = 'mihai' AND t.id <= 4)
             OR (u.username = 'zjef' AND t.id = 5)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    // A title written by a binary from before search_name still matches by
    // its plain name.
    sqlx::query("UPDATE title SET search_name = NULL WHERE id = 3")
        .execute(&h.state.pool)
        .await
        .unwrap();

    let found = |body: Value| -> Vec<String> {
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["name"].as_str().unwrap().to_string())
            .collect()
    };
    for (term, expected) in [
        ("pokemon", vec!["Pokémon"]),
        ("Pok%C3%A9mon", vec!["Pokémon"]),
        ("POK%C3%89MON", vec!["Pokémon"]),
        ("shippuden", vec!["Naruto Shippūden"]),
        ("SHIPPUDEN", vec!["Naruto Shippūden"]),
        ("%E7%96%BE%E9%A2%A8%E4%BC%9D", vec!["Naruto Shippūden"]),
        ("breaking", vec!["Breaking Bad"]),
        ("100%25", vec!["100% Wolf"]),
        ("0%25", vec!["100% Wolf"]),
        ("%25", vec!["100% Wolf"]),
        ("_", vec![]),
    ] {
        let (status, body) = h.get(&format!("/api/library?q={term}"), Some(&owner)).await;
        assert_eq!(status, StatusCode::OK, "{term}: {body}");
        assert_eq!(found(body), expected, "searching {term}");
    }

    let (_, body) = h.get("/api/library?q=pokemon", Some(&other)).await;
    assert_eq!(found(body), ["Pokémon: The First Movie"], "only your own");
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

/// An owner account, for tests that seed personal rows straight into the
/// database instead of going through the API. Every personal row names its
/// owner now, so those rows need somebody to belong to.
async fn an_owner(h: &Harness) {
    sqlx::query(
        "INSERT INTO app_user (id, username, password_hash, role, created_at)
         VALUES (1, 'owner', 'x', 'owner', '2026-10-26')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
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
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, undated)
         VALUES (1, 1, NULL, '2025-12-30T20:00:00Z', 0),
                (1, 1, NULL, '2025-12-31T20:00:00Z', 0),
                (1, 2, NULL, '2026-03-02T20:00:00Z', 0),
                (1, 2, NULL, '2020-01-01T00:00:00Z', 1)",
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
    assert_eq!(all["ratings"][6], json!(1));
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
    assert_eq!(y["ratings"][6], json!(0), "only titles watched that year");
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
    an_owner(&h).await;
    sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name) VALUES
            ('movie', 1, 'Finished'), ('movie', 2, 'Planned'), ('movie', 3, 'Watched')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, finished_at, added_at, updated_at) VALUES
            (1, 1, 'completed', '2024-02-02T10:00:00Z', '2024-01-01T00:00:00Z', '2024-02-02T10:00:00Z'),
            (1, 2, 'planned', NULL, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z'),
            (1, 3, 'completed', '2024-03-03T10:00:00Z', '2024-01-01T00:00:00Z', '2024-03-03T10:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, watched_at) VALUES (1, 3, '2023-12-24T20:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    // 0006 backfilled this before personal rows had an owner, so replaying
    // that file against today's schema is not a thing that can happen. The
    // backfill it performed is what matters, written for the current columns.
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated)
         SELECT l.user_id, l.title_id, NULL, COALESCE(l.finished_at, l.updated_at),
                0, 'manual', 1
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE t.kind = 'movie'
            AND l.status = 'completed'
            AND NOT EXISTS (SELECT 1 FROM watch_event w
                             WHERE w.user_id = l.user_id AND w.title_id = l.title_id)",
    )
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
    an_owner(&h).await;
    cache_film(&h, 21, "Old Entry", "2001", &[(5, "Someone")]).await;
    sqlx::query("INSERT INTO title (kind, tmdb_id, name) VALUES ('movie', 21, 'Old Entry')")
        .execute(&h.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         VALUES (1, 1, 'completed', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
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

/// Each account picks the hour it hears about new episodes; anything
/// missing or out of range falls back to the server's FROG_NOTIFY_HOUR.
#[tokio::test]
async fn each_account_chooses_its_notification_hour() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    let pool = &h.state.pool;
    let id = |name: &'static str| async move {
        sqlx::query_as::<_, (i64,)>("SELECT id FROM app_user WHERE username = ?")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap()
            .0
    };
    let (owner_id, guest_id) = (id("mihai").await, id("zjef").await);

    assert_eq!(
        frog::jobs::notify_hour(pool, owner_id, 18).await,
        18,
        "nothing chosen"
    );

    h.put("/api/settings", json!({ "notifyHour": 7 }), Some(&owner))
        .await;
    h.put("/api/settings", json!({ "notifyHour": 99 }), Some(&guest))
        .await;
    assert_eq!(frog::jobs::notify_hour(pool, owner_id, 18).await, 7);
    assert_eq!(
        frog::jobs::notify_hour(pool, guest_id, 18).await,
        18,
        "an impossible hour falls back"
    );

    let (_, status) = h.get("/api/push", Some(&owner)).await;
    assert_eq!(status["default_hour"], json!(18));
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
    an_owner(&h).await;
    let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();

    sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name, poster_path) VALUES
            ('tv', 1, 'Followed', '/f.jpg'), ('tv', 2, 'Muted', NULL), ('tv', 3, 'Dropped', NULL)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, notify, added_at, updated_at) VALUES
            (1, 1, 'watching', 1, 'x', 'x'), (1, 2, 'watching', 0, 'x', 'x'), (1, 3, 'dropped', 1, 'x', 'x')",
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

    let due = frog::jobs::due_announcements(&h.state.pool, 1, today)
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
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at) VALUES (1, 1, 1, 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    let due = frog::jobs::due_announcements(&h.state.pool, 1, today)
        .await
        .unwrap();
    assert_eq!(
        due[0].payload()["body"],
        json!("S02E02 · Second is out today")
    );

    sqlx::query("INSERT INTO push_sent (user_id, episode_id, sent_at) VALUES (1, 2, 'x')")
        .execute(&h.state.pool)
        .await
        .unwrap();
    assert!(frog::jobs::due_announcements(&h.state.pool, 1, today)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_show_can_be_muted_from_its_library_entry() {
    let h = Harness::new().await;
    show_with_seasons(&h).await;
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         VALUES (1, 1, 'watching', 'x', 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    // The title is not in the TMDB cache, so go through the store directly.
    let patch = frog::store::LibraryPatch {
        notify: Some(false),
        ..Default::default()
    };
    let entry = frog::store::apply_patch(&h.state.pool, 1, 1, &patch)
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
        "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, added_at, updated_at) VALUES
            (1, 1, 'watching', 85.0, 1, '2026-01-01', 'x'),
            (1, 2, 'planned', 90.0, 0, '2026-01-02', 'x'),
            (1, 3, 'completed', 80.0, 0, '2026-01-03', 'x'),
            (1, 4, 'completed', 70.0, 0, '2026-01-04', 'x'),
            (1, 5, 'planned', NULL, 0, '2026-01-05', 'x')",
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
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at) VALUES
            (1, 1, 1, 'x'), (1, 3, 4, 'x'), (1, 4, NULL, 'x')",
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
        "INSERT INTO library_entry (user_id, title_id, status, rating, added_at, updated_at) VALUES
            (1, 1, 'completed', 95, 'x', 'x'), (1, 2, 'completed', 80, 'x', 'x'),
            (1, 3, 'completed', 40, 'x', 'x'), (1, 4, 'planned', NULL, 'x', 'x')",
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
    let migrator = frog::db::migrator();
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

/// After a failed update the previous image starts on a database that already
/// holds the new release's migrations. It must come up rather than refuse, or
/// a rollback leaves the site down.
#[tokio::test]
async fn an_older_binary_starts_on_a_newer_schema() {
    let h = Harness::new().await;
    let url = h.state.config.database_url.clone();

    // A migration from a release this binary has never heard of, applied.
    sqlx::query(
        "INSERT INTO _sqlx_migrations
             (version, description, success, checksum, execution_time)
         VALUES (299912312359, 'from the future', 1, X'00', 0)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    assert!(
        sqlx::migrate!("./migrations")
            .run(&h.state.pool)
            .await
            .is_err(),
        "sqlx's default refuses it"
    );
    h.state.pool.close().await;

    let pool = frog::db::connect(&url)
        .await
        .expect("starts on a schema one step ahead");
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = ?")
        .bind(299912312359_i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "and leaves the newer record alone");
}

// ---------------------------------------------------------------------------
// Isolation between accounts.
//
// Frog was single-user, so these are the tests that have to keep being true:
// nothing one account stores may be read or changed by another, on any route.
// The checks go through the HTTP surface rather than the store, because that
// is the only thing an actual attacker can reach.
// ---------------------------------------------------------------------------

/// Two accounts: the owner from first-run, and a second one that joined with
/// an invite the owner made. Returns a cookie for each.
async fn two_accounts(h: &Harness) -> (String, String) {
    let (code, _, owner) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass", "remember": true }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let owner = owner.expect("setup signs the owner in");

    let (code, invite, _) = h
        .post("/api/invites", json!({ "role": "user" }), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::OK, "{invite:?}");
    let code_text = invite["code"].as_str().expect("an invite code").to_string();

    let (code, body, guest) = h
        .post(
            "/api/auth/register",
            json!({
                "username": "zjef",
                "password": "another-long-pass",
                "remember": true,
                "invite": code_text,
            }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    let guest = guest.expect("joining signs you in");

    (owner, guest)
}

/// A film, a show with one aired episode, and the owner's own history of both.
async fn seed_owner_history(h: &Harness) {
    sqlx::query(
        "INSERT INTO title (id, kind, tmdb_id, name, runtime) VALUES
            (1, 'movie', 1, 'Private Film', 100),
            (2, 'tv', 2, 'Private Show', 30)",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO episode (id, title_id, season_number, episode_number, name, air_date)
         VALUES (1, 2, 1, 1, 'Pilot', '2026-01-01')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes,
                                    added_at, updated_at)
         VALUES (1, 1, 'completed', 90.0, 1, 'mine', '2026-01-01', '2026-01-01'),
                (1, 2, 'watching', 70.0, 0, NULL, '2026-01-01', '2026-01-01')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at)
         VALUES (1, 1, NULL, '2026-01-02T20:00:00Z'),
                (1, 2, 1, '2026-01-03T20:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO episode_state (user_id, episode_id, rating, notes, updated_at)
         VALUES (1, 1, 80.0, 'a private note', '2026-01-03')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO shelf (id, user_id, name, kind, rules_json, position, created_at)
         VALUES (1, 1, 'Owner shelf', 'manual', '{}', 0, '2026-01-01')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO shelf_item (shelf_id, title_id, position) VALUES (1, 1, 0)")
        .execute(&h.state.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_second_account_sees_none_of_the_first_accounts_data() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    seed_owner_history(&h).await;

    h.post("/api/settings", json!({}), Some(&owner)).await;
    let (code, _, _) = h
        .put(
            "/api/settings",
            json!({ "theme": "pond-secret" }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::OK);

    // The owner can see their own things, so the assertions below mean
    // something.
    let (_, mine) = h.get("/api/library", Some(&owner)).await;
    assert_eq!(mine["items"].as_array().unwrap().len(), 2);

    let (code, body) = h.get("/api/library", Some(&guest)).await;
    assert_eq!(code, StatusCode::OK);
    assert!(
        body["items"].as_array().unwrap().is_empty(),
        "the guest sees an empty library, got {body:?}"
    );

    let (_, body) = h.get("/api/library/stats", Some(&guest)).await;
    assert_eq!(body["films_watched"], json!(0));
    assert_eq!(body["episodes_watched"], json!(0));
    assert_eq!(body["minutes_watched"], json!(0));
    assert_eq!(body["mean_rating"], Value::Null);
    assert!(body["by_status"].as_object().unwrap().is_empty());

    let (_, body) = h.get("/api/stats", Some(&guest)).await;
    assert_eq!(body["totals"]["films"], json!(0));
    assert_eq!(body["totals"]["episodes"], json!(0));
    assert!(body["years"].as_array().unwrap().is_empty());

    let (_, body) = h.get("/api/home", Some(&guest)).await;
    for row in ["continuing", "waiting", "recent", "favourites"] {
        if let Some(list) = body[row].as_array() {
            assert!(list.is_empty(), "{row} leaked, got {body:?}");
        }
    }
    assert_eq!(body["hero"], Value::Null, "the hero leaked: {body:?}");

    let (_, body) = h.get("/api/diary", Some(&guest)).await;
    assert!(body["entries"].as_array().unwrap().is_empty());
    assert_eq!(body["total"], json!(0));
    assert_eq!(body["undated"], json!(0));

    let (_, body) = h.get("/api/calendar", Some(&guest)).await;
    assert!(body["entries"].as_array().unwrap().is_empty());

    let (_, body) = h.get("/api/shelves", Some(&guest)).await;
    assert!(body["shelves"].as_array().unwrap().is_empty());

    // Someone else's shelf is simply not there, by id.
    let (code, _) = h.get("/api/shelves/1", Some(&guest)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let (_, body) = h.get("/api/titles/movie/1/shelves", Some(&guest)).await;
    assert!(body["shelves"].as_array().unwrap().is_empty());

    let (_, body) = h.get("/api/export/library.json", Some(&guest)).await;
    assert!(body["titles"].as_array().unwrap().is_empty());
    assert_eq!(body["settings"], json!({}));

    // Preferences are per account, so the owner's theme is not the guest's.
    let (_, body) = h.get("/api/settings", Some(&guest)).await;
    assert_eq!(body, json!({}), "the owner's settings leaked: {body:?}");

    let (_, body) = h.get("/api/push", Some(&guest)).await;
    assert_eq!(body["devices"], json!(0));
}

#[tokio::test]
async fn a_second_account_cannot_change_the_first_accounts_data() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    seed_owner_history(&h).await;

    // Removing "the same" film only ever removes your own row.
    let (code, _, _) = h.delete("/api/titles/movie/1/library", Some(&guest)).await;
    assert!(
        code == StatusCode::OK || code == StatusCode::NOT_FOUND,
        "{code}"
    );

    // Ticking the shared episode records the guest's watch, never the owner's.
    let (code, _, _) = h
        .post(
            "/api/titles/tv/2/episodes/1/1/watch",
            json!({ "watched": true }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    // ...and un-ticking it does not clear the owner's.
    let (code, _, _) = h
        .post(
            "/api/titles/tv/2/episodes/1/1/watch",
            json!({ "watched": false }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::OK);

    // Someone else's shelf cannot be renamed, emptied or deleted.
    let (code, _, _) = h
        .put(
            "/api/shelves/1",
            json!({ "name": "Taken over" }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _, _) = h
        .post(
            "/api/shelves/1/items",
            json!({ "kind": "tv", "tmdb_id": 2 }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _, _) = h.delete("/api/shelves/1", Some(&guest)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    // Nor can their diary be rewritten.
    let (code, body, _) = h
        .post(
            "/api/diary/redate",
            json!({
                "kind": "movie",
                "tmdb_id": 1,
                "day": "2026-01-02",
                "when": "unknown",
            }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["changed"], json!(0), "the guest redated someone else");

    // The owner's rows are exactly as they were.
    let (_, mine) = h.get("/api/library", Some(&owner)).await;
    let items = mine["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "the owner lost a title: {mine:?}");
    let film = items
        .iter()
        .find(|i| i["tmdb_id"] == json!(1))
        .expect("the film is still there");
    assert_eq!(film["entry"]["rating"], json!(90.0));
    assert_eq!(film["entry"]["notes"], json!("mine"));

    let (_, diary) = h.get("/api/diary", Some(&owner)).await;
    assert_eq!(diary["total"], json!(2), "the owner's diary changed");
    assert_eq!(diary["undated"], json!(0));

    let (_, shelves) = h.get("/api/shelves", Some(&owner)).await;
    let shelves = shelves["shelves"].as_array().unwrap();
    assert_eq!(shelves.len(), 1);
    assert_eq!(shelves[0]["name"], json!("Owner shelf"));
    assert_eq!(shelves[0]["count"], json!(1));

    let (_, show) = h.get("/api/library?status=watching", Some(&owner)).await;
    let watched = show["items"][0]["watched_episodes"].clone();
    assert_eq!(watched, json!(1), "the owner's progress changed: {show:?}");
}

#[tokio::test]
async fn only_administrators_reach_the_admin_routes() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;

    let (code, _) = h.get("/api/invites", Some(&owner)).await;
    assert_eq!(code, StatusCode::OK);

    let (code, body) = h.get("/api/invites", Some(&guest)).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body:?}");

    let (code, _, _) = h
        .post("/api/invites", json!({ "role": "user" }), Some(&guest))
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN);

    // Only the owner hands out administrator accounts.
    let (code, _, _) = h
        .post("/api/invites", json!({ "role": "admin" }), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn an_invite_works_once_and_there_is_no_open_sign_up() {
    let h = Harness::new().await;
    let (owner, _) = two_accounts(&h).await;

    let (_, invite, _) = h
        .post("/api/invites", json!({ "role": "user" }), Some(&owner))
        .await;
    let code_text = invite["code"].as_str().unwrap().to_string();

    let join = |name: &str, code: String| {
        let body = json!({
            "username": name,
            "password": "yet-another-long-pass",
            "invite": code,
        });
        h.post("/api/auth/register", body, None)
    };

    let (code, body, _) = join("first", code_text.clone()).await;
    assert_eq!(code, StatusCode::OK, "{body:?}");

    // The same link a second time is refused.
    let (code, body, _) = join("second", code_text).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body:?}");

    // And a made-up code is no way in.
    let (code, _, _) = join("third", "not-a-real-invite".into()).await;
    assert_eq!(code, StatusCode::FORBIDDEN);

    // Setup stays closed now that the instance has accounts.
    let (code, _, _) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "sneaky", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_disabled_account_cannot_sign_in_or_keep_using_its_session() {
    let h = Harness::new().await;
    let (_, guest) = two_accounts(&h).await;

    let (code, _) = h.get("/api/library", Some(&guest)).await;
    assert_eq!(code, StatusCode::OK);

    sqlx::query("UPDATE app_user SET disabled = 1 WHERE username = 'zjef'")
        .execute(&h.state.pool)
        .await
        .unwrap();

    // The session stops working on the very next request, not at next sign-in.
    let (code, _) = h.get("/api/library", Some(&guest)).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    let (code, body, _) = h
        .post(
            "/api/auth/login",
            json!({ "username": "zjef", "password": "another-long-pass" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body:?}");
}

#[tokio::test]
async fn signing_in_is_throttled_per_account_and_per_address() {
    let h = Harness::new().await;
    let (_, _) = two_accounts(&h).await;

    // Hammer one account from one address until it locks.
    let mut locked = false;
    for _ in 0..12 {
        let (code, _, _) = h
            .post_from(
                "/api/auth/login",
                json!({ "username": "zjef", "password": "wrong-password" }),
                "203.0.113.1",
            )
            .await;
        if code == StatusCode::TOO_MANY_REQUESTS {
            locked = true;
            break;
        }
    }
    assert!(locked, "the account never locked");

    // That account stays locked from a fresh address: moving house does not
    // buy another eight guesses.
    let (code, _, _) = h
        .post_from(
            "/api/auth/login",
            json!({ "username": "zjef", "password": "another-long-pass" }),
            "198.51.100.7",
        )
        .await;
    assert_eq!(code, StatusCode::TOO_MANY_REQUESTS);

    // The other account, from a different address, is untouched: one target
    // cannot lock everybody out.
    let (code, body, _) = h
        .post_from(
            "/api/auth/login",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            "198.51.100.7",
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
}

/// Sign in again as someone, from a fresh device.
async fn sign_in(h: &Harness, username: &str, password: &str) -> (StatusCode, Option<String>) {
    let (code, _, cookie) = h
        .post(
            "/api/auth/login",
            json!({ "username": username, "password": password, "remember": true }),
            None,
        )
        .await;
    (code, cookie)
}

#[tokio::test]
async fn changing_the_password_signs_out_every_other_device() {
    let h = Harness::new().await;
    let (_, guest) = two_accounts(&h).await;
    let (_, laptop) = sign_in(&h, "zjef", "another-long-pass").await;
    let laptop = laptop.unwrap();

    let (code, body, _) = h
        .put(
            "/api/account/password",
            json!({ "current": "wrong-password", "password": "brand-new-password" }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{body}");

    let (code, body, _) = h
        .put(
            "/api/account/password",
            json!({ "current": "another-long-pass", "password": "short" }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{body}");

    let (code, body, _) = h
        .put(
            "/api/account/password",
            json!({ "current": "another-long-pass", "password": "brand-new-password" }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["signed_out"], json!(1));

    let (code, _) = h.get("/api/account", Some(&guest)).await;
    assert_eq!(code, StatusCode::OK, "this device stays signed in");
    let (code, _) = h.get("/api/account", Some(&laptop)).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED, "the other device is out");

    assert_eq!(
        sign_in(&h, "zjef", "another-long-pass").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        sign_in(&h, "zjef", "brand-new-password").await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn your_devices_are_listed_and_can_be_signed_out() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    let (_, second) = sign_in(&h, "zjef", "another-long-pass").await;
    let second = second.unwrap();

    let (code, body) = h.get("/api/account/sessions", Some(&guest)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let sessions = body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2, "only my own devices: {body}");
    assert_eq!(
        sessions
            .iter()
            .filter(|s| s["current"] == json!(true))
            .count(),
        1
    );
    let other = sessions
        .iter()
        .find(|s| s["current"] == json!(false))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Another account cannot end my session, even knowing its id.
    let (code, _, _) = h
        .delete(&format!("/api/account/sessions/{other}"), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _) = h.get("/api/account", Some(&second)).await;
    assert_eq!(code, StatusCode::OK);

    let (code, _, _) = h
        .delete(&format!("/api/account/sessions/{other}"), Some(&guest))
        .await;
    assert_eq!(code, StatusCode::OK);
    let (code, _) = h.get("/api/account", Some(&second)).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    // "Sign out everywhere else" spares this device.
    let (_, third) = sign_in(&h, "zjef", "another-long-pass").await;
    let (code, body, _) = h.delete("/api/account/sessions/others", Some(&guest)).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["signed_out"], json!(1));
    assert_eq!(h.get("/api/account", Some(&guest)).await.0, StatusCode::OK);
    assert_eq!(
        h.get("/api/account", third.as_deref()).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn leaving_deletes_your_data_and_only_yours() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    seed_owner_history(&h).await;

    // The guest tracks something of their own.
    let guest_id: (i64,) = sqlx::query_as("SELECT id FROM app_user WHERE username = 'zjef'")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         VALUES (?, 1, 'planned', 'x', 'x')",
    )
    .bind(guest_id.0)
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (code, _) = h
        .send(
            "DELETE",
            "/api/account",
            json!({ "password": "wrong-password" }),
            &guest,
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    let (code, body) = h
        .send(
            "DELETE",
            "/api/account",
            json!({ "password": "another-long-pass" }),
            &guest,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(
        h.get("/api/account", Some(&guest)).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM library_entry WHERE user_id = ?")
        .bind(guest_id.0)
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "the account's rows went with it");
    let (_, library) = h.get("/api/library", Some(&owner)).await;
    assert!(
        !library["items"].as_array().unwrap().is_empty(),
        "the owner's library is untouched"
    );

    // The owner cannot leave their own instance.
    let (code, _) = h
        .send(
            "DELETE",
            "/api/account",
            json!({ "password": "a-long-enough-pass" }),
            &owner,
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn administrators_manage_accounts_within_their_rank() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;

    let (code, body) = h.get("/api/admin/users", Some(&owner)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let users = body["users"].as_array().unwrap();
    assert_eq!(users[0]["role"], json!("owner"), "owner listed first");
    let owner_id = users[0]["id"].as_i64().unwrap();
    let guest_id = users[1]["id"].as_i64().unwrap();

    assert_eq!(
        h.get("/api/admin/users", Some(&guest)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        h.get("/api/admin/instance", Some(&guest)).await.0,
        StatusCode::FORBIDDEN
    );

    // Promote the guest; as an administrator they still cannot touch the owner,
    // themselves, or another administrator's rank.
    let (code, body, _) = h
        .put(
            &format!("/api/admin/users/{guest_id}"),
            json!({ "role": "admin" }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["role"], json!("admin"));

    let (code, _, _) = h
        .put(
            &format!("/api/admin/users/{owner_id}"),
            json!({ "disabled": true }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    let (code, _, _) = h
        .put(
            &format!("/api/admin/users/{guest_id}"),
            json!({ "role": "user" }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "not on yourself");

    // A third account, joined as a user: the new administrator may disable it,
    // which signs it out at once, but may not promote it.
    let (_, invite, _) = h
        .post("/api/invites", json!({ "role": "user" }), Some(&owner))
        .await;
    let (_, _, third) = h
        .post(
            "/api/auth/register",
            json!({ "username": "third", "password": "third-long-pass", "invite": invite["code"] }),
            None,
        )
        .await;
    let third = third.unwrap();
    let (_, body) = h.get("/api/admin/users", Some(&guest)).await;
    let third_id = body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == json!("third"))
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let (code, _, _) = h
        .put(
            &format!("/api/admin/users/{third_id}"),
            json!({ "role": "admin" }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "only the owner makes admins");
    let (code, _, _) = h
        .put(
            &format!("/api/admin/users/{third_id}"),
            json!({ "disabled": true }),
            Some(&guest),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        h.get("/api/account", Some(&third)).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Sign-in says the account is switched off, as it does for any disabled one.
    assert_eq!(
        sign_in(&h, "third", "third-long-pass").await.0,
        StatusCode::FORBIDDEN
    );

    let (code, body) = h.get("/api/admin/instance", Some(&owner)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["accounts"], json!(3));
    assert!(body["database_bytes"].as_i64().unwrap() > 0);
    assert_eq!(body["last_backup"], Value::Null, "no backup script here");
}

#[tokio::test]
async fn a_profile_is_only_seen_when_its_owner_turns_it_on() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    seed_owner_history(&h).await;

    // Off by default: not listed, and looking answers like a missing account.
    let (_, body) = h.get("/api/profiles", Some(&guest)).await;
    let names: Vec<_> = body["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["username"].clone())
        .collect();
    assert_eq!(names, vec![json!("zjef")], "only yourself: {body}");
    let (code, hidden) = h.get("/api/profiles/mihai", Some(&guest)).await;
    let (code_missing, missing) = h.get("/api/profiles/nobody", Some(&guest)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    assert_eq!((code, hidden), (code_missing, missing));

    let (code, _, _) = h
        .put(
            "/api/account/profile",
            json!({ "public": true }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let (code, body) = h.get("/api/profiles/MIHAI", Some(&guest)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["username"], json!("mihai"));
    assert!(body["stats"]["films"].as_i64().unwrap() >= 1, "{body}");

    // Never visible without an account on this instance.
    assert_eq!(
        h.get("/api/profiles/mihai", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

/// AniList sync merges: new titles are added, a status only moves forward,
/// your own rating, pause or drop is never undone, and a sync that changes
/// nothing says so.
#[tokio::test]
async fn a_sync_merge_never_undoes_what_you_did_in_frog() {
    let h = Harness::new().await;
    let (owner, _) = two_accounts(&h).await;
    for (id, name) in [(31, "Rated Film"), (32, "Paused Film"), (33, "New Film")] {
        cache_film(&h, id, name, "2020", &[]).await;
    }
    let put = |id: i64, body: Value| {
        let owner = owner.clone();
        let h = &h;
        async move {
            h.put(
                &format!("/api/titles/movie/{id}/library"),
                body,
                Some(&owner),
            )
            .await
        }
    };
    put(31, json!({ "status": "watching", "rating": 90.0 })).await;
    put(32, json!({ "status": "paused" })).await;

    let incoming = json!({
        "on_conflict": "merge",
        "items": [
            { "kind": "movie", "tmdb_id": 31, "status": "completed", "rating": 50.0 },
            { "kind": "movie", "tmdb_id": 32, "status": "completed", "rating": 70.0 },
            { "kind": "movie", "tmdb_id": 33, "status": "planned", "rating": null }
        ]
    });
    let (status, body, _) = h
        .post("/api/import/apply", incoming.clone(), Some(&owner))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["added"], json!(1), "{body}");
    assert_eq!(body["updated"], json!(2), "{body}");

    let entry = |id: i64| {
        let owner = owner.clone();
        let h = &h;
        async move {
            let (_, b) = h
                .get(&format!("/api/titles/movie/{id}"), Some(&owner))
                .await;
            b["item"]["entry"].clone()
        }
    };
    let rated = entry(31).await;
    assert_eq!(rated["status"], json!("completed"), "moved forward");
    assert_eq!(rated["rating"], json!(90.0), "your own rating stays");
    let paused = entry(32).await;
    assert_eq!(paused["status"], json!("paused"), "a pause is yours");
    assert_eq!(paused["rating"], json!(70.0), "an empty rating is filled");

    // The same list again changes nothing, and says so.
    let (_, body, _) = h.post("/api/import/apply", incoming, Some(&owner)).await;
    assert_eq!(body["updated"], json!(0), "{body}");
    assert_eq!(body["skipped"], json!(3), "{body}");
}

/// The stats add up to the library you see: a title taken out of it leaves
/// the numbers, and a finished film imported without a date still counts.
#[tokio::test]
async fn stats_only_count_what_is_in_the_library() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "zjef", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();
    cache_film(&h, 41, "Kept Film", "2001", &[]).await;
    cache_film(&h, 42, "Removed Film", "2002", &[]).await;

    let (status, body, _) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [
                { "kind": "movie", "tmdb_id": 41, "status": "completed" },
                { "kind": "movie", "tmdb_id": 42, "status": "completed" }
            ]}),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (_, stats) = h.get("/api/stats", Some(&cookie)).await;
    assert_eq!(
        stats["totals"]["films"],
        json!(2),
        "undated finished films count"
    );
    let (_, diary) = h.get("/api/diary", Some(&cookie)).await;
    assert_eq!(diary["entries"], json!([]), "but stay out of the diary");

    h.delete("/api/titles/movie/42/library", Some(&cookie))
        .await;
    let (_, stats) = h.get("/api/stats", Some(&cookie)).await;
    assert_eq!(stats["totals"]["films"], json!(1), "{}", stats["totals"]);
    assert_eq!(stats["totals"]["titles"], json!(1));
    let (_, library) = h.get("/api/library/stats", Some(&cookie)).await;
    assert_eq!(library["films_watched"], json!(1), "{library}");
}

/// Administrators delete accounts within their rank, and everything personal
/// goes with the account.
#[tokio::test]
async fn administrators_delete_accounts_within_their_rank() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    let (_, body) = h.get("/api/admin/users", Some(&owner)).await;
    let users = body["users"].as_array().unwrap().clone();
    let owner_id = users[0]["id"].as_i64().unwrap();
    let guest_id = users[1]["id"].as_i64().unwrap();

    // The guest has a shelf and a setting of their own.
    h.post("/api/shelves", json!({ "name": "Mine" }), Some(&guest))
        .await;
    h.put("/api/settings", json!({ "theme": "dark" }), Some(&guest))
        .await;

    // A user cannot delete anyone; nobody deletes themselves or the owner here.
    let (code, _, _) = h
        .delete(&format!("/api/admin/users/{owner_id}"), Some(&guest))
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    let (code, _, _) = h
        .delete(&format!("/api/admin/users/{owner_id}"), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "not yourself");

    // An administrator cannot delete another administrator; the owner can.
    h.put(
        &format!("/api/admin/users/{guest_id}"),
        json!({ "role": "admin" }),
        Some(&owner),
    )
    .await;
    let (_, invite, _) = h
        .post("/api/invites", json!({ "role": "admin" }), Some(&owner))
        .await;
    let (_, _, third) = h
        .post(
            "/api/auth/register",
            json!({ "username": "third", "password": "third-long-pass", "invite": invite["code"] }),
            None,
        )
        .await;
    assert!(third.is_some());
    let (_, body) = h.get("/api/admin/users", Some(&owner)).await;
    let third_id = body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == json!("third"))
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let (code, _, _) = h
        .delete(&format!("/api/admin/users/{third_id}"), Some(&guest))
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "only the owner removes admins");

    let (code, body, _) = h
        .delete(&format!("/api/admin/users/{guest_id}"), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(
        h.get("/api/account", Some(&guest)).await.0,
        StatusCode::UNAUTHORIZED,
        "signed out at once"
    );
    for table in ["shelf", "app_setting", "session"] {
        let (left,): (i64,) =
            sqlx::query_as(&format!("SELECT COUNT(*) FROM {table} WHERE user_id = ?"))
                .bind(guest_id)
                .fetch_one(&h.state.pool)
                .await
                .unwrap();
        assert_eq!(left, 0, "{table} rows went with the account");
    }
    let (code, _, _) = h
        .delete(&format!("/api/admin/users/{guest_id}"), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

/// A profile of one's own: name, about, pins and pictures, shown only where
/// the profile itself may be seen.
#[tokio::test]
async fn profiles_carry_a_name_an_about_pins_and_pictures() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    cache_film(&h, 51, "Pinned Film", "2003", &[]).await;
    h.put(
        "/api/titles/movie/51/library",
        json!({ "status": "completed", "rating": 90.0 }),
        Some(&owner),
    )
    .await;

    let (code, body, _) = h
        .put(
            "/api/account/profile",
            json!({
                "display_name": "  Mihai  ",
                "bio": "Films first.",
                "banner_backdrop": "/abc123.jpg",
                "pins": [{ "kind": "movie", "tmdb_id": 51 }]
            }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["display_name"], json!("Mihai"));

    let (code, _, _) = h
        .put(
            "/api/account/profile",
            json!({ "bio": "x".repeat(601) }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    let (code, _, _) = h
        .put(
            "/api/account/profile",
            json!({ "banner_backdrop": "https://evil.example/x.jpg" }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    // A real PNG goes in; an SVG pretending to be one does not.
    let png: Vec<u8> = [&b"\x89PNG\r\n\x1a\n"[..], &[0u8; 32]].concat();
    let put_image = |bytes: Vec<u8>, cookie: String| {
        let h = &h;
        async move {
            let req = axum::http::Request::builder()
                .uri("/api/account/images/avatar")
                .method("PUT")
                .header(header::COOKIE, cookie)
                .header(header::CONTENT_TYPE, "image/png")
                .body(Body::from(bytes))
                .unwrap();
            h.call(req).await.0
        }
    };
    assert_eq!(put_image(png.clone(), owner.clone()).await, StatusCode::OK);
    assert_eq!(
        put_image(b"<svg onload=alert(1)>".to_vec(), owner.clone()).await,
        StatusCode::BAD_REQUEST
    );

    let (_, profile) = h.get("/api/profiles/mihai", Some(&owner)).await;
    assert_eq!(profile["display_name"], json!("Mihai"));
    assert_eq!(profile["bio"], json!("Films first."));
    assert_eq!(profile["pinned"][0]["tmdb_id"], json!(51));
    assert_eq!(profile["stats"]["films"], json!(1));
    let avatar = profile["avatar"]
        .as_str()
        .expect("an avatar address")
        .to_string();
    let (code, _) = h.get(&avatar, Some(&owner)).await;
    assert_eq!(code, StatusCode::OK);

    // Private: the other account sees neither the profile nor its picture.
    assert_eq!(
        h.get("/api/profiles/mihai", Some(&guest)).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(h.get(&avatar, Some(&guest)).await.0, StatusCode::NOT_FOUND);
    h.put(
        "/api/account/profile",
        json!({ "public": true }),
        Some(&owner),
    )
    .await;
    assert_eq!(h.get(&avatar, Some(&guest)).await.0, StatusCode::OK);
    assert_eq!(h.get(&avatar, None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn emptying_a_library_takes_only_your_own() {
    let h = Harness::new().await;
    let (owner, guest) = two_accounts(&h).await;
    seed_owner_history(&h).await;

    // Give the guest something of their own, so "it emptied everything"
    // and "it emptied the right account" are different results.
    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         VALUES (2, 1, 'watching', 'x', 'x')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at)
         VALUES (2, 2, 1, '2026-02-02T20:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    // The phrase has to be typed, and the password has to be right.
    let (code, _, _) = h
        .delete_with(
            "/api/account/library",
            json!({ "password": "a-long-enough-pass", "confirm": "yes" }),
            Some(&owner),
        )
        .await;
    assert_eq!(
        code,
        StatusCode::BAD_REQUEST,
        "a vague confirmation is refused"
    );

    let (code, _, _) = h
        .delete_with(
            "/api/account/library",
            json!({ "password": "wrong-password", "confirm": "delete my library" }),
            Some(&owner),
        )
        .await;
    assert_eq!(
        code,
        StatusCode::BAD_REQUEST,
        "the wrong password is refused"
    );

    // Nothing has gone yet.
    let (_, before) = h.get("/api/library", Some(&owner)).await;
    assert_eq!(before["items"].as_array().unwrap().len(), 2);

    let (code, body, _) = h
        .delete_with(
            "/api/account/library",
            json!({ "password": "a-long-enough-pass", "confirm": "Delete My Library" }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    assert_eq!(body["titles"], json!(2));
    assert_eq!(body["watches"], json!(2));
    assert_eq!(body["shelves"], json!(1));

    // The owner is empty...
    let (_, mine) = h.get("/api/library", Some(&owner)).await;
    assert!(mine["items"].as_array().unwrap().is_empty());
    let (_, diary) = h.get("/api/diary", Some(&owner)).await;
    assert_eq!(diary["total"], json!(0));
    let (_, shelves) = h.get("/api/shelves", Some(&owner)).await;
    assert!(shelves["shelves"].as_array().unwrap().is_empty());

    // ...and still signed in, with the account intact.
    let (code, status) = h.get("/api/auth/status", Some(&owner)).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(status["authenticated"], json!(true));

    // The guest kept everything.
    let (_, theirs) = h.get("/api/library", Some(&guest)).await;
    assert_eq!(
        theirs["items"].as_array().unwrap().len(),
        1,
        "the guest lost a title: {theirs:?}"
    );
    let (_, their_diary) = h.get("/api/diary", Some(&guest)).await;
    assert_eq!(their_diary["total"], json!(1), "the guest lost a watch");
}

/// A sign-in that does not match says so in words a person can act on, and
/// the same words whether the username or the password was wrong.
#[tokio::test]
async fn a_wrong_sign_in_says_so_plainly() {
    let h = Harness::new().await;
    h.post(
        "/api/auth/setup",
        json!({ "username": "zjef", "password": "a-long-enough-pass" }),
        None,
    )
    .await;

    let (code, wrong_password, _) = h
        .post(
            "/api/auth/login",
            json!({ "username": "zjef", "password": "not-the-password" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, wrong_name, _) = h
        .post(
            "/api/auth/login",
            json!({ "username": "nobody", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong_password["error"],
        json!("That username and password don't match.")
    );
    assert_eq!(
        wrong_password, wrong_name,
        "never says which part was wrong"
    );
}

/// One search finds people, studios and characters besides titles: people
/// and studios from TMDB, characters from the casts of titles already looked
/// up. Studios named exactly that come first.
#[tokio::test]
async fn search_finds_people_studios_and_characters() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    let cache = |key: String, body: Value| {
        let pool = h.state.pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
                 VALUES (?, ?, '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
            )
            .bind(key)
            .bind(body.to_string())
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    cache(
        "/search/person?lang=en-US&query=pond&include_adult=false".into(),
        json!({ "results": [{ "id": 7, "name": "Pond Person", "known_for_department": "Acting",
                              "known_for": [{ "title": "Pond Movie" }] }] }),
    )
    .await;
    cache(
        "/search/company?lang=en-US&query=pond".into(),
        json!({ "results": [
            { "id": 2, "name": "Pond Pictures Holding", "logo_path": null },
            { "id": 1, "name": "Pond", "logo_path": "/logo.png", "origin_country": "BE" }
        ] }),
    )
    .await;
    cache(
        "/movie/11?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids,release_dates&include_image_language=en,null".into(),
        json!({ "id": 11, "title": "Lily", "popularity": 5.0, "poster_path": "/lily.jpg",
                "credits": { "cast": [
                    { "id": 30, "name": "An Actor", "character": "Captain Pond", "profile_path": "/a.jpg" },
                    { "id": 31, "name": "Someone", "character": "Self" }
                ] } }),
    )
    .await;

    let (status, found) = h.get("/api/search/more?q=pond", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["people"][0]["name"], json!("Pond Person"));
    assert_eq!(found["people"][0]["known_for"][0], json!("Pond Movie"));
    assert_eq!(
        found["studios"][0]["name"],
        json!("Pond"),
        "exact name first"
    );
    assert_eq!(found["studios"].as_array().unwrap().len(), 2);
    let characters = found["characters"].as_array().unwrap();
    assert_eq!(characters.len(), 1, "{found}");
    assert_eq!(characters[0]["name"], json!("Captain Pond"));
    assert_eq!(characters[0]["actor"]["id"], json!(30));
    assert_eq!(characters[0]["title"]["tmdb_id"], json!(11));

    // Signed out: nothing.
    let (status, _) = h.get("/api/search/more?q=pond", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Marking a whole show leaves specials alone unless they are named: a
/// finished show means its seasons, and the specials seen are chosen.
#[tokio::test]
async fn marking_a_show_ticks_only_the_specials_chosen() {
    let h = Harness::new().await;
    let cookie = show_with_seasons(&h).await;
    sqlx::query(
        "INSERT INTO episode (title_id, season_number, episode_number, name, air_date)
         VALUES (1, 0, 2, 'Another special', '2015-01-01')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (status, body, _) = h
        .post(
            "/api/titles/tv/48866/episodes/watch",
            json!({ "watched": true, "when": "unknown", "specials": [2] }),
            Some(&cookie),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ticked: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT e.season_number, e.episode_number FROM watch_event w
           JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = 1 AND e.season_number = 0",
    )
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(ticked, vec![(0, 2)], "only the special that was chosen");
}

/// "Complete automatically" decides whether ticking the last aired episode
/// completes the show; with it off the show stays where it was.
#[tokio::test]
async fn completing_on_the_last_episode_follows_the_setting() {
    let h = Harness::new().await;
    let cookie = show_with_seasons(&h).await;
    // Only aired episodes, so ticking them all finishes the show.
    sqlx::query("DELETE FROM episode WHERE air_date > date('now')")
        .execute(&h.state.pool)
        .await
        .unwrap();
    let mark_all = json!({ "watched": true, "when": "unknown" });

    let (status, _) = h
        .send(
            "PUT",
            "/api/settings",
            json!({ "autoComplete": false }),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body, _) = h
        .post(
            "/api/titles/tv/48866/episodes/watch",
            mark_all.clone(),
            Some(&cookie),
        )
        .await;
    assert_eq!(body["item"]["entry"]["status"], json!("watching"), "{body}");

    let (status, _) = h
        .send(
            "PUT",
            "/api/settings",
            json!({ "autoComplete": true }),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body, _) = h
        .post(
            "/api/titles/tv/48866/episodes/watch",
            mark_all,
            Some(&cookie),
        )
        .await;
    assert_eq!(
        body["item"]["entry"]["status"],
        json!("completed"),
        "{body}"
    );
}

/// TMDB matches whole words, so a few letters of a title this Frog already
/// knows find it anyway, ahead of TMDB's own results.
#[tokio::test]
async fn search_finds_known_titles_by_their_first_letters() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();
    sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name, release_date) VALUES ('movie', 157336, 'Interstellar', '2014-11-05')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
         VALUES ('/search/multi?lang=en-US&query=inter&page=1&include_adult=false', ?,
                 '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
    )
    .bind(
        json!({ "results": [
            { "media_type": "movie", "id": 1, "title": "Inter-Pol", "release_date": "1967-01-01" }
        ] })
        .to_string(),
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (status, body) = h.get("/api/search?q=inter", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let names: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    assert_eq!(names, ["Interstellar", "Inter-Pol"]);
}

/// Which part of the banner shows is kept, and only as a real position.
#[tokio::test]
async fn the_banner_position_is_kept_and_checked() {
    let h = Harness::new().await;
    let (_, _, cookie) = h
        .post(
            "/api/auth/setup",
            json!({ "username": "mihai", "password": "a-long-enough-pass" }),
            None,
        )
        .await;
    let cookie = cookie.unwrap();

    let (status, _) = h
        .send(
            "PUT",
            "/api/account/profile",
            json!({ "banner_position": "50% 22.5%" }),
            &cookie,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, account) = h.get("/api/account", Some(&cookie)).await;
    assert_eq!(account["banner_position"], json!("50% 22.5%"));

    for bad in ["50%", "50% 120%", "url(x) 1%", "50px 10px"] {
        let (status, _) = h
            .send(
                "PUT",
                "/api/account/profile",
                json!({ "banner_position": bad }),
                &cookie,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
}
