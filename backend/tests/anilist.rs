//! AniList sync against a stand-in AniList: sign-in, placing seasons by id,
//! and changes travelling in both directions.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use frog::config::{AniListClient, Config};
use frog::state::AppState;

mod common;

/// What the stand-in AniList holds, and every save Frog sent it.
/// Tests run side by side and all use account 1, and a sync already running
/// for an account makes another return at once without trying. One at a time.
static SYNCS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Default)]
struct Fake {
    entries: Vec<Value>,
    saves: Vec<Value>,
    /// What the relations query answers, by AniList id.
    media: std::collections::HashMap<i64, Value>,
    /// Extra rows for Fribb's anime-lists and Kometa's Anime-IDs.
    lists: Vec<Value>,
    ids: Vec<Value>,
    /// Entries per chunk of the list; the whole list at once when unset.
    per_chunk: Option<usize>,
    /// Statuses the list answers with before it answers properly.
    list_failures: Vec<u16>,
    /// How many times the list was asked for.
    list_calls: usize,
    /// AniList's full answer about an anime, by id, for titles TMDB lacks.
    details: std::collections::HashMap<i64, Value>,
    /// People and studios, as AniList's Staff and Studio queries answer.
    people: std::collections::HashMap<i64, Value>,
    studios: std::collections::HashMap<i64, Value>,
}

type Shared = Arc<Mutex<Fake>>;

fn entry(
    id: i64,
    name: &str,
    format: &str,
    episodes: i64,
    status: &str,
    progress: i64,
    score: i64,
) -> Value {
    json!({
        "status": status, "score": score as f64, "progress": progress,
        "media": { "id": id, "format": format, "episodes": episodes, "seasonYear": 2020,
                   "title": { "english": name, "romaji": name } }
    })
}

async fn graphql(State(fake): State<Shared>, Json(body): Json<Value>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let query = body["query"].as_str().unwrap_or("");
    if query.contains("MediaListCollection") {
        let mut fake = fake.lock().unwrap();
        fake.list_calls += 1;
        if !fake.list_failures.is_empty() {
            let code = StatusCode::from_u16(fake.list_failures.remove(0)).unwrap();
            return (code, [("retry-after", "0")], Json(json!({}))).into_response();
        }
        let chunk = body["variables"]["chunk"].as_u64().unwrap_or(1) as usize;
        let size = fake.per_chunk.unwrap_or(usize::MAX);
        let all = &fake.entries;
        let from = (chunk - 1).saturating_mul(size).min(all.len());
        let to = from.saturating_add(size).min(all.len());
        return Json(json!({ "data": { "MediaListCollection": {
            "hasNextChunk": to < all.len(),
            "lists": [ { "entries": all[from..to] } ] } } }))
        .into_response();
    }
    answer(State(fake), Json(body)).await.into_response()
}

async fn answer(State(fake): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let query = body["query"].as_str().unwrap_or("");
    let vars = &body["variables"];
    let mut fake = fake.lock().unwrap();
    if query.contains("Viewer") {
        return Json(json!({ "data": { "Viewer": { "id": 77, "name": "pondwatcher" } } }));
    }
    let id = vars["id"].as_i64().unwrap_or_default();
    if query.contains("streamingEpisodes") {
        let media = fake.details.get(&id).cloned().unwrap_or(Value::Null);
        return Json(json!({ "data": { "Media": media } }));
    }
    if query.contains("airingSchedules") {
        return Json(json!({ "data": { "Page": {
            "pageInfo": { "hasNextPage": false }, "airingSchedules": [] } } }));
    }
    if query.contains("Staff(id") {
        let staff = fake.people.get(&id).cloned().unwrap_or(Value::Null);
        return Json(json!({ "data": { "Staff": staff } }));
    }
    if query.contains("Studio(id") {
        let studio = fake.studios.get(&id).cloned().unwrap_or(Value::Null);
        return Json(json!({ "data": { "Studio": studio } }));
    }
    if query.contains("SaveMediaListEntry") {
        fake.saves.push(vars.clone());
        let id = vars["mediaId"].as_i64().unwrap();
        let found = fake
            .entries
            .iter_mut()
            .find(|e| e["media"]["id"].as_i64() == Some(id));
        if let Some(e) = found {
            e["status"] = vars["status"].clone();
            e["progress"] = vars["progress"].clone();
            e["score"] = json!(vars["scoreRaw"].as_i64().unwrap_or(0) as f64);
        }
        return Json(json!({ "data": { "SaveMediaListEntry": {
            "status": vars["status"], "progress": vars["progress"],
            "score": vars["scoreRaw"].as_i64().unwrap_or(0) as f64 } } }));
    }
    if query.contains("relations") {
        if let Some(m) = vars["id"].as_i64().and_then(|id| fake.media.get(&id)) {
            return Json(json!({ "data": { "Media": m } }));
        }
        // Lily Pad Story has a second season that no list knows.
        let media = match vars["id"].as_i64() {
            Some(40002) => json!({ "id": 40002, "format": "TV", "episodes": 12,
                "startDate": { "year": 2021 },
                "title": { "english": "Lily Pad Story Season 2", "romaji": null, "native": null },
                "relations": { "edges": [
                    { "relationType": "PREQUEL", "node": { "id": 30000, "type": "ANIME", "format": "TV" } },
                    { "relationType": "PREQUEL", "node": { "id": 30500, "type": "ANIME", "format": "MOVIE" } }
                ] } }),
            Some(30000) => json!({ "id": 30000, "format": "TV", "episodes": 12,
                "startDate": { "year": 2020 },
                "title": { "english": "Lily Pad Story", "romaji": null, "native": null },
                "relations": { "edges": [
                    { "relationType": "SEQUEL", "node": { "id": 40002, "type": "ANIME", "format": "TV" } }
                ] } }),
            _ => Value::Null,
        };
        return Json(json!({ "data": { "Media": media } }));
    }
    if query.contains("Page") {
        // Episode counts of seasons not on this list.
        return Json(json!({ "data": { "Page": { "media": [
            { "id": 113415, "format": "TV", "episodes": 24,
              "title": { "english": "JUJUTSU KAISEN", "romaji": "Jujutsu Kaisen" } }
        ] } } }));
    }
    Json(json!({ "data": { "MediaListCollection": { "lists": [ { "entries": fake.entries } ] } } }))
}

async fn token() -> Json<Value> {
    Json(json!({ "access_token": "fake-token", "expires_in": 31536000 }))
}

async fn anime_ids(State(fake): State<Shared>) -> Json<Value> {
    let mut ids = json!({
        "1": { "anilist_id": 113415, "tvdb_id": 377543, "tvdb_season": 1, "tvdb_epoffset": 0 },
        "2": { "anilist_id": 145064, "tvdb_id": 377543, "tvdb_season": 2, "tvdb_epoffset": 0 },
        "3": { "anilist_id": 9000, "imdb_id": "tt0900000" }
    });
    for (i, row) in fake.lock().unwrap().ids.iter().enumerate() {
        ids[format!("x{i}")] = row.clone();
    }
    Json(ids)
}

/// Fribb's anime-lists: TMDB ids, and TMDB's own season and offset.
async fn anime_lists(State(fake): State<Shared>) -> Json<Value> {
    let extra = fake.lock().unwrap().lists.clone();
    let mut rows = json!([
        { "anilist_id": 145064, "type": "TV", "themoviedb_id": { "tv": 95479 },
          "season": { "tvdb": 2, "tmdb": 1 }, "episode_offset": { "tmdb": 24 } },
        { "anilist_id": 20000, "type": "MOVIE", "themoviedb_id": { "movie": 600 } },
        // Two remakes the list files under the same season: neither is placed.
        { "anilist_id": 20001, "type": "TV", "themoviedb_id": { "tv": 800 },
          "season": { "tmdb": 1 } },
        { "anilist_id": 20002, "type": "TV", "themoviedb_id": { "tv": 800 },
          "season": { "tmdb": 1 } }
    ]);
    if let Some(list) = rows.as_array_mut() {
        list.extend(extra);
    }
    Json(rows)
}

struct Harness {
    state: AppState,
    fake: Shared,
    _dir: tempfile::TempDir,
}

impl Harness {
    /// Frog's id for a title as a provider names it (see `common::title_id`).
    async fn tid(&self, kind: &str, id: i64) -> i64 {
        common::title_id(&self.state, kind, id).await
    }

    async fn new() -> Self {
        let fake: Shared = Arc::default();
        let app = Router::new()
            .route("/graphql", post(graphql))
            .route("/api/v2/oauth/token", post(token))
            .route("/anime_ids.json", get(anime_ids))
            .route("/anime_lists.json", get(anime_lists))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let dir = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite:{}?mode=rwc",
            dir.path().join("t.db").to_string_lossy().replace('\\', "/")
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
            anilist: Some(AniListClient {
                id: "55123".into(),
                secret: "not-a-secret".into(),
                redirect_uri: "https://frog.example/api/anilist/callback".into(),
                api_url: format!("{base}/graphql"),
                site_url: base.clone(),
                anime_ids_url: format!("{base}/anime_ids.json"),
                anime_lists_url: format!("{base}/anime_lists.json"),
            }),
        };
        let pool = frog::db::connect(&url).await.unwrap();
        let state = AppState::new(pool, config);
        common::add_title_seed(&state).await;
        Self {
            state,
            fake,
            _dir: dir,
        }
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        cookie: Option<&str>,
    ) -> (StatusCode, Value, Option<String>, Option<String>) {
        let mut req = Request::builder().uri(path).method(method);
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = frog::routes::router(self.state.clone())
            .oneshot(req)
            .await
            .unwrap();
        let status = res.status();
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap().to_string());
        let location = res
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            cookie,
            location,
        )
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

    async fn sync(&self, user_id: i64) -> frog::anilist::Summary {
        let _turn = SYNCS.lock().await;
        // A sync started by the sign-in may still be running.
        while frog::anilist::is_syncing(user_id) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        frog::anilist::sync_user(&self.state, user_id)
            .await
            .unwrap()
    }

    fn saves(&self) -> Vec<Value> {
        self.fake.lock().unwrap().saves.clone()
    }
}

async fn seed_tmdb(h: &Harness) {
    // TMDB keeps Jujutsu Kaisen as one 47-episode season.
    h.cache(
        "/find/377543?lang=en-US&external_source=tvdb_id",
        json!({ "tv_results": [{ "id": 95479 }], "movie_results": [] }),
    )
    .await;
    h.cache(
        "/tv/95479?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids&include_image_language=en,null",
        json!({ "id": 95479, "name": "JUJUTSU KAISEN", "first_air_date": "2020-10-03",
                "seasons": [{ "season_number": 1, "name": "Season 1", "episode_count": 47 }] }),
    )
    .await;
    let episodes: Vec<Value> = (1..=47)
        .map(|e| json!({ "season_number": 1, "episode_number": e, "air_date": "2021-01-01" }))
        .collect();
    h.cache(
        "/tv/95479/season/1?lang=en-US",
        json!({ "season_number": 1, "episodes": episodes }),
    )
    .await;
    h.cache(
        "/find/tt0900000?lang=en-US&external_source=imdb_id",
        json!({ "movie_results": [{ "id": 500 }], "tv_results": [] }),
    )
    .await;
    h.cache(
        "/movie/500?lang=en-US&append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids,release_dates&include_image_language=en,null",
        json!({ "id": 500, "title": "Jujutsu Kaisen 0", "release_date": "2021-12-24", "runtime": 105 }),
    )
    .await;
    h.cache(
        "/search/multi?lang=en-US&query=Nothing Like It&page=1&include_adult=false",
        json!({ "results": [{ "media_type": "tv", "id": 4242, "name": "Something Close", "first_air_date": "2019-01-01" }] }),
    )
    .await;
}

#[tokio::test]
async fn anilist_comes_in_read_only_and_places_seasons_by_id() {
    let h = Harness::new().await;
    seed_tmdb(&h).await;
    h.fake.lock().unwrap().entries = vec![
        entry(113415, "JUJUTSU KAISEN", "TV", 24, "COMPLETED", 24, 80),
        entry(145064, "JUJUTSU KAISEN Season 2", "TV", 23, "CURRENT", 5, 0),
        entry(9000, "JUJUTSU KAISEN 0", "MOVIE", 1, "COMPLETED", 1, 0),
        entry(31337, "Nothing Like It", "TV", 12, "CURRENT", 3, 0),
    ];

    let (_, _, cookie, _) = h
        .call(
            "POST",
            "/api/auth/setup",
            Some(json!({ "username": "zjef", "password": "a-long-enough-pass" })),
            None,
        )
        .await;
    let cookie = cookie.unwrap();
    let user = 1;

    // Not connected yet, but available.
    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&cookie)).await;
    assert_eq!(status["available"], json!(true));
    assert_eq!(status["connected"], json!(false));

    // Sign in through AniList: the state must come back with the code.
    let (_, start, _, _) = h
        .call("POST", "/api/anilist/connect", None, Some(&cookie))
        .await;
    let url = start["url"].as_str().unwrap();
    assert!(url.contains("client_id=55123"), "{url}");
    assert!(
        url.contains("redirect_uri=https%3A%2F%2Ffrog.example"),
        "{url}"
    );
    let nonce = url.split("state=").nth(1).unwrap();

    let (code, _, _, location) = h
        .call(
            "GET",
            "/api/anilist/callback?code=abc&state=forged",
            None,
            Some(&cookie),
        )
        .await;
    assert!(code.is_redirection());
    assert!(
        location.unwrap().contains("anilist=failed"),
        "a forged state is refused"
    );

    let (code, _, _, location) = h
        .call(
            "GET",
            &format!("/api/anilist/callback?code=abc&state={nonce}"),
            None,
            Some(&cookie),
        )
        .await;
    assert!(code.is_redirection());
    assert!(location.unwrap().contains("anilist=connected"));

    // The sign-in starts the first sync; wait for it, then read the library.
    let summary = h.sync(user).await;
    let _ = summary;
    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&cookie)).await;
    assert_eq!(status["connected"], json!(true));
    assert_eq!(status["name"], json!("pondwatcher"));
    assert_eq!(status["unsure"], json!(1), "{status}");

    let (_, show, _, _) = h
        .call("GET", "/api/titles/tv/95479", None, Some(&cookie))
        .await;
    let item = &show["item"];
    assert_eq!(item["entry"]["status"], json!("watching"), "{item}");
    assert_eq!(
        item["watched_episodes"],
        json!(29),
        "24 + 5, season 2 inside TMDB's season 1"
    );
    // AniList scores each entry; Frog keeps each one on the season it covers,
    // not on the whole show.
    assert_eq!(item["entry"]["rating"], Value::Null, "{item}");
    let season_one = show["ratings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["season"] == json!(1) && u["first_episode"] == json!(1))
        .cloned()
        .unwrap_or_default();
    assert_eq!(season_one["rating"], json!(80.0), "{}", show["ratings"]);
    let (_, film, _, _) = h
        .call("GET", "/api/titles/movie/500", None, Some(&cookie))
        .await;
    assert_eq!(film["item"]["entry"]["status"], json!("completed"));
    assert!(
        h.saves().is_empty(),
        "AniList is only ever read: {:?}",
        h.saves()
    );

    // A tick in Frog stays in Frog.
    let (code, _, _, _) = h
        .call(
            "POST",
            &format!("/api/t/{}/episodes/1/30/watch", h.tid("tv", 95479).await),
            Some(json!({})),
            Some(&cookie),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    h.sync(user).await;
    assert!(h.saves().is_empty(), "{:?}", h.saves());

    // Progress made on AniList comes to Frog.
    {
        let mut fake = h.fake.lock().unwrap();
        fake.entries[1]["progress"] = json!(10);
    }
    let summary = h.sync(user).await;
    assert_eq!(summary.to_frog, 1, "{summary:?}");
    let (_, show, _, _) = h
        .call("GET", "/api/titles/tv/95479", None, Some(&cookie))
        .await;
    assert_eq!(show["item"]["watched_episodes"], json!(34));
    assert!(h.saves().is_empty(), "{:?}", h.saves());

    // Rating the whole show stays in Frog.
    h.call(
        "PUT",
        &format!("/api/t/{}/library", h.tid("tv", 95479).await),
        Some(json!({ "rating": 60.0 })),
        Some(&cookie),
    )
    .await;
    h.sync(user).await;
    assert!(h.saves().is_empty(), "{:?}", h.saves());

    // So does rating one season (here the part of TMDB's season 1 that
    // AniList calls season 2).
    let (code, body, _, _) = h
        .call(
            "PUT",
            &format!("/api/t/{}/seasons/1/rating", h.tid("tv", 95479).await),
            Some(json!({ "first_episode": 25, "rating": 95.0 })),
            Some(&cookie),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    h.sync(user).await;
    assert!(h.saves().is_empty(), "{:?}", h.saves());

    // Taking the film out of Frog leaves AniList alone, and it is not re-added.
    h.call(
        "DELETE",
        &format!("/api/t/{}/library", h.tid("movie", 500).await),
        None,
        Some(&cookie),
    )
    .await;
    h.sync(user).await;
    h.sync(user).await;
    assert!(h.saves().is_empty(), "{:?}", h.saves());
    let (code, film, _, _) = h
        .call("GET", "/api/titles/movie/500", None, Some(&cookie))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert!(film["item"]["entry"].is_null(), "{film}");

    // The unsure one: offered candidates, then placed by choice.
    let (_, list, _, _) = h
        .call("GET", "/api/anilist/entries", None, Some(&cookie))
        .await;
    let unsure = &list["entries"][0];
    assert_eq!(unsure["state"], json!("unsure"));
    assert_eq!(unsure["candidates"][0]["tmdb_id"], json!(4242));
    let (code, _, _, _) = h
        .call(
            "PUT",
            "/api/anilist/entries/31337",
            Some(json!({ "ignore": true })),
            Some(&cookie),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&cookie)).await;
    assert_eq!(status["unsure"], json!(0));

    // Disconnecting forgets the link, not the library.
    h.call("DELETE", "/api/anilist", None, Some(&cookie)).await;
    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&cookie)).await;
    assert_eq!(status["connected"], json!(false));
    let (_, show, _, _) = h
        .call("GET", "/api/titles/tv/95479", None, Some(&cookie))
        .await;
    assert_eq!(show["item"]["watched_episodes"], json!(34));
}

#[tokio::test]
async fn another_account_cannot_see_or_change_your_anilist() {
    let h = Harness::new().await;
    let (_, _, owner, _) = h
        .call(
            "POST",
            "/api/auth/setup",
            Some(json!({ "username": "zjef", "password": "a-long-enough-pass" })),
            None,
        )
        .await;
    let owner = owner.unwrap();
    sqlx::query(
        "INSERT INTO anilist_link (user_id, anilist_user_id, anilist_name, token, expires_at, connected_at)
         VALUES (1, 77, 'pondwatcher', 't', '2099-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO anilist_entry (user_id, media_id, name, state, updated_at)
         VALUES (1, 5, 'Mine', 'unsure', '2026-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    // An invited second account.
    let (_, invite, _, _) = h
        .call(
            "POST",
            "/api/invites",
            Some(json!({ "role": "user" })),
            Some(&owner),
        )
        .await;
    let code = invite["code"].as_str().unwrap().to_string();
    let (_, _, other, _) = h
        .call(
            "POST",
            "/api/auth/register",
            Some(json!({ "invite": code, "username": "mihai", "password": "another-long-pass" })),
            None,
        )
        .await;
    let other = other.unwrap();

    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&other)).await;
    assert_eq!(status["connected"], json!(false));
    let (_, list, _, _) = h
        .call("GET", "/api/anilist/entries", None, Some(&other))
        .await;
    assert_eq!(list["entries"], json!([]));
    let (code, _, _, _) = h
        .call(
            "PUT",
            "/api/anilist/entries/5",
            Some(json!({ "ignore": true })),
            Some(&other),
        )
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    h.call("DELETE", "/api/anilist", None, Some(&other)).await;
    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&owner)).await;
    assert_eq!(
        status["connected"],
        json!(true),
        "the owner's link survives"
    );

    // Signed out: nothing.
    let (code, _, _, _) = h.call("GET", "/api/anilist", None, None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

/// TMDB out of reach is an error to retry, not a reason to give up on every
/// entry: nothing is marked as needing a place, and the next sync places them.
#[tokio::test]
async fn a_tmdb_outage_is_retried_not_left_for_the_person() {
    let h = Harness::new().await;
    h.fake.lock().unwrap().entries = vec![entry(
        113415,
        "JUJUTSU KAISEN",
        "TV",
        24,
        "COMPLETED",
        24,
        80,
    )];
    let (_, _, cookie, _) = h
        .call(
            "POST",
            "/api/auth/setup",
            Some(json!({ "username": "zjef", "password": "a-long-enough-pass" })),
            None,
        )
        .await;
    let cookie = cookie.unwrap();
    sqlx::query(
        "INSERT INTO anilist_link (user_id, anilist_user_id, anilist_name, token, expires_at, connected_at)
         VALUES (1, 77, 'pondwatcher', 't', '2099-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();

    // Nothing from TMDB is cached and there is no key: TMDB is unreachable.
    {
        let _turn = SYNCS.lock().await;
        while frog::anilist::is_syncing(1) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(frog::anilist::sync_user(&h.state, 1).await.is_err());
    }
    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&cookie)).await;
    assert_eq!(status["unsure"], json!(0), "{status}");
    assert!(
        status["error"].as_str().unwrap_or("").contains("TMDB"),
        "{status}"
    );

    // TMDB back: the same entry is placed as if nothing had happened.
    seed_tmdb(&h).await;
    h.sync(1).await;
    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&cookie)).await;
    assert_eq!(status["error"], Value::Null, "{status}");
    let (_, show, _, _) = h
        .call("GET", "/api/titles/tv/95479", None, Some(&cookie))
        .await;
    assert_eq!(show["item"]["watched_episodes"], json!(24));
}

/// What Anime-IDs cannot place: a film only Fribb's list knows, and a show
/// neither list knows but TMDB has under exactly that name and year. A name
/// shared by two animations, or a spot two entries claim, is left to choose.
#[tokio::test]
async fn ids_from_either_list_or_an_exact_name_place_entries() {
    let h = Harness::new().await;
    seed_tmdb(&h).await;
    let detail = "append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids";
    h.cache(
        &format!("/movie/600?lang=en-US&{detail},release_dates&include_image_language=en,null"),
        json!({ "id": 600, "title": "Pond Movie", "release_date": "2020-07-01", "runtime": 90 }),
    )
    .await;
    h.cache(
        "/search/multi?lang=en-US&query=Lily Pad Story&page=1&include_adult=false",
        json!({ "results": [
            { "media_type": "tv", "id": 701, "name": "Lily Pad Story", "genre_ids": [18],
              "first_air_date": "2020-01-01" },
            { "media_type": "tv", "id": 702, "name": "Lily Pad Story", "genre_ids": [16],
              "first_air_date": "2008-01-01" },
            { "media_type": "tv", "id": 700, "name": "Lily Pad Story!", "genre_ids": [16],
              "first_air_date": "2020-04-01" }
        ] }),
    )
    .await;
    h.cache(
        &format!("/tv/700?lang=en-US&{detail}&include_image_language=en,null"),
        json!({ "id": 700, "name": "Lily Pad Story!", "first_air_date": "2020-04-01",
                "seasons": [{ "season_number": 1, "name": "Season 1", "episode_count": 12 },
                            { "season_number": 2, "name": "Season 2", "episode_count": 12 }] }),
    )
    .await;
    let episodes: Vec<Value> = (1..=12)
        .map(|e| json!({ "season_number": 1, "episode_number": e, "air_date": "2020-06-01" }))
        .collect();
    h.cache(
        "/tv/700/season/1?lang=en-US",
        json!({ "season_number": 1, "episodes": episodes }),
    )
    .await;
    let episodes: Vec<Value> = (1..=12)
        .map(|e| json!({ "season_number": 2, "episode_number": e, "air_date": "2021-06-01" }))
        .collect();
    h.cache(
        "/tv/700/season/2?lang=en-US",
        json!({ "season_number": 2, "episodes": episodes }),
    )
    .await;
    h.cache(
        "/search/multi?lang=en-US&query=Twin Name&page=1&include_adult=false",
        json!({ "results": [
            { "media_type": "tv", "id": 710, "name": "Twin Name", "genre_ids": [16],
              "first_air_date": "2020-01-01" },
            { "media_type": "tv", "id": 711, "name": "Twin Name", "genre_ids": [16, 18],
              "first_air_date": "2020-10-01" }
        ] }),
    )
    .await;
    for name in ["Remake One", "Remake Two", "Lily Pad Story Season 2"] {
        h.cache(
            &format!("/search/multi?lang=en-US&query={name}&page=1&include_adult=false"),
            json!({ "results": [] }),
        )
        .await;
    }
    h.fake.lock().unwrap().entries = vec![
        entry(20000, "Pond Movie", "MOVIE", 1, "COMPLETED", 1, 70),
        entry(30000, "Lily Pad Story", "TV", 12, "CURRENT", 4, 0),
        entry(40002, "Lily Pad Story Season 2", "TV", 12, "CURRENT", 2, 0),
        entry(30001, "Twin Name", "TV", 12, "PLANNING", 0, 0),
        entry(20001, "Remake One", "TV", 12, "PLANNING", 0, 0),
        entry(20002, "Remake Two", "TV", 12, "PLANNING", 0, 0),
    ];
    let (_, _, cookie, _) = h
        .call(
            "POST",
            "/api/auth/setup",
            Some(json!({ "username": "zjef", "password": "a-long-enough-pass" })),
            None,
        )
        .await;
    let cookie = cookie.unwrap();
    sqlx::query(
        "INSERT INTO anilist_link (user_id, anilist_user_id, anilist_name, token, expires_at, connected_at)
         VALUES (1, 77, 'pondwatcher', 't', '2099-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    h.sync(1).await;

    let (_, film, _, _) = h
        .call("GET", "/api/titles/movie/600", None, Some(&cookie))
        .await;
    assert_eq!(
        film["item"]["entry"]["status"],
        json!("completed"),
        "{film}"
    );

    let (_, show, _, _) = h
        .call("GET", "/api/titles/tv/700", None, Some(&cookie))
        .await;
    assert_eq!(show["item"]["entry"]["status"], json!("watching"), "{show}");
    assert_eq!(
        show["item"]["watched_episodes"],
        json!(6),
        "season 2, found through its prequel, lands in TMDB season 2"
    );
    let (_, season, _, _) = h
        .call(
            "GET",
            &format!("/api/t/{}/seasons/2", h.tid("tv", 700).await),
            None,
            Some(&cookie),
        )
        .await;
    let watched: Vec<bool> = season["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["watched"].as_bool().unwrap_or(false))
        .collect();
    assert_eq!(&watched[..3], &[true, true, false], "{season}");

    let (_, status, _, _) = h.call("GET", "/api/anilist", None, Some(&cookie)).await;
    assert_eq!(
        status["unsure"],
        json!(3),
        "the twin name and both remakes wait for a choice: {status}"
    );
}

// --- the cases from real lists -------------------------------------------------

/// An entry with AniList's release facts: its state and its start date.
#[allow(clippy::too_many_arguments)]
fn released(
    id: i64,
    name: &str,
    format: &str,
    episodes: Option<i64>,
    status: &str,
    progress: i64,
    media_status: &str,
    start: Option<&str>,
) -> Value {
    let date = start.map(|d| {
        let p: Vec<i64> = d.split('-').map(|x| x.parse().unwrap()).collect();
        json!({ "year": p[0], "month": p[1], "day": p[2] })
    });
    json!({
        "status": status, "score": 0.0, "progress": progress,
        "media": {
            "id": id, "format": format, "episodes": episodes, "status": media_status,
            "countryOfOrigin": "JP",
            "startDate": date.unwrap_or(json!({ "year": null, "month": null, "day": null })),
            "title": { "english": name, "romaji": name }
        }
    })
}

/// How the relations query describes one entry.
fn linked(id: i64, format: &str, episodes: Option<i64>, relations: &[(&str, i64, &str)]) -> Value {
    json!({
        "id": id, "format": format, "episodes": episodes, "status": "FINISHED",
        "startDate": { "year": 2020, "month": null, "day": null },
        "title": { "english": format!("Entry {id}"), "romaji": null, "native": null },
        "relations": { "edges": relations.iter().map(|(rel, other, f)| json!({
            "relationType": rel, "node": { "id": other, "type": "ANIME", "format": f }
        })).collect::<Vec<_>>() }
    })
}

const TV_DETAIL: &str = "append_to_response=credits,videos,images,watch/providers,recommendations,similar,external_ids&include_image_language=en,null";

impl Harness {
    /// A TMDB show in the cache: each season as a list of air dates.
    async fn show(
        &self,
        id: i64,
        name: &str,
        seasons: &[(i64, Vec<Option<&str>>)],
        next: Option<(i64, i64, &str)>,
    ) {
        let mut detail = json!({
            "id": id, "name": name, "first_air_date": "2018-01-01",
            "genres": [{ "id": 16, "name": "Animation" }],
            "seasons": seasons.iter().map(|(n, eps)| json!({
                "season_number": n, "name": format!("Season {n}"), "episode_count": eps.len()
            })).collect::<Vec<_>>(),
        });
        if let Some((season, episode, date)) = next {
            detail["next_episode_to_air"] =
                json!({ "season_number": season, "episode_number": episode, "air_date": date });
        }
        self.cache(&format!("/tv/{id}?lang=en-US&{TV_DETAIL}"), detail)
            .await;
        for (n, eps) in seasons {
            let episodes: Vec<Value> = eps
                .iter()
                .enumerate()
                .map(|(i, date)| {
                    json!({
                        "season_number": n, "episode_number": i + 1,
                        "name": format!("Episode {}", i + 1), "air_date": date
                    })
                })
                .collect();
            self.cache(
                &format!("/tv/{id}/season/{n}?lang=en-US"),
                json!({ "season_number": n, "episodes": episodes }),
            )
            .await;
        }
    }

    async fn search(&self, query: &str, results: Value) {
        self.cache(
            &format!("/search/multi?lang=en-US&query={query}&page=1&include_adult=false"),
            json!({ "results": results }),
        )
        .await;
    }

    /// A signed-in account with AniList connected, without the sign-in dance.
    async fn connected(&self) -> String {
        let (_, _, cookie, _) = self
            .call(
                "POST",
                "/api/auth/setup",
                Some(json!({ "username": "zjef", "password": "a-long-enough-pass" })),
                None,
            )
            .await;
        sqlx::query(
            "INSERT INTO anilist_link (user_id, anilist_user_id, anilist_name, token, expires_at, connected_at)
             VALUES (1, 77, 'pondwatcher', 't', '2099-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(&self.state.pool)
        .await
        .unwrap();
        cookie.unwrap()
    }

    async fn get(&self, path: &str, cookie: &str) -> Value {
        self.call("GET", path, None, Some(cookie)).await.1
    }

    /// (season, episode) of every watched episode of a show, in order.
    async fn watched(&self, tmdb_id: i64) -> Vec<(i64, i64)> {
        sqlx::query_as(
            "SELECT e.season_number, e.episode_number FROM watch_event w
               JOIN episode e ON e.id = w.episode_id
               JOIN title t ON t.id = e.title_id
              WHERE w.user_id = 1 AND (SELECT tmdb_id FROM title_tmdb WHERE title_id = t.id) = ? AND t.kind = 'tv'
              ORDER BY e.season_number, e.episode_number",
        )
        .bind(tmdb_id)
        .fetch_all(&self.state.pool)
        .await
        .unwrap()
    }

    async fn entry(&self, cookie: &str, media_id: i64) -> Value {
        let list = self.get("/api/anilist/entries", cookie).await;
        list["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["media_id"] == json!(media_id))
            .cloned()
            .unwrap_or_default()
    }
}

fn weekly(from: &str, count: usize) -> Vec<Option<String>> {
    let start = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").unwrap();
    (0..count)
        .map(|i| {
            Some(
                (start + chrono::Duration::weeks(i as i64))
                    .format("%Y-%m-%d")
                    .to_string(),
            )
        })
        .collect()
}

fn dates(list: &[Option<String>]) -> Vec<Option<&str>> {
    list.iter().map(|d| d.as_deref()).collect()
}

/// Angels of Death: its four-episode ONA is not a season on TMDB but four
/// of the show's specials. Nothing lists it; the air date places it.
#[tokio::test]
async fn an_ona_tmdb_keeps_as_specials_is_found_by_its_air_date() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.lists.push(json!({ "anilist_id": 99629, "type": "TV",
            "themoviedb_id": { "tv": 1001 }, "season": { "tmdb": 1 } }));
        fake.media.insert(
            104243,
            linked(104243, "ONA", Some(4), &[("PREQUEL", 99629, "TV")]),
        );
        fake.media.insert(
            99629,
            linked(99629, "TV", Some(12), &[("SEQUEL", 104243, "ONA")]),
        );
        fake.entries = vec![
            released(
                99629,
                "Angels of Death",
                "TV",
                Some(12),
                "COMPLETED",
                12,
                "FINISHED",
                Some("2018-07-06"),
            ),
            released(
                104243,
                "Angels of Death (ONA)",
                "ONA",
                Some(4),
                "COMPLETED",
                4,
                "FINISHED",
                Some("2018-12-21"),
            ),
        ];
    }
    let season1 = weekly("2018-07-06", 12);
    let specials = weekly("2018-12-21", 4);
    h.show(
        1001,
        "Angels of Death",
        &[(0, dates(&specials)), (1, dates(&season1))],
        None,
    )
    .await;
    // What searching by hand finds, the matcher finds too.
    h.search(
        "Angels of Death",
        json!([{ "media_type": "tv", "id": 1001, "name": "Angels of Death",
        "genre_ids": [16], "first_air_date": "2018-07-06", "original_language": "ja" }]),
    )
    .await;
    let cookie = h.connected().await;
    h.sync(1).await;

    let ona = h.entry(&cookie, 104243).await;
    assert_eq!(ona["outcome"], json!("matched"), "{ona}");
    assert_eq!(ona["method"], json!("air date"));
    assert_eq!(ona["title"]["season"], json!(0));
    assert_eq!(ona["title"]["from_episode"], json!(1));
    let watched = h.watched(1001).await;
    assert_eq!(
        &watched[..4],
        &[(0, 1), (0, 2), (0, 3), (0, 4)],
        "{watched:?}"
    );
    assert_eq!(watched.len(), 16, "four specials and the twelve episodes");
}

/// A sequel whose title shares nothing with the first season still lands on
/// the right season of the right show, by counting on from the first.
#[tokio::test]
async fn a_sequel_with_another_title_lands_on_its_season() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.lists.push(json!({ "anilist_id": 50001, "type": "TV",
            "themoviedb_id": { "tv": 2001 }, "season": { "tmdb": 1 } }));
        fake.media.insert(
            50002,
            linked(50002, "TV", Some(10), &[("PREQUEL", 50001, "TV")]),
        );
        fake.media.insert(
            50001,
            linked(50001, "TV", Some(12), &[("SEQUEL", 50002, "TV")]),
        );
        fake.entries = vec![
            released(
                50001,
                "Pond Tales",
                "TV",
                Some(12),
                "COMPLETED",
                12,
                "FINISHED",
                Some("2020-01-03"),
            ),
            released(
                50002,
                "The Lily Wars",
                "TV",
                Some(10),
                "CURRENT",
                3,
                "FINISHED",
                Some("2022-04-01"),
            ),
        ];
    }
    let s1 = weekly("2020-01-03", 12);
    let s2 = weekly("2022-04-01", 10);
    h.show(
        2001,
        "Pond Tales",
        &[(1, dates(&s1)), (2, dates(&s2))],
        None,
    )
    .await;
    let cookie = h.connected().await;
    let first = h.sync(1).await;

    let sequel = h.entry(&cookie, 50002).await;
    assert_eq!(sequel["title"]["season"], json!(2), "{sequel}");
    assert_eq!(sequel["title"]["from_episode"], json!(1));
    let watched = h.watched(2001).await;
    assert_eq!(watched.len(), 15);
    assert_eq!(watched[12..], [(2, 1), (2, 2), (2, 3)]);
    let show = h.get("/api/titles/tv/2001", &cookie).await;
    assert_eq!(show["item"]["entry"]["status"], json!("watching"));
    assert_eq!(first.unsure, 0);

    // Synced again and again with nothing changed: nothing moves, nothing is
    // added twice, nothing is sent.
    let count = |table: &'static str| {
        let pool = h.state.pool.clone();
        async move {
            let (n,): (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            n
        }
    };
    let before = (
        count("watch_event").await,
        count("anilist_entry").await,
        count("library_entry").await,
        count("season_rating").await,
    );
    for _ in 0..3 {
        let again = h.sync(1).await;
        assert_eq!((again.to_frog, again.repaired), (0, 0), "{again:?}");
    }
    let after = (
        count("watch_event").await,
        count("anilist_entry").await,
        count("library_entry").await,
        count("season_rating").await,
    );
    assert_eq!(before, after);
    assert_eq!(h.watched(2001).await, watched);
    assert!(h.saves().is_empty(), "{:?}", h.saves());

    // An older matcher put the sequel on season 1: the next sync moves it,
    // takes back only what it had given there, and keeps season 1's own.
    sqlx::query(
        "UPDATE anilist_entry SET season = 1, episode_offset = 0, resolver = 0 WHERE media_id = 50002",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM watch_event WHERE episode_id IN (SELECT id FROM episode WHERE season_number = 2)")
        .execute(&h.state.pool)
        .await
        .unwrap();
    let repaired = h.sync(1).await;
    assert_eq!(repaired.repaired, 1, "{repaired:?}");
    assert_eq!(
        h.watched(2001).await,
        watched,
        "season 1 kept, season 2 restored"
    );
    let sequel = h.entry(&cookie, 50002).await;
    assert_eq!(sequel["title"]["season"], json!(2));
    assert!(
        h.saves().is_empty(),
        "the repair sends AniList nothing: {:?}",
        h.saves()
    );

    // A place the person chose is theirs, however old the matcher.
    sqlx::query(
        "UPDATE anilist_entry SET season = 1, episode_offset = 0, resolver = 0, manual = 1
          WHERE media_id = 50002",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    let kept = h.sync(1).await;
    assert_eq!(kept.repaired, 0);
    assert_eq!(h.entry(&cookie, 50002).await["title"]["season"], json!(1));
}

/// One Piece: one AniList entry, still airing, 8 episodes in, over TMDB
/// seasons of 5. Progress runs on past season 1 instead of stopping there.
#[tokio::test]
async fn a_long_running_entry_runs_on_through_tmdb_seasons() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.ids.push(
            json!({ "anilist_id": 21, "tvdb_id": 81797, "tvdb_season": -1, "tvdb_epoffset": 0 }),
        );
        fake.entries = vec![released(
            21,
            "Long Voyage",
            "TV",
            None,
            "CURRENT",
            8,
            "RELEASING",
            Some("1999-10-20"),
        )];
    }
    h.cache(
        "/find/81797?lang=en-US&external_source=tvdb_id",
        json!({ "tv_results": [{ "id": 3001 }], "movie_results": [] }),
    )
    .await;
    let s1 = weekly("1999-10-20", 5);
    let s2 = weekly("1999-11-24", 5);
    let mut s3 = weekly("1999-12-29", 3);
    s3.push(Some("2099-01-04".into()));
    s3.push(Some("2099-01-11".into()));
    h.show(
        3001,
        "Long Voyage",
        &[(1, dates(&s1)), (2, dates(&s2)), (3, dates(&s3))],
        Some((3, 4, "2099-01-04")),
    )
    .await;
    let cookie = h.connected().await;
    h.sync(1).await;

    let watched = h.watched(3001).await;
    assert_eq!(watched.len(), 8, "{watched:?}");
    assert_eq!(
        watched.last(),
        Some(&(2, 3)),
        "not stopped at the end of TMDB's season 1"
    );
    let show = h.get("/api/titles/tv/3001", &cookie).await;
    assert_eq!(show["next_up"]["season_number"], json!(2));
    assert_eq!(show["next_up"]["episode_number"], json!(4));
    assert_eq!(show["item"]["entry"]["status"], json!("watching"));

    // Caught up with everything out on a series still airing: watching,
    // shown as caught up, never completed.
    h.fake.lock().unwrap().entries[0]["progress"] = json!(13);
    h.sync(1).await;
    assert_eq!(h.watched(3001).await.len(), 13, "never the two not out yet");
    let show = h.get("/api/titles/tv/3001", &cookie).await;
    assert_eq!(show["item"]["entry"]["status"], json!("watching"));
    let home = h.get("/api/home", &cookie).await;
    let waiting: Vec<Value> = home["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .chain(home["hero"].as_object().map(|_| &home["hero"]).into_iter())
        .map(|u| u["item"]["id"].clone())
        .collect();
    assert!(waiting.contains(&json!(h.tid("tv", 3001).await)), "{home}");
}

/// Alya: season 1 finished, season 2 announced with an episode TMDB lists
/// without a date. Completed, nothing to continue, no phantom episode.
#[tokio::test]
async fn an_announced_season_makes_no_phantom_episode() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.lists.push(json!({ "anilist_id": 60001, "type": "TV",
            "themoviedb_id": { "tv": 4001 }, "season": { "tmdb": 1 } }));
        fake.lists.push(json!({ "anilist_id": 60002, "type": "TV",
            "themoviedb_id": { "tv": 4001 }, "season": { "tmdb": 2 } }));
        fake.entries = vec![
            released(
                60001,
                "Alya",
                "TV",
                Some(12),
                "COMPLETED",
                12,
                "FINISHED",
                Some("2024-07-03"),
            ),
            released(
                60002,
                "Alya Season 2",
                "TV",
                None,
                "PLANNING",
                0,
                "NOT_YET_RELEASED",
                None,
            ),
        ];
    }
    let s1 = weekly("2024-07-03", 12);
    h.show(4001, "Alya", &[(1, dates(&s1)), (2, vec![None])], None)
        .await;
    let cookie = h.connected().await;
    h.sync(1).await;

    let show = h.get("/api/titles/tv/4001", &cookie).await;
    assert_eq!(
        show["item"]["entry"]["status"],
        json!("completed"),
        "{}",
        show["item"]
    );
    assert_eq!(
        show["item"]["total_episodes"],
        json!(12),
        "the undated episode is not out"
    );
    assert_eq!(show["next_up"], Value::Null, "nothing to watch next");
    let frog_id = h.tid("tv", 4001).await;
    let home = h.get("/api/home", &cookie).await;
    for k in ["continue", "waiting"] {
        assert!(
            !home[k]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u["item"]["id"] == json!(frog_id)),
            "{k}: {home}"
        );
    }
    assert_eq!(home["hero"]["next"], Value::Null, "{home}");
}

/// A finished season with a dated sequel: Completed, and the sequel on the
/// calendar once, from TMDB's episode and AniList's date alike.
#[tokio::test]
async fn a_completed_season_still_puts_its_sequel_on_the_calendar() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.lists.push(json!({ "anilist_id": 70001, "type": "TV",
            "themoviedb_id": { "tv": 5001 }, "season": { "tmdb": 1 } }));
        fake.lists.push(json!({ "anilist_id": 70002, "type": "TV",
            "themoviedb_id": { "tv": 5001 }, "season": { "tmdb": 1 }, "episode_offset": { "tmdb": 4 } }));
        fake.entries = vec![
            released(
                70001,
                "Frontier",
                "TV",
                Some(4),
                "COMPLETED",
                4,
                "FINISHED",
                Some("2023-10-01"),
            ),
            released(
                70002,
                "Frontier Season 2",
                "TV",
                None,
                "PLANNING",
                0,
                "NOT_YET_RELEASED",
                Some("2099-01-10"),
            ),
        ];
    }
    let mut s1 = weekly("2023-10-01", 4);
    s1.push(Some("2099-01-10".into()));
    h.show(
        5001,
        "Frontier",
        &[(1, dates(&s1))],
        Some((1, 5, "2099-01-10")),
    )
    .await;
    let cookie = h.connected().await;
    h.sync(1).await;

    let show = h.get("/api/titles/tv/5001", &cookie).await;
    assert_eq!(
        show["item"]["entry"]["status"],
        json!("completed"),
        "{}",
        show["item"]
    );
    let frog_id = h.tid("tv", 5001).await;
    let calendar = h
        .get("/api/calendar?from=2099-01-01&to=2099-01-31", &cookie)
        .await;
    let days: Vec<Value> = calendar["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"] == json!(frog_id))
        .map(|e| e["date"].clone())
        .collect();
    assert_eq!(days, vec![json!("2099-01-10")], "{calendar}");
}

/// Nothing out yet: planned, and nowhere on Home as something to watch.
#[tokio::test]
async fn a_title_not_out_is_never_watchable() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.lists.push(json!({ "anilist_id": 80001, "type": "TV",
            "themoviedb_id": { "tv": 6001 }, "season": { "tmdb": 1 } }));
        fake.entries = vec![released(
            80001,
            "Soon",
            "TV",
            Some(3),
            "PLANNING",
            0,
            "NOT_YET_RELEASED",
            Some("2099-04-01"),
        )];
    }
    h.show(
        6001,
        "Soon",
        &[(
            1,
            vec![Some("2099-04-01"), Some("2099-04-08"), Some("2099-04-15")],
        )],
        Some((1, 1, "2099-04-01")),
    )
    .await;
    let cookie = h.connected().await;
    h.sync(1).await;
    // Even if somebody marks it watching, nothing out means planned.
    h.call(
        "PUT",
        &format!("/api/t/{}/library", h.tid("tv", 6001).await),
        Some(json!({ "status": "watching" })),
        Some(&cookie),
    )
    .await;
    h.sync(1).await;

    let show = h.get("/api/titles/tv/6001", &cookie).await;
    assert_eq!(show["item"]["total_episodes"], json!(0));
    let frog_id = h.tid("tv", 6001).await;
    let home = h.get("/api/home", &cookie).await;
    for k in ["continue", "waiting"] {
        assert!(
            !home[k]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u["item"]["id"] == json!(frog_id)),
            "{k}: {home}"
        );
    }
}

/// Where the id lists and the air dates disagree, nothing is applied: the
/// entry waits, saying why.
#[tokio::test]
async fn disagreeing_metadata_is_flagged_not_applied() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.lists.push(json!({ "anilist_id": 90001, "type": "TV",
            "themoviedb_id": { "tv": 8001 }, "season": { "tmdb": 1 }, "episode_offset": { "tmdb": 5 } }));
        fake.entries = vec![released(
            90001,
            "Mismatch Story",
            "TV",
            Some(3),
            "COMPLETED",
            3,
            "FINISHED",
            Some("2021-03-01"),
        )];
    }
    let s1 = weekly("2019-01-04", 8);
    h.show(8001, "Mismatch Story", &[(1, dates(&s1))], None)
        .await;
    h.search("Mismatch Story", json!([])).await;
    let cookie = h.connected().await;
    let summary = h.sync(1).await;

    assert_eq!(summary.unsure, 1, "{summary:?}");
    let entry = h.entry(&cookie, 90001).await;
    assert_eq!(entry["outcome"], json!("not_confident"), "{entry}");
    assert!(
        entry["note"]
            .as_str()
            .unwrap_or("")
            .contains("do not agree"),
        "{entry}"
    );
    assert!(h.watched(8001).await.is_empty(), "nothing applied");
}

/// Seasons are rated one by one, and only once all of a season is out.
#[tokio::test]
async fn seasons_are_rated_separately_and_only_when_out() {
    let h = Harness::new().await;
    let s1 = weekly("2020-01-01", 3);
    let s2 = weekly("2021-01-01", 3);
    let s3 = vec![Some("2022-01-01"), Some("2022-01-08"), Some("2099-01-01")];
    h.show(
        7001,
        "Rated",
        &[(1, dates(&s1)), (2, dates(&s2)), (3, s3)],
        Some((3, 3, "2099-01-01")),
    )
    .await;
    let cookie = h.connected().await;
    h.call(
        "PUT",
        &format!("/api/t/{}/library", h.tid("tv", 7001).await),
        Some(json!({ "status": "watching" })),
        Some(&cookie),
    )
    .await;

    let show = h.tid("tv", 7001).await;
    let rate = |season: i64, rating: f64| {
        let path = format!("/api/t/{show}/seasons/{season}/rating");
        let cookie = cookie.clone();
        let h = &h;
        async move {
            h.call(
                "PUT",
                &path,
                Some(json!({ "rating": rating })),
                Some(&cookie),
            )
            .await
            .0
        }
    };
    assert_eq!(rate(1, 80.0).await, StatusCode::OK);
    assert_eq!(rate(2, 60.0).await, StatusCode::OK);
    assert_eq!(
        rate(3, 90.0).await,
        StatusCode::BAD_REQUEST,
        "season 3 is still airing"
    );

    let show = h.get("/api/titles/tv/7001", &cookie).await;
    let units: Vec<(Value, Value, Value)> = show["ratings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| {
            (
                u["season"].clone(),
                u["rating"].clone(),
                u["rateable"].clone(),
            )
        })
        .collect();
    assert_eq!(
        units,
        vec![
            (json!(1), json!(80.0), json!(true)),
            (json!(2), json!(60.0), json!(true)),
            (json!(3), Value::Null, json!(false)),
        ]
    );
    assert_eq!(
        show["item"]["entry"]["rating"],
        Value::Null,
        "the show itself is not rated by them"
    );
}

// --- resilience: titles TMDB lacks, failures, retries, corrections -----------

/// A row of the library list by its TMDB id (negative: its AniList id).
async fn in_library(h: &Harness, cookie: &str, tmdb_id: i64) -> Option<Value> {
    let id = common::existing_id(&h.state, tmdb_id).await?;
    let list = h.get("/api/library?limit=500", cookie).await;
    list["items"]
        .as_array()
        .or_else(|| list.as_array())
        .unwrap()
        .iter()
        .find(|i| i["id"] == json!(id))
        .cloned()
}

async fn count(h: &Harness, sql: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(sql)
        .fetch_one(&h.state.pool)
        .await
        .unwrap()
        .0
}

/// Angels of Death with nothing on TMDB to match: it is in the library all
/// the same, with AniList's progress. Once TMDB has it, the same library
/// entry moves to the TMDB show (one entry, not two) and AniList's progress
/// lands on its episodes; an old link to the AniList-only page follows.
#[tokio::test]
async fn an_entry_tmdb_lacks_is_kept_and_linked_once_tmdb_has_it() {
    let h = Harness::new().await;
    let mut angels = released(
        99423,
        "Angels of Death",
        "TV",
        Some(16),
        "CURRENT",
        5,
        "FINISHED",
        Some("2018-07-06"),
    );
    angels["score"] = json!(70.0);
    h.fake.lock().unwrap().entries = vec![angels];
    h.search("Angels of Death", json!([])).await;
    let cookie = h.connected().await;
    let summary = h.sync(1).await;
    assert_eq!(summary.local, 1, "{summary:?}");

    let kept = in_library(&h, &cookie, -99423)
        .await
        .expect("kept in the library");
    assert_eq!(kept["entry"]["status"], json!("watching"), "{kept}");
    assert_eq!(kept["entry"]["rating"], json!(70.0), "{kept}");
    assert_eq!(kept["watched_episodes"], json!(5), "{kept}");
    assert_eq!(kept["total_episodes"], json!(16), "{kept}");
    let page = h.get("/api/titles/tv/-99423", &cookie).await;
    assert_eq!(page["anilist_only"], json!(true), "{page}");
    let found = h.get("/api/library?q=angels%20of%20DEATH", &cookie).await;
    assert_eq!(
        found["items"].as_array().map(Vec::len),
        Some(1),
        "found by name like any title: {found}"
    );
    let entry = h.entry(&cookie, 99423).await;
    assert_eq!(
        entry["kept"]["id"],
        json!(h.tid("tv", -99423).await),
        "{entry}"
    );
    assert_eq!(entry["state"], json!("unsure"));

    // Synced again with nothing new: not looked up again, nothing doubled.
    h.sync(1).await;
    assert_eq!(
        count(&h, "SELECT tries FROM anilist_entry WHERE media_id = 99423").await,
        1,
        "waits for its next look"
    );
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM library_entry WHERE user_id = 1").await,
        1
    );

    // An episode watched elsewhere reaches Frog while it waits.
    h.fake.lock().unwrap().entries[0]["progress"] = json!(6);
    h.sync(1).await;
    let kept = in_library(&h, &cookie, -99423).await.unwrap();
    assert_eq!(kept["watched_episodes"], json!(6), "{kept}");

    // TMDB gets it. The next look finds it and links the same entry.
    {
        let mut fake = h.fake.lock().unwrap();
        fake.lists.push(json!({ "anilist_id": 99423, "type": "TV",
            "themoviedb_id": { "tv": 7400 }, "season": { "tmdb": 1 }, "episode_offset": { "tmdb": 0 } }));
    }
    let s1 = weekly("2018-07-06", 16);
    h.show(7400, "Angels of Death", &[(1, dates(&s1))], None)
        .await;
    sqlx::query("DELETE FROM anime_map_state")
        .execute(&h.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE anilist_entry SET next_try_at = NULL")
        .execute(&h.state.pool)
        .await
        .unwrap();
    h.sync(1).await;

    assert!(in_library(&h, &cookie, -99423).await.is_none());
    let show = in_library(&h, &cookie, 7400)
        .await
        .expect("on the TMDB show");
    assert_eq!(show["watched_episodes"], json!(6), "{show}");
    assert_eq!(show["entry"]["status"], json!("watching"), "{show}");
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM library_entry WHERE user_id = 1").await,
        1,
        "one entry, not two"
    );
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM title_merge WHERE user_id = 1").await,
        1
    );
    // The old AniList address opens the TMDB show, which answers to the
    // AniList id too; the old title by its own id says where it went.
    let tmdb_show = h.tid("tv", 7400).await;
    let old = h.get("/api/titles/tv/-99423", &cookie).await;
    assert_eq!(old["item"]["id"], json!(tmdb_show), "{old}");
    assert_eq!(old["external"]["anilist"], json!([99423]), "{old}");
    let kept_id =
        frog::catalog::find_exact(&h.state.pool, &frog::catalog::ExternalId::AniList(99423))
            .await
            .unwrap()
            .expect("the AniList title is kept, folded in");
    let by_id = h.get(&format!("/api/t/{kept_id}"), &cookie).await;
    assert_eq!(by_id["moved_to"], json!({ "id": tmdb_show }), "{by_id}");
    let rated = h.get("/api/titles/tv/7400", &cookie).await;
    assert!(
        rated["ratings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["rating"] == json!(70.0)),
        "the AniList score lands on the season: {}",
        rated["ratings"]
    );

    // And again: nothing moves twice, nothing is ticked twice.
    h.sync(1).await;
    h.sync(1).await;
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM watch_event WHERE user_id = 1").await,
        6
    );
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM title_merge WHERE user_id = 1").await,
        1
    );
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM title_external WHERE provider = 'anilist'
                AND external_id = '99423' AND is_primary = 1"
        )
        .await,
        1,
        "the AniList-only title is kept, pointing on, never reused"
    );
}

/// A long list comes in chunks and all of them are read. A failed read
/// changes nothing: no progress lost, no entry marked removed, and the last
/// good sync stays the last good sync, with the next try put off.
#[tokio::test]
async fn a_list_is_read_whole_and_a_failed_read_changes_nothing() {
    let h = Harness::new().await;
    seed_tmdb(&h).await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.per_chunk = Some(2);
        fake.entries = vec![entry(113415, "JUJUTSU KAISEN", "TV", 24, "CURRENT", 7, 0)];
        for id in 1..=4 {
            fake.entries
                .push(entry(70000 + id, "Set Aside", "TV", 12, "PLANNING", 0, 0));
        }
    }
    let cookie = h.connected().await;
    for id in 1..=4 {
        sqlx::query(
            "INSERT INTO anilist_entry (user_id, media_id, name, state, manual, updated_at)
             VALUES (1, ?, 'Set Aside', 'ignored', 1, '2026-01-01T00:00:00Z')",
        )
        .bind(70000 + id)
        .execute(&h.state.pool)
        .await
        .unwrap();
    }
    let summary = h.sync(1).await;
    assert_eq!(summary.entries, 5, "{summary:?}");
    assert_eq!(h.fake.lock().unwrap().list_calls, 3, "three chunks of two");
    assert_eq!(h.watched(95479).await.len(), 7);
    let good: (Option<String>,) =
        sqlx::query_as("SELECT synced_at FROM anilist_link WHERE user_id = 1")
            .fetch_one(&h.state.pool)
            .await
            .unwrap();

    // AniList down for longer than the retries last.
    h.fake.lock().unwrap().list_failures = vec![503; 8];
    {
        let _turn = SYNCS.lock().await;
        assert!(frog::anilist::sync_user(&h.state, 1).await.is_err());
    }
    let after: (Option<String>, Option<String>, i64, Option<String>) = sqlx::query_as(
        "SELECT synced_at, last_error, failures, retry_at FROM anilist_link WHERE user_id = 1",
    )
    .fetch_one(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(after.0, good.0, "a failure is not a sync");
    assert!(after.1.is_some());
    assert_eq!(after.2, 1);
    assert!(after.3.unwrap() > chrono::Utc::now().to_rfc3339());
    assert_eq!(h.watched(95479).await.len(), 7, "nothing taken away");
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM anilist_entry WHERE gone_at IS NOT NULL"
        )
        .await,
        0
    );
    // Waiting out the failure: the worker leaves the account for now...
    sqlx::query("UPDATE anilist_link SET synced_at = '2020-01-01T00:00:00Z'")
        .execute(&h.state.pool)
        .await
        .unwrap();
    assert!(frog::anilist::due(&h.state.pool).await.unwrap().is_empty());
    // ...and once the wait is over, including after a restart, it is back.
    sqlx::query("UPDATE anilist_link SET retry_at = '2020-01-01T00:00:00Z'")
        .execute(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(frog::anilist::due(&h.state.pool).await.unwrap(), vec![1]);

    // A rate limit is waited out as AniList asks, inside the same sync.
    h.fake.lock().unwrap().list_failures = vec![429, 429];
    let summary = h.sync(1).await;
    assert_eq!(summary.entries, 5);
    let (failures, error): (i64, Option<String>) =
        sqlx::query_as("SELECT failures, last_error FROM anilist_link WHERE user_id = 1")
            .fetch_one(&h.state.pool)
            .await
            .unwrap();
    assert_eq!((failures, error), (0, None));

    // An entry taken off AniList stays in Frog, marked.
    h.fake.lock().unwrap().entries.remove(0);
    let summary = h.sync(1).await;
    assert_eq!(summary.removed, 1, "{summary:?}");
    assert_eq!(h.watched(95479).await.len(), 7);
    assert!(in_library(&h, &cookie, 95479).await.is_some());
    assert_eq!(h.entry(&cookie, 113415).await["gone"], json!(true));

    // An answer with no entries at all is not read as everything removed.
    h.fake.lock().unwrap().entries.clear();
    let summary = h.sync(1).await;
    assert_eq!(summary.removed, 0, "{summary:?}");
}

/// A refused token stops the retries until the person connects again.
#[tokio::test]
async fn a_refused_token_waits_for_a_reconnect() {
    let h = Harness::new().await;
    let cookie = h.connected().await;
    h.fake.lock().unwrap().list_failures = vec![401];
    {
        let _turn = SYNCS.lock().await;
        assert!(frog::anilist::sync_user(&h.state, 1).await.is_err());
    }
    let status = h.get("/api/anilist", &cookie).await;
    assert_eq!(status["needs_reconnect"], json!(true), "{status}");
    assert_eq!(status["next_sync_at"], Value::Null);
    assert!(status["error"]
        .as_str()
        .unwrap()
        .contains("Connect it again"));
    sqlx::query("UPDATE anilist_link SET retry_at = NULL, synced_at = NULL")
        .execute(&h.state.pool)
        .await
        .unwrap();
    assert!(
        frog::anilist::due(&h.state.pool).await.unwrap().is_empty(),
        "not retried behind the person's back"
    );
}

/// Progress lowered on AniList takes back only what AniList ticked; an
/// episode logged in Frog stays logged.
#[tokio::test]
async fn a_lower_progress_on_anilist_keeps_watches_logged_in_frog() {
    let h = Harness::new().await;
    seed_tmdb(&h).await;
    h.fake.lock().unwrap().entries =
        vec![entry(113415, "JUJUTSU KAISEN", "TV", 24, "CURRENT", 10, 0)];
    let cookie = h.connected().await;
    h.sync(1).await;
    assert_eq!(h.watched(95479).await.len(), 10);

    // Episode 11 watched and logged in Frog: AniList is left as it was.
    let (code, _, _, _) = h
        .call(
            "POST",
            &format!("/api/t/{}/episodes/1/11/watch", h.tid("tv", 95479).await),
            Some(json!({ "watched": true })),
            Some(&cookie),
        )
        .await;
    assert!(code.is_success(), "{code}");
    h.sync(1).await;
    assert!(h.saves().is_empty(), "{:?}", h.saves());

    // Corrected down to 8 on AniList.
    h.fake.lock().unwrap().entries[0]["progress"] = json!(8);
    h.sync(1).await;
    let seen: Vec<i64> = h.watched(95479).await.into_iter().map(|e| e.1).collect();
    assert_eq!(seen, vec![1, 2, 3, 4, 5, 6, 7, 8, 11]);
}

/// Two syncs of one account at once: one runs, the other steps aside, and
/// running it again and again changes nothing.
#[tokio::test]
async fn overlapping_and_repeated_syncs_change_nothing() {
    let h = Harness::new().await;
    seed_tmdb(&h).await;
    h.fake.lock().unwrap().entries = vec![
        entry(113415, "JUJUTSU KAISEN", "TV", 24, "CURRENT", 12, 0),
        entry(9000, "JUJUTSU KAISEN 0", "MOVIE", 1, "COMPLETED", 1, 0),
    ];
    h.connected().await;
    let (a, b) = {
        let _turn = SYNCS.lock().await;
        tokio::join!(
            frog::anilist::sync_user(&h.state, 1),
            frog::anilist::sync_user(&h.state, 1)
        )
    };
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.entries + b.entries, 2, "one ran, one stepped aside");
    let watches = "SELECT COUNT(*) FROM watch_event WHERE user_id = 1";
    assert_eq!(count(&h, watches).await, 13);
    for _ in 0..3 {
        let summary = h.sync(1).await;
        assert_eq!(summary.to_frog, 0, "{summary:?}");
    }
    assert_eq!(count(&h, watches).await, 13);
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM library_entry WHERE user_id = 1").await,
        2
    );
    assert!(h.saves().is_empty(), "{:?}", h.saves());
}

/// Moving one account's data between titles: clashes resolved without loss,
/// the other account untouched, all or nothing, and safe to repeat.
#[tokio::test]
async fn moving_title_data_keeps_everything_and_everyone_apart() {
    let h = Harness::new().await;
    h.connected().await;
    let pool = &h.state.pool;
    for sql in [
        "INSERT INTO app_user (id, username, password_hash, created_at)
         VALUES (2, 'zoe', 'x', '2026-01-01T00:00:00Z')",
        "INSERT INTO title_seed (id, kind, tmdb_id, name, genres_json, anilist_id)
         VALUES (100, 'movie', -5, 'Pond Film', '[]', 5)",
        "INSERT INTO title_seed (id, kind, tmdb_id, name, genres_json) VALUES (101, 'movie', 5, 'Pond Film', '[]')",
        "INSERT INTO library_entry (user_id, title_id, status, rating, notes, added_at, updated_at)
         VALUES (1, 100, 'completed', 80, 'from AniList', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z')",
        "INSERT INTO library_entry (user_id, title_id, status, notes, added_at, updated_at)
         VALUES (1, 101, 'planned', 'mine', '2026-01-05T00:00:00Z', '2026-01-05T00:00:00Z')",
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         VALUES (2, 100, 'watching', '2026-01-03T00:00:00Z', '2026-01-03T00:00:00Z')",
        "INSERT INTO watch_event (user_id, title_id, watched_at, source, undated)
         VALUES (1, 100, '2026-01-02T00:00:00Z', 'anilist', 1)",
        "INSERT INTO shelf (id, user_id, name, kind, created_at)
         VALUES (1, 1, 'Pond', 'manual', '2026-01-01T00:00:00Z')",
        "INSERT INTO shelf_item (shelf_id, title_id) VALUES (1, 100)",
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }

    // Fails halfway: nothing of it is left behind.
    sqlx::query(
        "CREATE TRIGGER boom BEFORE INSERT ON title_merge BEGIN SELECT RAISE(ABORT, 'boom'); END",
    )
    .execute(pool)
    .await
    .unwrap();
    assert!(frog::store::move_title_data(pool, 1, 100, 101, "test")
        .await
        .is_err());
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM library_entry WHERE user_id = 1 AND title_id = 100"
        )
        .await,
        1
    );
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM shelf_item WHERE title_id = 100").await,
        1
    );
    sqlx::query("DROP TRIGGER boom")
        .execute(pool)
        .await
        .unwrap();

    assert!(frog::store::move_title_data(pool, 1, 100, 101, "test")
        .await
        .unwrap());
    let (status, rating, notes, added): (String, Option<f64>, Option<String>, String) =
        sqlx::query_as(
            "SELECT status, rating, notes, added_at FROM library_entry
              WHERE user_id = 1 AND title_id = 101",
        )
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(status, "completed", "the further-along status");
    assert_eq!(rating, Some(80.0));
    assert_eq!(
        notes.as_deref(),
        Some("mine\n\nfrom AniList"),
        "both notes kept"
    );
    assert_eq!(added, "2026-01-02T00:00:00Z", "the earlier date");
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM watch_event WHERE user_id = 1 AND title_id = 101"
        )
        .await,
        1
    );
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM shelf_item WHERE title_id = 101").await,
        1
    );
    // The other account still has the old title, so it is not folded away.
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM library_entry WHERE user_id = 2 AND title_id = 100"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM title WHERE id = 100 AND merged_into IS NULL"
        )
        .await,
        1
    );

    // Again: nothing left to move, nothing written twice.
    assert!(!frog::store::move_title_data(pool, 1, 100, 101, "test")
        .await
        .unwrap());
    assert_eq!(count(&h, "SELECT COUNT(*) FROM title_merge").await, 1);
    // The second account moves too; then the old title points on.
    assert!(frog::store::move_title_data(pool, 2, 100, 101, "test")
        .await
        .unwrap());
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM title WHERE id = 100 AND merged_into = 101"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM library_entry WHERE title_id = 101"
        )
        .await,
        2
    );
}

/// What AniList says about Angels of Death, as its detail query answers.
fn angels_detail() -> Value {
    let cdn = "https://s4.anilist.co/file/anilistcdn";
    let work = |id: i64, name: &str, format: &str| {
        json!({ "id": id, "type": "ANIME", "format": format,
                "title": { "english": name, "romaji": name },
                "coverImage": { "extraLarge": format!("{cdn}/media/anime/cover/large/bx{id}.jpg") },
                "bannerImage": null, "startDate": { "year": 2019, "month": 1, "day": 1 },
                "averageScore": 70, "popularity": 1000 })
    };
    json!({
        "id": 99423, "format": "TV", "status": "FINISHED", "episodes": 4, "duration": 24,
        "countryOfOrigin": "JP",
        "title": { "english": "Angels of Death", "romaji": "Satsuriku no Tenshi", "native": "殺戮の天使" },
        "description": "Rachel wakes up in a basement.<br><br>She meets <i>Zack</i>.<br>\n<br>(Source: Crunchyroll)",
        "startDate": { "year": 2018, "month": 7, "day": 6 },
        "endDate": { "year": 2018, "month": 7, "day": 27 },
        "coverImage": { "extraLarge": format!("{cdn}/media/anime/cover/large/bx99423-a.jpg"), "color": "#e4505d" },
        "bannerImage": format!("{cdn}/media/anime/banner/99423-b.jpg"),
        "genres": ["Horror", "Mystery", "Psychological"],
        "averageScore": 64, "popularity": 120000,
        "trailer": { "id": "abc123", "site": "youtube" },
        "nextAiringEpisode": null,
        "studios": { "nodes": [{ "id": 7, "name": "J.C.Staff" }] },
        "characters": { "edges": [
            { "role": "MAIN", "node": { "id": 1, "name": { "full": "Rachel Gardner" } },
              "voiceActors": [{ "id": 95000, "name": { "full": "Haruka Chisuga" },
                                "image": { "large": format!("{cdn}/staff/large/n95000.png") } }] },
            { "role": "MAIN", "node": { "id": 2, "name": { "full": "Nobody Voiced" } }, "voiceActors": [] }
        ] },
        "staff": { "edges": [
            { "role": "Director", "node": { "id": 96000, "name": { "full": "Kentarou Suzuki" }, "image": { "large": null } } },
            { "role": "Original Creator", "node": { "id": 96001, "name": { "full": "Makoto Sanada" }, "image": { "large": null } } },
            { "role": "Key Animation (ep 2)", "node": { "id": 96002, "name": { "full": "Someone" }, "image": { "large": null } } }
        ] },
        "relations": { "edges": [
            { "relationType": "SIDE_STORY", "node": work(99629, "Angels of Death Episode.Eddie", "ONA") }
        ] },
        "recommendations": { "nodes": [
            { "mediaRecommendation": work(145064, "JUJUTSU KAISEN Season 2", "TV") }
        ] },
        "externalLinks": [
            { "id": 5, "site": "Crunchyroll", "url": "https://www.crunchyroll.com/series/angels", "type": "STREAMING",
              "icon": format!("{cdn}/link/icon/5-cr.png") },
            { "id": 6, "site": "Twitter", "url": "https://twitter.com/angels", "type": "SOCIAL", "icon": null }
        ],
        "streamingEpisodes": [
            { "title": "Episode 1 - Kill me...", "thumbnail": "https://img1.ak.crunchyroll.com/i/1.jpg" },
            { "title": "Episode 3 - Just one more", "thumbnail": "https://elsewhere.example/3.jpg" }
        ]
    })
}

/// An anime TMDB does not have looks and behaves like any other title: the
/// cover, synopsis, cast, staff, studio, trailer, where it streams, what is
/// related, and episodes that tick, sync and count, all from AniList. People
/// and studios open their own pages. When TMDB finds it, a watch logged in
/// Frog goes with it to the same episode.
#[tokio::test]
async fn an_anilist_only_show_is_a_title_like_any_other() {
    let h = Harness::new().await;
    {
        let mut fake = h.fake.lock().unwrap();
        fake.details.insert(99423, angels_detail());
        fake.people.insert(
            95000,
            json!({
                "id": 95000, "name": { "full": "Haruka Chisuga" },
                "description": "A voice actress.", "image": { "large": null },
                "dateOfBirth": { "year": 1991, "month": 10, "day": 12 },
                "dateOfDeath": { "year": null, "month": null, "day": null },
                "homeTown": "Tokyo", "primaryOccupations": ["Voice Actor"],
                "characterMedia": { "edges": [
                    { "characters": [{ "name": { "full": "Rachel Gardner" } }],
                      "node": { "id": 99423, "type": "ANIME", "format": "TV",
                                "title": { "english": "Angels of Death", "romaji": null },
                                "coverImage": { "extraLarge": null }, "bannerImage": null,
                                "startDate": { "year": 2018, "month": 7, "day": 6 } } }
                ] },
                "staffMedia": { "edges": [] }
            }),
        );
        fake.studios.insert(
            7,
            json!({
                "id": 7, "name": "J.C.Staff", "siteUrl": "https://anilist.co/studio/7",
                "media": { "nodes": [
                    { "id": 99423, "type": "ANIME", "format": "TV",
                      "title": { "english": "Angels of Death", "romaji": null },
                      "coverImage": { "extraLarge": null }, "bannerImage": null,
                      "startDate": { "year": 2018, "month": 7, "day": 6 } }
                ] }
            }),
        );
        fake.entries = vec![released(
            99423,
            "Angels of Death",
            "TV",
            Some(4),
            "CURRENT",
            2,
            "FINISHED",
            Some("2018-07-06"),
        )];
    }
    h.search("Angels of Death", json!([])).await;
    let cookie = h.connected().await;
    h.sync(1).await;

    let item = in_library(&h, &cookie, -99423)
        .await
        .expect("in the library");
    assert_eq!(
        item["poster_path"],
        json!("/anilist/media/anime/cover/large/bx99423-a.jpg")
    );
    assert_eq!(item["watched_episodes"], json!(2), "{item}");
    assert_eq!(item["total_episodes"], json!(4), "{item}");
    assert_eq!(item["entry"]["status"], json!("watching"), "{item}");
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM watch_event WHERE user_id = 1").await,
        2,
        "two episodes ticked, as on AniList"
    );

    let page = h.get("/api/titles/tv/-99423", &cookie).await;
    let it = &page["item"];
    assert_eq!(it["name"], json!("Angels of Death"));
    assert_eq!(
        it["overview"],
        json!("Rachel wakes up in a basement.\n\nShe meets Zack.")
    );
    assert_eq!(
        it["genres"],
        json!(["Animation", "Horror", "Mystery", "Psychological"])
    );
    assert_eq!(
        it["backdrop_path"],
        json!("/anilist/media/anime/banner/99423-b.jpg")
    );
    assert_eq!(it["runtime"], json!(24));
    assert_eq!(it["tmdb_vote"], json!(6.4));
    assert_eq!(it["last_air_date"], json!("2018-07-27"));
    let cast = page["cast"].as_array().unwrap();
    assert_eq!(cast.len(), 1, "only voiced characters: {cast:?}");
    assert_eq!(cast[0]["id"], json!(-95000));
    assert_eq!(cast[0]["character"], json!("Rachel Gardner"));
    assert_eq!(
        cast[0]["profile_path"],
        json!("/anilist/staff/large/n95000.png")
    );
    let crew: Vec<(Value, Value)> = page["crew"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].clone(), c["job"].clone()))
        .collect();
    assert_eq!(
        crew,
        vec![
            (json!(-96001), json!("Creator")),
            (json!(-96000), json!("Director"))
        ]
    );
    assert_eq!(page["trailers"][0]["key"], json!("abc123"));
    let streams = page["providers"]["flatrate"].as_array().unwrap();
    assert_eq!(streams.len(), 1, "streaming sites only: {streams:?}");
    assert_eq!(
        streams[0]["url"],
        json!("https://www.crunchyroll.com/series/angels")
    );
    assert_eq!(
        streams[0]["logo_path"],
        json!("/anilist/link/icon/5-cr.png")
    );
    assert_eq!(page["seasons"][0]["episode_count"], json!(4));
    assert_eq!(page["collection"]["parts"][0]["anilist_id"], json!(99629));
    assert_eq!(page["collection"]["parts"][0]["tmdb_id"], Value::Null);
    assert_eq!(
        page["recommendations"][0]["tmdb_id"],
        json!(95479),
        "a recommendation TMDB has opens the TMDB title"
    );

    let season = h
        .get(
            &format!("/api/t/{}/seasons/1", h.tid("tv", -99423).await),
            &cookie,
        )
        .await;
    let eps = season["episodes"].as_array().unwrap();
    assert_eq!(eps.len(), 4);
    assert_eq!(eps[0]["name"], json!("Kill me..."));
    assert_eq!(eps[0]["still_path"], json!("/crunchyroll/i/1.jpg"));
    assert_eq!(eps[0]["air_date"], json!("2018-07-06"));
    assert_eq!(
        eps[2]["still_path"],
        Value::Null,
        "an untrusted host is not used"
    );
    assert_eq!(eps[0]["watched"], json!(true));
    assert_eq!(eps[2]["watched"], json!(false));

    // Home's hero is the most live show, and is left out of the row below.
    let home = h.get("/api/home", &cookie).await;
    assert_eq!(
        home["hero"]["item"]["id"],
        json!(h.tid("tv", -99423).await),
        "continued from Home like any show: {home}"
    );
    assert_eq!(home["hero"]["next"]["episode_number"], json!(3));
    assert_eq!(home["hero"]["next"]["aired"], json!(true));

    let person = h.get("/api/person/-95000", &cookie).await;
    assert_eq!(person["name"], json!("Haruka Chisuga"));
    assert_eq!(person["known_for_department"], json!("Acting"));
    assert_eq!(person["credits"][0]["item"]["anilist_id"], json!(99423));
    assert_eq!(
        person["credits"][0]["item"]["id"],
        json!(h.tid("tv", -99423).await)
    );
    assert_eq!(person["credits"][0]["role"], json!("Rachel Gardner"));
    assert_eq!(
        person["credits"][0]["item"]["in_library"],
        json!("watching")
    );
    let studio = h.get("/api/studios/-7", &cookie).await;
    assert_eq!(studio["name"], json!("J.C.Staff"));
    assert_eq!(studio["titles"][0]["anilist_id"], json!(99423));

    // An AniList address for a show TMDB has opens the TMDB title, and no
    // AniList copy of it is made.
    seed_tmdb(&h).await;
    let elsewhere = h.get("/api/titles/tv/-145064", &cookie).await;
    assert_eq!(
        elsewhere["item"]["id"],
        json!(h.tid("tv", 95479).await),
        "{elsewhere}"
    );
    assert_eq!(
        count(
            &h,
            "SELECT COUNT(*) FROM title_external WHERE provider = 'anilist' AND external_id = '145064'"
        )
        .await,
        0
    );

    // Episode 3 watched and logged in Frog: AniList is left as it was.
    let (code, _, _, _) = h
        .call(
            "POST",
            &format!("/api/t/{}/episodes/1/3/watch", h.tid("tv", -99423).await),
            Some(json!({ "watched": true, "watched_at": "2026-10-20T20:00:00Z" })),
            Some(&cookie),
        )
        .await;
    assert!(code.is_success(), "{code}");
    h.sync(1).await;
    assert!(h.saves().is_empty(), "{:?}", h.saves());
    h.sync(1).await;
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM watch_event WHERE user_id = 1").await,
        3,
        "nothing doubled"
    );

    // TMDB gets it: the logged watch goes to the same episode there.
    h.fake.lock().unwrap().lists.push(json!({ "anilist_id": 99423, "type": "TV",
        "themoviedb_id": { "tv": 7400 }, "season": { "tmdb": 1 }, "episode_offset": { "tmdb": 0 } }));
    let s1 = weekly("2018-07-06", 4);
    h.show(7400, "Angels of Death", &[(1, dates(&s1))], None)
        .await;
    sqlx::query("DELETE FROM anime_map_state")
        .execute(&h.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE anilist_entry SET next_try_at = NULL")
        .execute(&h.state.pool)
        .await
        .unwrap();
    h.sync(1).await;
    let show = in_library(&h, &cookie, 7400)
        .await
        .expect("on the TMDB show");
    assert_eq!(show["watched_episodes"], json!(3), "{show}");
    assert!(in_library(&h, &cookie, -99423).await.is_none());
    let (source, at): (String, String) = sqlx::query_as(
        "SELECT w.source, w.watched_at FROM watch_event w
           JOIN episode e ON e.id = w.episode_id JOIN title t ON t.id = e.title_id
          WHERE w.user_id = 1 AND (SELECT tmdb_id FROM title_tmdb WHERE title_id = t.id) = 7400 AND e.episode_number = 3",
    )
    .fetch_one(&h.state.pool)
    .await
    .unwrap();
    assert_ne!(source, "anilist", "the watch logged in Frog, kept");
    assert_eq!(at, "2026-10-20T20:00:00Z");
    assert_eq!(
        count(&h, "SELECT COUNT(*) FROM watch_event WHERE user_id = 1").await,
        3
    );
}
