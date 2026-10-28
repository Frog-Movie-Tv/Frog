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

/// What the stand-in AniList holds, and every save Frog sent it.
#[derive(Default)]
struct Fake {
    entries: Vec<Value>,
    saves: Vec<Value>,
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

async fn graphql(State(fake): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let query = body["query"].as_str().unwrap_or("");
    let vars = &body["variables"];
    let mut fake = fake.lock().unwrap();
    if query.contains("Viewer") {
        return Json(json!({ "data": { "Viewer": { "id": 77, "name": "pondwatcher" } } }));
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

async fn anime_ids() -> Json<Value> {
    Json(json!({
        "1": { "anilist_id": 113415, "tvdb_id": 377543, "tvdb_season": 1, "tvdb_epoffset": 0 },
        "2": { "anilist_id": 145064, "tvdb_id": 377543, "tvdb_season": 2, "tvdb_epoffset": 0 },
        "3": { "anilist_id": 9000, "imdb_id": "tt0900000" }
    }))
}

/// Fribb's anime-lists: TMDB ids, and TMDB's own season and offset.
async fn anime_lists() -> Json<Value> {
    Json(json!([
        { "anilist_id": 145064, "type": "TV", "themoviedb_id": { "tv": 95479 },
          "season": { "tvdb": 2, "tmdb": 1 }, "episode_offset": { "tmdb": 24 } },
        { "anilist_id": 20000, "type": "MOVIE", "themoviedb_id": { "movie": 600 } },
        // Two remakes the list files under the same season: neither is placed.
        { "anilist_id": 20001, "type": "TV", "themoviedb_id": { "tv": 800 },
          "season": { "tmdb": 1 } },
        { "anilist_id": 20002, "type": "TV", "themoviedb_id": { "tv": 800 },
          "season": { "tmdb": 1 } }
    ]))
}

struct Harness {
    state: AppState,
    fake: Shared,
    _dir: tempfile::TempDir,
}

impl Harness {
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
        Self {
            state: AppState::new(pool, config),
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
async fn anilist_syncs_both_ways_and_places_seasons_by_id() {
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
    assert_eq!(item["entry"]["rating"], json!(80.0));
    let (_, film, _, _) = h
        .call("GET", "/api/titles/movie/500", None, Some(&cookie))
        .await;
    assert_eq!(film["item"]["entry"]["status"], json!("completed"));
    assert!(
        h.saves().is_empty(),
        "nothing in Frog to send yet: {:?}",
        h.saves()
    );

    // A tick in Frog goes to AniList, on the season it belongs to.
    let (code, _, _, _) = h
        .call(
            "POST",
            "/api/titles/tv/95479/episodes/1/30/watch",
            Some(json!({})),
            Some(&cookie),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    h.sync(user).await;
    let saves = h.saves();
    assert_eq!(saves.len(), 1, "{saves:?}");
    assert_eq!(saves[0]["mediaId"], json!(145064));
    assert_eq!(saves[0]["progress"], json!(6));
    assert_eq!(saves[0]["status"], json!("CURRENT"));
    assert_eq!(saves[0]["scoreRaw"], json!(0), "a tick sends no score");

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
    assert_eq!(h.saves().len(), 1, "nothing echoed back");

    // A rating in Frog reaches every season of the show.
    h.call(
        "PUT",
        "/api/titles/tv/95479/library",
        Some(json!({ "rating": 95.0 })),
        Some(&cookie),
    )
    .await;
    h.sync(user).await;
    let scores: Vec<Value> = h
        .saves()
        .iter()
        .skip(1)
        .map(|s| s["scoreRaw"].clone())
        .collect();
    assert_eq!(scores, vec![json!(95), json!(95)]);

    // Taking the film out of Frog leaves AniList alone, and it is not re-added.
    h.call(
        "DELETE",
        "/api/titles/movie/500/library",
        None,
        Some(&cookie),
    )
    .await;
    let before = h.saves().len();
    h.sync(user).await;
    h.sync(user).await;
    assert_eq!(h.saves().len(), before);
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
    // Tests run side by side and all use account 1; a sync from another one
    // would make this one return at once without trying.
    while frog::anilist::is_syncing(1) {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(frog::anilist::sync_user(&h.state, 1).await.is_err());
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
        .call("GET", "/api/titles/tv/700/seasons/2", None, Some(&cookie))
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
