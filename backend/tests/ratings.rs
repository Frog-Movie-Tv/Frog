//! Scores from other sites land on Frog's own title, matched by provider id,
//! and asking for them never makes a title or writes to AniList.

mod common;

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use frog::config::{AniListClient, Config, MdbListClient};
use frog::state::AppState;
use serde_json::{json, Value};

#[derive(Default)]
struct Fake {
    /// Every request MDBList saw, as "METHOD path ids".
    calls: Vec<String>,
    /// AniList queries seen, to prove none of them writes.
    graphql: Vec<String>,
    limited: bool,
}

type Shared = Arc<Mutex<Fake>>;

fn item(tmdb: i64, imdb: f64, tomatoes: f64) -> Value {
    json!({
        "ids": { "tmdb": tmdb, "imdb": "tt1", "mal": null },
        "ratings": [
            { "source": "imdb", "value": imdb / 10.0, "score": imdb, "votes": 100 },
            { "source": "tomatoes", "value": tomatoes, "score": tomatoes, "votes": 10 },
            { "source": "metacritic", "value": null, "score": null, "votes": null }
        ]
    })
}

async fn batch(
    State(fake): State<Shared>,
    Path(kind): Path<String>,
    Json(body): Json<Value>,
) -> (StatusCode, HeaderMap, Json<Value>) {
    let mut f = fake.lock().unwrap();
    f.calls.push(format!("POST {kind} {}", body["ids"]));
    let mut headers = HeaderMap::new();
    headers.insert("x-ratelimit-remaining", HeaderValue::from_static("900"));
    if f.limited {
        return (StatusCode::TOO_MANY_REQUESTS, headers, Json(json!({})));
    }
    let items: Vec<Value> = body["ids"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_i64)
        // MDBList leaves out what it does not know.
        .filter(|id| *id != 404)
        .map(|id| item(id, 88.0, 81.0))
        .collect();
    (StatusCode::OK, headers, Json(json!(items)))
}

async fn by_mal(State(fake): State<Shared>, Path(id): Path<i64>) -> Json<Value> {
    fake.lock().unwrap().calls.push(format!("GET mal {id}"));
    Json(json!({
        "ids": { "tmdb": null, "mal": id },
        "ratings": [{ "source": "myanimelist", "value": 7.9, "score": 79, "votes": 5 }]
    }))
}

async fn anilist(State(fake): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let query = body["query"].as_str().unwrap_or_default().to_string();
    fake.lock().unwrap().graphql.push(query);
    let media: Vec<Value> = body["variables"]["ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .map(|id| json!({ "id": id, "idMal": id + 1000, "averageScore": 85, "popularity": 1 }))
        .collect();
    Json(json!({ "data": { "Page": { "media": media } } }))
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
            .route("/tmdb/{kind}/", post(batch))
            .route("/mal/any/{id}/", get(by_mal))
            .route("/graphql", post(anilist))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });

        let dir = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite:{}?mode=rwc",
            dir.path().join("t.db").to_string_lossy().replace('\\', "/")
        );
        let config = Config {
            tmdb_api_key: "test".into(),
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
                id: "1".into(),
                secret: "s".into(),
                redirect_uri: "http://x/cb".into(),
                api_url: format!("{base}/graphql"),
                site_url: base.clone(),
                anime_ids_url: base.clone(),
                anime_lists_url: base.clone(),
            }),
            fanart: None,
            mdblist: Some(MdbListClient {
                key: "secret-key".into(),
                api_url: base,
                budget: Arc::default(),
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

    async fn exec(&self, sql: &str) {
        sqlx::query(sql).execute(&self.state.pool).await.unwrap();
    }

    async fn count_titles(&self) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM title")
            .fetch_one(&self.state.pool)
            .await
            .unwrap();
        n
    }

    async fn scores(&self, title_id: i64) -> Vec<(String, f64)> {
        let view = frog::ratings::of_title(&self.state.pool, title_id)
            .await
            .unwrap();
        view["ratings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["source"].as_str().unwrap().to_string(),
                    r["score"].as_f64().unwrap(),
                )
            })
            .collect()
    }
}

#[tokio::test]
async fn every_site_lands_on_the_one_title_and_none_makes_another() {
    let h = Harness::new().await;
    // A film TMDB supplies; TMDB's show AniList splits into two entries; a
    // title kept from AniList that no list maps to TMDB.
    h.exec("INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (1, 'movie', 550, 'Film')")
        .await;
    h.exec("INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (2, 'tv', 1429, 'Show')")
        .await;
    h.exec(
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (3, 'tv', -777, 'Only AniList')",
    )
    .await;
    h.exec("UPDATE title SET tmdb_vote = 8.4 WHERE id IN (1, 2)")
        .await;
    h.exec("UPDATE title SET tmdb_vote = 7.5 WHERE id = 3")
        .await;
    h.exec(
        "INSERT INTO anime_map (anilist_id, tmdb_kind, tmdb_id, tmdb_season, tmdb_offset) VALUES
            (200, 'tv', 1429, 2, 0), (100, 'tv', 1429, 1, 0)",
    )
    .await;
    let before = h.count_titles().await;

    frog::ratings::refresh(&h.state, &[1, 2, 3], 0)
        .await
        .unwrap();

    assert_eq!(h.count_titles().await, before, "no title was made");

    assert_eq!(
        h.scores(1).await,
        [
            ("imdb".to_string(), 88.0),
            ("tomatoes".to_string(), 81.0),
            ("tmdb".to_string(), 84.0)
        ],
        "a missing Metacritic score is simply not there"
    );
    // TMDB's title shows AniList's score too, from its first season's entry.
    assert_eq!(
        h.scores(2).await,
        [
            ("imdb".to_string(), 88.0),
            ("tomatoes".to_string(), 81.0),
            ("tmdb".to_string(), 84.0),
            ("anilist".to_string(), 85.0)
        ]
    );
    // AniList's own title keeps its own score, and MyAnimeList's by id.
    assert_eq!(
        h.scores(3).await,
        [
            ("anilist".to_string(), 75.0),
            ("myanimelist".to_string(), 79.0)
        ]
    );

    let fake = h.fake.lock().unwrap();
    assert!(
        fake.calls.contains(&"POST movie [550]".to_string()),
        "{:?}",
        fake.calls
    );
    assert!(
        fake.calls.contains(&"POST show [1429]".to_string()),
        "{:?}",
        fake.calls
    );
    assert!(
        fake.calls.contains(&"GET mal 1777".to_string()),
        "{:?}",
        fake.calls
    );
    assert!(
        fake.graphql
            .iter()
            .all(|q| q.trim_start().starts_with("query")),
        "{:?}",
        fake.graphql
    );
}

#[tokio::test]
async fn titles_are_not_asked_about_again_until_they_are_due() {
    let h = Harness::new().await;
    h.exec("INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (1, 'movie', 550, 'Film')")
        .await;
    h.exec(
        "INSERT INTO app_user (id, username, password_hash, role, created_at)
         VALUES (1, 'owner', 'x', 'owner', '2026-10-26')",
    )
    .await;
    h.exec(
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         VALUES (1, 1, 'watching', 'x', 'x')",
    )
    .await;

    frog::ratings::refresh_due(&h.state).await;
    frog::ratings::refresh_due(&h.state).await;
    frog::ratings::for_page(&h.state, 1).await.unwrap();
    assert_eq!(h.fake.lock().unwrap().calls.len(), 1);
}

#[tokio::test]
async fn a_failed_request_keeps_what_was_stored_and_tries_again() {
    let h = Harness::new().await;
    h.exec("INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (1, 'movie', 550, 'Film')")
        .await;
    frog::ratings::refresh(&h.state, &[1], 0).await.unwrap();
    let kept = h.scores(1).await;
    assert!(!kept.is_empty());

    h.fake.lock().unwrap().limited = true;
    h.exec("UPDATE title_rating_check SET next_at = '2000-01-01'")
        .await;
    frog::ratings::refresh(&h.state, &[1], 0).await.unwrap();
    assert_eq!(h.scores(1).await, kept);
    let (next,): (String,) = sqlx::query_as("SELECT next_at FROM title_rating_check")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(next, "2000-01-01", "still due");
}

#[tokio::test]
async fn nothing_known_is_not_an_error() {
    let h = Harness::new().await;
    h.exec("INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (1, 'movie', 404, 'Lost')")
        .await;
    frog::ratings::refresh(&h.state, &[1], 0).await.unwrap();
    assert!(h.scores(1).await.is_empty());
    let view = frog::ratings::of_title(&h.state.pool, 1).await.unwrap();
    assert_eq!(view["age_rating"], Value::Null);
}
