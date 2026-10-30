//! Imports and exports end to end: which TMDB title a row means, what each
//! conflict mode does to a library that already has the title, how watches
//! land, and the exact shape of both export files. TMDB is answered from
//! the cache, so nothing here touches the network.

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

impl Harness {
    /// Frog's id for a title as a provider names it (see `common::title_id`).
    async fn tid(&self, kind: &str, id: i64) -> i64 {
        common::title_id(&self.state, kind, id).await
    }

    async fn new() -> Self {
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
            notify_hour: 18,
            anilist: None,
            mdblist: None,
        };
        let pool = frog::db::connect(&url).await.expect("database");
        let state = AppState::new(pool, config);
        common::add_title_seed(&state).await;
        Self { state, _dir: dir }
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, String, Option<String>) {
        let res = frog::routes::router(self.state.clone())
            .oneshot(req)
            .await
            .expect("response");
        let status = res.status();
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).to_string());
        let bytes = res.into_body().collect().await.expect("body").to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned(), cookie)
    }

    async fn post(&self, path: &str, body: Value, cookie: Option<&str>) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .uri(path)
            .method("POST")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        let (status, text, _) = self
            .send(req.body(Body::from(body.to_string())).unwrap())
            .await;
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn put(&self, path: &str, body: Value, cookie: &str) -> StatusCode {
        let req = Request::builder()
            .uri(path)
            .method("PUT")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, cookie)
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(req).await.0
    }

    async fn get_text(&self, path: &str, cookie: &str) -> (StatusCode, String) {
        let req = Request::builder()
            .uri(path)
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap();
        let (status, text, _) = self.send(req).await;
        (status, text)
    }

    /// The owner, signed in.
    async fn owner(&self) -> String {
        let req = Request::builder()
            .uri("/api/auth/setup")
            .method("POST")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "username": "mihai", "password": "a-long-enough-pass" }).to_string(),
            ))
            .unwrap();
        let (status, body, cookie) = self.send(req).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        cookie.expect("setup signs the owner in")
    }

    async fn cache(&self, key: &str, body: Value) {
        sqlx::query(
            "INSERT OR REPLACE INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
             VALUES (?, ?, '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
        )
        .bind(key)
        .bind(body.to_string())
        .execute(&self.state.pool)
        .await
        .unwrap();
    }

    /// What TMDB's multi search answers for `query`.
    async fn cache_search(&self, query: &str, results: Value) {
        let key = format!("/search/multi?lang=en-US&query={query}&page=1&include_adult=false");
        self.cache(&key, json!({ "results": results })).await;
    }

    async fn cache_film(&self, id: i64, name: &str, year: &str) {
        let key = format!(
            "/movie/{id}?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids,release_dates&include_image_language=en,null,ja"
        );
        self.cache(
            &key,
            json!({ "id": id, "title": name, "release_date": format!("{year}-05-01"), "runtime": 100 }),
        )
        .await;
    }

    /// A show whose seasons are `(number, name, episodes)`, all aired.
    async fn cache_show(&self, id: i64, name: &str, seasons: &[(i64, &str, i64)]) {
        let key = format!(
            "/tv/{id}?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids,content_ratings&include_image_language=en,null,ja"
        );
        let detail = json!({
            "id": id,
            "name": name,
            "first_air_date": "2015-01-01",
            "number_of_seasons": seasons.len(),
            "seasons": seasons.iter().map(|(s, n, count)| json!({
                "season_number": s, "name": n, "episode_count": count
            })).collect::<Vec<_>>(),
        });
        self.cache(&key, detail).await;
        for (s, _, count) in seasons {
            let episodes: Vec<Value> = (1..=*count)
                .map(|e| {
                    json!({
                        "season_number": s, "episode_number": e,
                        "name": format!("Chapter {s}.{e}"), "air_date": "2015-01-01"
                    })
                })
                .collect();
            self.cache(
                &format!("/tv/{id}/season/{s}?lang=en-US"),
                json!({ "season_number": s, "episodes": episodes }),
            )
            .await;
        }
    }

    async fn one<T>(&self, sql: &str) -> T
    where
        T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin,
    {
        sqlx::query_as(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }
}

fn film(id: i64, name: &str, year: i64) -> Value {
    json!({ "media_type": "movie", "id": id, "title": name, "release_date": format!("{year}-01-01") })
}

fn show(id: i64, name: &str, year: i64, genres: &[i64]) -> Value {
    json!({ "media_type": "tv", "id": id, "name": name,
            "first_air_date": format!("{year}-01-01"), "genre_ids": genres })
}

/// A row as the import wizard sends it, with everything optional left out.
fn row(key: &str, title: &str, extra: Value) -> Value {
    let mut r = json!({ "key": key, "title": title });
    for (k, v) in extra.as_object().unwrap() {
        r[k] = v.clone();
    }
    r
}

/// The result for one key, out of a match answer.
fn result<'a>(body: &'a Value, key: &str) -> &'a Value {
    body["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == key)
        .unwrap_or_else(|| panic!("no result for {key}: {body}"))
}

/// Ids beat names; a name prefers the same year, then the same letters, and
/// says when it had to guess. Rows from an anime tracker prefer animation,
/// and two shows of one name are told apart by the season the row is about.
#[tokio::test]
async fn matching_follows_ids_then_names_and_says_why() {
    let h = Harness::new().await;
    let owner = h.owner().await;

    h.cache_film(949, "Heat", "1995").await;
    h.cache(
        "/find/tt0113277?lang=en-US&external_source=imdb_id",
        json!({ "movie_results": [film(949, "Heat", 1995)], "tv_results": [] }),
    )
    .await;
    h.cache_search(
        "Heat",
        json!([film(1001, "Heat", 1986), film(949, "Heat", 1995)]),
    )
    .await;
    h.cache_search("Heet", json!([film(949, "Heat", 1995)]))
        .await;
    h.cache_search("Nothing At All", json!([])).await;
    h.cache_search("Shingeki", json!([])).await;
    h.cache_search(
        "Attack on Titan",
        json!([
            show(2001, "Attack on Titan", 2024, &[18]),
            show(1429, "Attack on Titan", 2013, &[16, 18])
        ]),
    )
    .await;
    h.cache_search(
        "One Piece",
        json!([
            show(3001, "One Piece", 2023, &[10759]),
            show(37854, "One Piece", 1999, &[16])
        ]),
    )
    .await;
    h.cache_show(3001, "One Piece", &[(1, "Season 1", 8)]).await;
    h.cache_show(
        37854,
        "One Piece",
        &[(1, "East Blue", 61), (2, "Alabasta", 74)],
    )
    .await;
    h.cache_show(
        1429,
        "Attack on Titan",
        &[(1, "Season 1", 25), (2, "Season 2", 12)],
    )
    .await;

    // Already in the library: the match says so.
    assert_eq!(
        h.put(
            &format!("/api/t/{}/library", h.tid("movie", 949).await),
            json!({ "status": "watching" }),
            &owner
        )
        .await,
        StatusCode::OK
    );

    let (status, body) = h
        .post(
            "/api/import/match",
            json!({ "items": [
                row("by-tmdb", "whatever", json!({ "tmdb_id": 949, "kind": "movie" })),
                row("by-imdb", "whatever", json!({ "imdb_id": "tt0113277" })),
                row("same-year", "Heat", json!({ "year": 1995 })),
                row("year-off", "Heat", json!({ "year": 2001 })),
                row("close", "Heet", json!({})),
                row("none", "Nothing At All", json!({})),
                row("alt", "Shingeki", json!({ "alt_titles": ["Attack on Titan"], "anime": true, "season": 2 })),
                row("season-name", "One Piece", json!({ "season_name": "Alabasta Arc" })),
            ]}),
            Some(&owner),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    for key in ["by-tmdb", "by-imdb", "same-year"] {
        let r = result(&body, key);
        assert_eq!(r["matched"]["tmdb_id"], json!(949), "{key}: {r}");
        assert_eq!(r["note"], Value::Null, "{key}: {r}");
        assert_eq!(r["already_tracked"], json!("watching"), "{key}: {r}");
    }
    assert_eq!(
        result(&body, "same-year")["candidates"][0]["tmdb_id"],
        json!(1001)
    );

    let off = result(&body, "year-off");
    assert_eq!(off["matched"]["tmdb_id"], json!(1001), "{off}");
    assert_eq!(off["note"], json!("Your file says 2001, TMDB says 1986"));
    assert_eq!(off["already_tracked"], Value::Null);

    let close = result(&body, "close");
    assert_eq!(close["matched"]["tmdb_id"], json!(949), "{close}");
    assert_eq!(close["note"], json!("Closest match by name"));

    let none = result(&body, "none");
    assert_eq!(none["matched"], Value::Null);
    assert_eq!(none["note"], json!("No match found"));

    let alt = result(&body, "alt");
    assert_eq!(
        alt["matched"]["tmdb_id"],
        json!(1429),
        "animation first: {alt}"
    );
    assert_eq!(alt["searched"], json!("Attack on Titan"));
    assert_eq!(alt["season"], json!(2));
    assert_eq!(alt["last_season"], json!(2));
    assert_eq!(
        alt["seasons"],
        json!([
            { "number": 1, "name": "Season 1", "episodes": 25 },
            { "number": 2, "name": "Season 2", "episodes": 12 }
        ])
    );

    let named = result(&body, "season-name");
    assert_eq!(
        named["matched"]["tmdb_id"],
        json!(37854),
        "the one with that season: {named}"
    );
    assert_eq!(named["note"], Value::Null);
}

/// A season the show does not have is pointed out, and the episode names the
/// row carries are found across the show's seasons instead.
#[tokio::test]
async fn matching_places_episodes_by_name_when_the_season_is_in_doubt() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_search("Dark", json!([show(70523, "Dark", 2017, &[18])]))
        .await;
    h.cache_show(70523, "Dark", &[(1, "Season 1", 3), (2, "Season 2", 3)])
        .await;

    let (status, body) = h
        .post(
            "/api/import/match",
            json!({ "items": [row("dark", "Dark", json!({
                "kind": "tv", "season": 5,
                "episodes": ["Chapter 2.3", "Something Else", "Dark: Chapter 1.1"]
            }))]}),
            Some(&owner),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let r = result(&body, "dark");
    assert_eq!(
        r["note"],
        json!("TMDB lists no season 5 for this show; check the match")
    );
    assert_eq!(
        r["placed"],
        json!([
            { "index": 0, "season": 2, "episode": 3 },
            { "index": 2, "season": 1, "episode": 1 }
        ])
    );
}

/// A row that knows its season has its episodes found there and nowhere else,
/// and results keep the order the rows came in, however they were looked up.
#[tokio::test]
async fn a_numbered_season_is_searched_for_its_episode_names() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_search("Dark", json!([show(70523, "Dark", 2017, &[18])]))
        .await;
    h.cache_show(70523, "Dark", &[(1, "Season 1", 3), (2, "Season 2", 3)])
        .await;

    let mut items: Vec<Value> = (0..20)
        .map(|i| row(&format!("row-{i}"), "Dark", json!({ "kind": "tv" })))
        .collect();
    items[7] = row(
        "row-7",
        "Dark",
        json!({
            "kind": "tv", "season": 2,
            "episodes": ["Chapter 2.3", "Chapter 1.1", "Not On TMDB"]
        }),
    );
    let (status, body) = h
        .post("/api/import/match", json!({ "items": items }), Some(&owner))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        result(&body, "row-7")["placed"],
        json!([{ "index": 0, "season": 2, "episode": 3 }])
    );
    let keys: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    let wanted: Vec<String> = (0..20).map(|i| format!("row-{i}")).collect();
    assert_eq!(keys, wanted);
}

/// An import that knows the day fills in one that did not, on the same watch,
/// and leaves a dated watch alone. Until then the show has no last-watched
/// day, rather than the day of the import.
#[tokio::test]
async fn a_dated_import_fills_in_an_undated_one() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_show(70523, "Dark", &[(1, "Season 1", 3)]).await;

    let (_, body) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [
                { "kind": "tv", "tmdb_id": 70523, "status": "watching", "season": 1, "progress": 3 }
            ]}),
            Some(&owner),
        )
        .await;
    assert_eq!(body["added"], json!(1), "{body}");
    let (last,): (Option<String>,) = h
        .one("SELECT MAX(watched_at) FROM watch_event WHERE undated = 0")
        .await;
    assert_eq!(last, None);

    let (_, body) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [{
                "kind": "tv", "tmdb_id": 70523, "status": "watching",
                "episodes": [
                    { "season": 1, "episode": 1, "watched_at": "2025-03-01T20:00:00Z" },
                    { "season": 1, "episode": 2, "watched_at": null }
                ]
            }]}),
            Some(&owner),
        )
        .await;
    assert_eq!(body["updated"], json!(1), "{body}");

    let watches: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT e.episode_number, w.undated, substr(w.watched_at, 1, 10)
           FROM watch_event w JOIN episode e ON e.id = w.episode_id
          ORDER BY e.episode_number",
    )
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(watches.len(), 3, "no episode is ticked twice: {watches:?}");
    assert_eq!(watches[0], (1, 0, "2025-03-01".to_string()));
    assert_eq!((watches[1].0, watches[1].1), (2, 1));
    assert_eq!((watches[2].0, watches[2].1), (3, 1));

    let (status, library) = h.get_text("/api/library", &owner).await;
    assert_eq!(status, StatusCode::OK, "{library}");
    let library: Value = serde_json::from_str(&library).unwrap();
    assert_eq!(
        library["items"][0]["last_watched_at"],
        json!("2025-03-01T20:00:00Z")
    );

    // A dated watch stays on its day whatever a later file says.
    h.post(
        "/api/import/apply",
        json!({ "on_conflict": "overwrite", "items": [{
            "kind": "tv", "tmdb_id": 70523, "status": "watching",
            "episodes": [{ "season": 1, "episode": 1, "watched_at": "2026-01-01T20:00:00Z" }]
        }]}),
        Some(&owner),
    )
    .await;
    let (kept,): (String,) = h
        .one(
            "SELECT substr(w.watched_at, 1, 10) FROM watch_event w
               JOIN episode e ON e.id = w.episode_id WHERE e.episode_number = 1",
        )
        .await;
    assert_eq!(kept, "2025-03-01");
}

#[tokio::test]
async fn a_match_or_import_batch_is_capped() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    let rows: Vec<Value> = (0..201)
        .map(|i| row(&i.to_string(), "x", json!({})))
        .collect();
    let (status, _) = h
        .post("/api/import/match", json!({ "items": rows }), Some(&owner))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let items: Vec<Value> = (0..201)
        .map(|i| json!({ "kind": "movie", "tmdb_id": i }))
        .collect();
    let (status, _) = h
        .post("/api/import/apply", json!({ "items": items }), Some(&owner))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Skip leaves a tracked title exactly as it is; overwrite lets the file
/// win, rating included, even when the file has none.
#[tokio::test]
async fn skip_and_overwrite_do_what_they_say() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_film(10, "Kept", "2001").await;
    h.cache_film(11, "Replaced", "2002").await;
    for id in [10, 11] {
        h.put(
            &format!("/api/t/{}/library", h.tid("movie", id).await),
            json!({ "status": "dropped", "rating": 40.0, "favourite": true }),
            &owner,
        )
        .await;
    }

    let item = |id: i64| json!({ "kind": "movie", "tmdb_id": id, "status": "planned" });
    let (_, body) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "skip", "items": [item(10), { "kind": "book", "tmdb_id": 1 }] }),
            Some(&owner),
        )
        .await;
    assert_eq!(
        body,
        json!({ "added": 0, "updated": 0, "skipped": 1,
                "failed": [{ "tmdb_id": 1, "error": "unknown kind" }] })
    );

    let (_, body) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [item(11)] }),
            Some(&owner),
        )
        .await;
    assert_eq!(body["updated"], json!(1), "{body}");

    let rows: Vec<(i64, String, Option<f64>, bool)> = sqlx::query_as(
        "SELECT (SELECT tmdb_id FROM title_tmdb WHERE title_id = t.id), l.status, l.rating, l.favourite
           FROM library_entry l JOIN title t ON t.id = l.title_id ORDER BY (SELECT tmdb_id FROM title_tmdb WHERE title_id = t.id)",
    )
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (10, "dropped".into(), Some(40.0), true),
            (11, "planned".into(), None, false)
        ]
    );
}

/// Episodes the source names are ticked one by one: on their own day when it
/// says, undated when not, and an episode already watched keeps its watch.
#[tokio::test]
async fn exact_episodes_land_on_their_day_and_old_watches_stay() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_show(70523, "Dark", &[(1, "Season 1", 3), (2, "Season 2", 3)])
        .await;
    let (_, body) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [{
                "kind": "tv", "tmdb_id": 70523, "status": "watching",
                "episodes": [{ "season": 1, "episode": 1, "watched_at": "2020-01-01T20:00:00Z" }]
            }]}),
            Some(&owner),
        )
        .await;
    assert_eq!(body["added"], json!(1), "{body}");

    let (_, body) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [{
                "kind": "tv", "tmdb_id": 70523, "status": "watching",
                "episodes": [
                    { "season": 1, "episode": 1, "watched_at": "2024-06-06T20:00:00Z" },
                    { "season": 2, "episode": 2, "watched_at": null }
                ]
            }]}),
            Some(&owner),
        )
        .await;
    assert_eq!(body["updated"], json!(1), "{body}");

    let watches: Vec<(i64, i64, String, i64, String)> = sqlx::query_as(
        "SELECT e.season_number, e.episode_number, substr(w.watched_at, 1, 10), w.undated, w.source
           FROM watch_event w JOIN episode e ON e.id = w.episode_id
          ORDER BY e.season_number, e.episode_number",
    )
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(watches.len(), 2, "{watches:?}");
    assert_eq!(
        watches[0],
        (1, 1, "2020-01-01".into(), 0, "import".into()),
        "the first watch is kept"
    );
    assert_eq!((watches[1].0, watches[1].1, watches[1].3), (2, 2, 1));

    // A show finished with no season named is watched whole, undated, and
    // the same again adds nothing.
    h.cache_show(1399, "Thrones", &[(0, "Specials", 2), (1, "Season 1", 4)])
        .await;
    for _ in 0..2 {
        h.post(
            "/api/import/apply",
            json!({ "items": [{ "kind": "tv", "tmdb_id": 1399, "status": "completed" }] }),
            Some(&owner),
        )
        .await;
    }
    let (ticked, undated): (i64, i64) = h
        .one(
            "SELECT COUNT(*), SUM(w.undated) FROM watch_event w JOIN title t ON t.id = w.title_id
              WHERE (SELECT tmdb_id FROM title_tmdb WHERE title_id = t.id) = 1399",
        )
        .await;
    assert_eq!((ticked, undated), (4, 4), "specials are not part of it");
}

/// A finished season with no count is all of it; a count of zero is none, which
/// is how a finished special stays unticked.
#[tokio::test]
async fn a_count_of_zero_ticks_nothing_even_when_finished() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_show(70523, "Dark", &[(0, "Specials", 3), (1, "Season 1", 3)])
        .await;
    h.cache_show(1399, "Thrones", &[(0, "Specials", 2), (1, "Season 1", 3)])
        .await;

    let (_, body) = h
        .post(
            "/api/import/apply",
            json!({ "on_conflict": "overwrite", "items": [
                { "kind": "tv", "tmdb_id": 70523, "status": "completed", "season": 0, "progress": 0 },
                { "kind": "tv", "tmdb_id": 1399, "status": "completed", "season": 0 }
            ]}),
            Some(&owner),
        )
        .await;
    assert_eq!(body["failed"], json!([]), "{body}");

    let ticked = |id: i64| {
        format!(
            "SELECT COUNT(*) FROM watch_event w JOIN episode e ON e.id = w.episode_id
              WHERE (SELECT tmdb_id FROM title_tmdb WHERE title_id = e.title_id) = {id}"
        )
    };
    let (zero,): (i64,) = h.one(&ticked(70523)).await;
    let (all,): (i64,) = h.one(&ticked(1399)).await;
    assert_eq!((zero, all), (0, 2));
}

/// A dated film is a diary entry, once, with its rewatch flag; a finished
/// film with no date gets one undated watch. A show never gets a
/// title-level entry.
#[tokio::test]
async fn films_are_logged_once_and_shows_never_at_title_level() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_film(949, "Heat", "1995").await;
    h.cache_film(348, "Alien", "1979").await;
    h.cache_show(70523, "Dark", &[(1, "Season 1", 3)]).await;
    let items = json!({ "on_conflict": "merge", "items": [
        { "kind": "movie", "tmdb_id": 949, "status": "completed",
          "watched_at": "2023-03-03T20:00:00Z", "rewatch": true, "rating": 80.0 },
        { "kind": "movie", "tmdb_id": 348, "status": "completed" },
        { "kind": "tv", "tmdb_id": 70523, "status": "watching",
          "watched_at": "2023-03-03T20:00:00Z", "season": 1, "progress": 2 }
    ]});
    let (_, first) = h
        .post("/api/import/apply", items.clone(), Some(&owner))
        .await;
    assert_eq!(first["added"], json!(3), "{first}");
    let (_, again) = h.post("/api/import/apply", items, Some(&owner)).await;
    assert_eq!(
        (&again["added"], &again["updated"], &again["skipped"]),
        (&json!(0), &json!(0), &json!(3)),
        "{again}"
    );

    let films: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT (SELECT tmdb_id FROM title_tmdb WHERE title_id = t.id), substr(w.watched_at, 1, 10), w.is_rewatch, w.undated
           FROM watch_event w JOIN title t ON t.id = w.title_id
          WHERE w.episode_id IS NULL ORDER BY (SELECT tmdb_id FROM title_tmdb WHERE title_id = t.id)",
    )
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(films.len(), 2, "{films:?}");
    assert_eq!(films[0].0, 348);
    assert_eq!(films[0].3, 1, "no date, undated");
    assert_eq!(films[1], (949, "2023-03-03".into(), 1, 0));
}

/// Both exports, field for field, and only the account's own library. The
/// backup is version 2: every version 1 field, plus Frog's id and every
/// provider id the title is known by.
#[tokio::test]
async fn both_exports_keep_their_shape() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_film(949, "Heat, the film", "1995").await;
    h.cache_show(70523, "Dark", &[(1, "Season 1", 3)]).await;
    h.post(
        "/api/import/apply",
        json!({ "items": [
            { "kind": "movie", "tmdb_id": 949, "status": "completed", "rating": 85.0,
              "favourite": true, "watched_at": "2023-03-03T20:00:00Z" },
            { "kind": "tv", "tmdb_id": 70523, "status": "watching", "season": 1, "progress": 2 }
        ]}),
        Some(&owner),
    )
    .await;
    h.put(
        &format!("/api/t/{}/library", h.tid("movie", 949).await),
        json!({ "notes": "Diner scene" }),
        &owner,
    )
    .await;
    sqlx::query(
        "INSERT INTO app_setting (user_id, key, value, updated_at)
         VALUES (1, 'preferences', '{\"theme\":\"pond\"}', '2026-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    let (status, text) = h.get_text("/api/export/library.json", &owner).await;
    assert_eq!(status, StatusCode::OK);
    let mut body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["format"], json!("frog-library"));
    assert_eq!(body["version"], json!(2));
    assert!(body["exported_at"].is_string());
    assert!(body["app_version"].is_string());
    assert_eq!(body["settings"], json!({ "theme": "pond" }));
    let dark = h.tid("tv", 70523).await;
    let heat = h.tid("movie", 949).await;
    for t in body["titles"].as_array_mut().unwrap() {
        assert!(t["added_at"].is_string());
        t["added_at"] = Value::Null;
    }
    assert_eq!(
        body["titles"],
        json!([
            {
                "id": dark,
                "ids": { "tmdb": { "kind": "tv", "id": 70523 }, "anilist": [], "imdb": [] },
                "kind": "tv", "tmdb_id": 70523, "name": "Dark", "release_date": "2015-01-01",
                "status": "watching", "rating": null, "favourite": false, "notes": null,
                "added_at": null,
                "episodes": [
                    { "season": 1, "episode": 1, "rating": null, "notes": null, "watched": true },
                    { "season": 1, "episode": 2, "rating": null, "notes": null, "watched": true }
                ],
                "watches": []
            },
            {
                "id": heat,
                "ids": { "tmdb": { "kind": "movie", "id": 949 }, "anilist": [], "imdb": [] },
                "kind": "movie", "tmdb_id": 949, "name": "Heat, the film", "release_date": "1995-05-01",
                "status": "completed", "rating": 85.0, "favourite": true, "notes": "Diner scene",
                "added_at": null,
                "episodes": [],
                "watches": [{ "watched_at": "2023-03-03T20:00:00Z", "rewatch": false, "source": "import" }]
            }
        ])
    );

    let (status, csv) = h.get_text("/api/export/library.csv", &owner).await;
    assert_eq!(status, StatusCode::OK);
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    assert_eq!(
        csv,
        format!(
            "Type,TMDB ID,Title,Year,Status,Rating,Favourite,Added,Watches,Frog ID,AniList ID\n\
             Show,70523,Dark,2015,watching,,no,{today},2,{dark},\n\
             Film,949,\"Heat, the film\",1995,completed,85,yes,{today},1,{heat},\n"
        )
    );
}

/// A version 2 backup names an anime TMDB lacks by its AniList id, and
/// carries the Frog id it had where it was made. The AniList id finds the
/// title here; the Frog id from elsewhere is never taken to mean a title
/// here, even when a title here happens to have it.
#[tokio::test]
async fn imports_go_by_provider_ids_never_by_another_frogs_ids() {
    let h = Harness::new().await;
    let owner = h.owner().await;
    h.cache_film(949, "Heat", "1995").await;
    for sql in [
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (5, 'tv', -21, 'Only On AniList')",
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (7, 'movie', 550, 'Fight Club')",
    ] {
        sqlx::query(sql).execute(&h.state.pool).await.unwrap();
    }

    let (status, body) = h
        .post(
            "/api/import/apply",
            json!({ "items": [
                // Frog id 7 in the file is Fight Club here: it must not matter.
                { "id": 7, "kind": "movie", "tmdb_id": 949, "status": "completed" },
                { "id": 1, "kind": "tv", "tmdb_id": null, "anilist_id": 21, "status": "watching" },
                { "kind": "tv", "tmdb_id": null, "anilist_id": null, "status": "watching" }
            ]}),
            Some(&owner),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["added"], json!(2), "{body}");
    assert_eq!(
        body["failed"],
        json!([{ "tmdb_id": null, "error": "no TMDB or AniList id" }]),
        "a row naming no title is refused"
    );

    let heat = h.tid("movie", 949).await;
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT title_id, status FROM library_entry ORDER BY title_id")
            .fetch_all(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![(5, "watching".into()), (heat, "completed".into())]
    );
    assert_ne!(heat, 7, "Heat is its own title");
    let (fight_club,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM library_entry WHERE title_id = 7")
            .fetch_one(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(fight_club, 0, "nothing landed on title 7");
}
