//! Badges and how a profile looks: who may make and give badges, who sees
//! which, what goes when a badge or an account goes, and what the server
//! accepts for a profile's colour and banner.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::Engine;
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
            mdblist: None,
        };
        let pool = frog::db::connect(&url).await.expect("database");
        Self {
            state: AppState::new(pool, config),
            _dir: dir,
        }
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        cookie: Option<&str>,
    ) -> (StatusCode, Value, Option<String>, Vec<u8>) {
        let mut req = Request::builder().uri(path).method(method);
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
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
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value, cookie, bytes)
    }

    async fn get(&self, path: &str, cookie: Option<&str>) -> (StatusCode, Value) {
        let (s, v, _, _) = self.send("GET", path, None, cookie).await;
        (s, v)
    }

    async fn post(&self, path: &str, body: Value, cookie: Option<&str>) -> (StatusCode, Value) {
        let (s, v, _, _) = self.send("POST", path, Some(body), cookie).await;
        (s, v)
    }

    async fn put(&self, path: &str, body: Value, cookie: Option<&str>) -> (StatusCode, Value) {
        let (s, v, _, _) = self.send("PUT", path, Some(body), cookie).await;
        (s, v)
    }

    async fn delete(&self, path: &str, cookie: Option<&str>) -> (StatusCode, Value) {
        let (s, v, _, _) = self.send("DELETE", path, None, cookie).await;
        (s, v)
    }

    async fn count(&self, sql: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
            .0
    }
}

/// The owner, and a member they invited.
async fn two_accounts(h: &Harness) -> (String, String) {
    let (code, _, owner, _) = h
        .send(
            "POST",
            "/api/auth/setup",
            Some(
                json!({ "username": "mihai", "password": "a-long-enough-pass", "remember": true }),
            ),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let owner = owner.unwrap();
    let (_, invite) = h
        .post("/api/invites", json!({ "role": "user" }), Some(&owner))
        .await;
    let (code, body, member, _) = h
        .send(
            "POST",
            "/api/auth/register",
            Some(json!({
                "username": "zjef",
                "password": "another-long-pass",
                "remember": true,
                "invite": invite["code"],
            })),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    (owner, member.unwrap())
}

async fn id_of(h: &Harness, username: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>("SELECT id FROM app_user WHERE username = ?")
        .bind(username)
        .fetch_one(&h.state.pool)
        .await
        .unwrap()
        .0
}

fn png(width: u32, height: u32) -> String {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 200, 90, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    base64::engine::general_purpose::STANDARD.encode(out.into_inner())
}

fn badge(name: &str) -> Value {
    json!({
        "name": name,
        "description": "Shops there every day",
        "settings": { "zoom": 1.2, "tolerance": 30 },
        "large": png(1024, 1024),
        "small": png(128, 128),
        "original": png(300, 200),
    })
}

async fn make_badge(h: &Harness, cookie: &str, name: &str) -> i64 {
    let (code, body) = h.post("/api/admin/badges", badge(name), Some(cookie)).await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    body["id"].as_i64().unwrap()
}

#[tokio::test]
async fn badge_routes_refuse_anyone_below_administrator() {
    let h = Harness::new().await;
    let (owner, member) = two_accounts(&h).await;
    let id = make_badge(&h, &owner, "Albert Heijn").await;
    let member_id = id_of(&h, "zjef").await;

    for (method, path, body) in [
        ("GET", "/api/admin/badges".to_string(), None),
        ("POST", "/api/admin/badges".to_string(), Some(badge("Mine"))),
        ("GET", format!("/api/admin/badges/{id}"), None),
        (
            "PUT",
            format!("/api/admin/badges/{id}"),
            Some(badge("Mine")),
        ),
        ("DELETE", format!("/api/admin/badges/{id}"), None),
        ("GET", format!("/api/admin/badges/{id}/original"), None),
        ("GET", format!("/api/admin/badges/{id}/images/large"), None),
        (
            "PUT",
            format!("/api/admin/badges/{id}/holders/{member_id}"),
            None,
        ),
        (
            "DELETE",
            format!("/api/admin/badges/{id}/holders/{member_id}"),
            None,
        ),
    ] {
        let (code, _, _, _) = h.send(method, &path, body.clone(), Some(&member)).await;
        assert_eq!(code, StatusCode::FORBIDDEN, "{method} {path}");
        let (code, _, _, _) = h.send(method, &path, body, None).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{method} {path} signed out");
    }
    assert_eq!(h.count("SELECT COUNT(*) FROM badge").await, 1);
    assert_eq!(h.count("SELECT COUNT(*) FROM badge_award").await, 0);

    let (code, body) = h.get("/api/admin/badges", Some(&owner)).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["badges"][0]["name"], "Albert Heijn");
}

#[tokio::test]
async fn only_finished_round_pngs_are_kept() {
    let h = Harness::new().await;
    let (owner, _) = two_accounts(&h).await;

    let mut wrong = badge("Soft");
    wrong["large"] = json!(png(1000, 1000));
    let (code, body) = h.post("/api/admin/badges", wrong, Some(&owner)).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("1024"), "{body:?}");

    let mut wide = badge("Wide");
    wide["small"] = json!(png(128, 96));
    let (_, body) = h.post("/api/admin/badges", wide, Some(&owner)).await;
    assert!(
        body["error"].as_str().unwrap().contains("square"),
        "{body:?}"
    );

    let mut svg = badge("Vector");
    svg["original"] = json!(base64::engine::general_purpose::STANDARD
        .encode(br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#));
    let (code, _) = h.post("/api/admin/badges", svg, Some(&owner)).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    let mut jpeg = badge("Flat");
    jpeg["large"] = json!(base64::engine::general_purpose::STANDARD.encode([0xFF, 0xD8, 0xFF, 0]));
    let (_, body) = h.post("/api/admin/badges", jpeg, Some(&owner)).await;
    assert!(body["error"].as_str().unwrap().contains("PNG"), "{body:?}");

    let mut alone = badge("Half");
    alone["original"] = Value::Null;
    let (code, _) = h.post("/api/admin/badges", alone, Some(&owner)).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    assert_eq!(h.count("SELECT COUNT(*) FROM badge").await, 0);

    make_badge(&h, &owner, "FEM BOY").await;
    let (code, body) = h
        .post("/api/admin/badges", badge("fem boy"), Some(&owner))
        .await;
    assert_eq!(
        code,
        StatusCode::CONFLICT,
        "names are unique, whatever the case"
    );
    assert!(body["error"].as_str().unwrap().contains("already"));
}

#[tokio::test]
async fn a_hidden_badge_is_seen_only_by_whoever_holds_it() {
    let h = Harness::new().await;
    let (owner, member) = two_accounts(&h).await;
    let member_id = id_of(&h, "zjef").await;
    let first = make_badge(&h, &owner, "Albert Heijn").await;
    let second = make_badge(&h, &owner, "Nami lover").await;
    for id in [first, second] {
        let (code, _) = h
            .put(
                &format!("/api/admin/badges/{id}/holders/{member_id}"),
                json!({}),
                Some(&owner),
            )
            .await;
        assert_eq!(code, StatusCode::OK);
    }
    h.put(
        "/api/account/profile",
        json!({ "public": true }),
        Some(&member),
    )
    .await;

    // Shown by default, in the order given.
    let (_, profile) = h.get("/api/profiles/zjef", Some(&owner)).await;
    let names: Vec<&str> = profile["badges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Albert Heijn", "Nami lover"]);
    let small = profile["badges"][0]["small"].as_str().unwrap().to_string();
    let (code, _, _, bytes) = h.send("GET", &small, None, Some(&owner)).await;
    assert_eq!(code, StatusCode::OK);
    assert!(bytes.starts_with(b"\x89PNG"));

    // Hide the first and put the second ahead.
    let (code, mine) = h
        .put(
            "/api/account/badges",
            json!({ "badges": [{ "id": second, "shown": true }, { "id": first, "shown": false }] }),
            Some(&member),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{mine:?}");
    assert_eq!(mine["badges"][0]["name"], "Nami lover");
    assert_eq!(mine["badges"][1]["shown"], false);

    let (_, profile) = h.get("/api/profiles/zjef", Some(&owner)).await;
    assert_eq!(profile["badges"].as_array().unwrap().len(), 1);
    assert_eq!(profile["badges"][0]["name"], "Nami lover");
    let (_, people) = h.get("/api/profiles", Some(&owner)).await;
    let card = people["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["username"] == "zjef")
        .unwrap()
        .clone();
    assert_eq!(card["badges"].as_array().unwrap().len(), 1);

    // The hidden one's picture is there for its holder and nobody else.
    let hidden = mine["badges"][1]["large"].as_str().unwrap().to_string();
    let (code, _, _, _) = h.send("GET", &hidden, None, Some(&owner)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _, _, _) = h.send("GET", &hidden, None, Some(&member)).await;
    assert_eq!(code, StatusCode::OK);

    // Arranging cannot reach a badge you do not hold.
    let (owner_badges,) = (h.get("/api/account/badges", Some(&owner)).await.1,);
    assert_eq!(owner_badges["badges"].as_array().unwrap().len(), 0);
    h.put(
        "/api/account/badges",
        json!({ "badges": [{ "id": first, "shown": true }] }),
        Some(&owner),
    )
    .await;
    let (_, mine) = h.get("/api/account/badges", Some(&member)).await;
    assert_eq!(mine["badges"][1]["shown"], false);

    // A private profile hides its badges' pictures along with it.
    h.put(
        "/api/account/profile",
        json!({ "public": false }),
        Some(&member),
    )
    .await;
    let shown = mine["badges"][0]["small"].as_str().unwrap().to_string();
    let (code, _, _, _) = h.send("GET", &shown, None, Some(&owner)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_badge_or_an_account_takes_its_awards_along() {
    let h = Harness::new().await;
    let (owner, member) = two_accounts(&h).await;
    let member_id = id_of(&h, "zjef").await;
    let owner_id = id_of(&h, "mihai").await;
    let id = make_badge(&h, &owner, "FEM BOY").await;
    for user in [member_id, owner_id] {
        h.put(
            &format!("/api/admin/badges/{id}/holders/{user}"),
            json!({}),
            Some(&owner),
        )
        .await;
    }
    // Giving it twice changes nothing.
    h.put(
        &format!("/api/admin/badges/{id}/holders/{member_id}"),
        json!({}),
        Some(&owner),
    )
    .await;
    assert_eq!(h.count("SELECT COUNT(*) FROM badge_award").await, 2);

    // The export lists it by name.
    let (_, _, _, export) = h
        .send("GET", "/api/export/library.json", None, Some(&member))
        .await;
    let export: Value = serde_json::from_slice(&export).unwrap();
    assert_eq!(export["badges"][0]["name"], "FEM BOY");
    assert!(export["badges"][0]["awarded_at"].is_string());

    // Removing the badge removes every award; undo puts both back.
    let (code, removed) = h
        .delete(&format!("/api/admin/badges/{id}"), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(h.count("SELECT COUNT(*) FROM badge").await, 0);
    assert_eq!(h.count("SELECT COUNT(*) FROM badge_award").await, 0);
    let (code, body) = h
        .post("/api/admin/badges/restore", removed, Some(&owner))
        .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    assert_eq!(h.count("SELECT COUNT(*) FROM badge_award").await, 2);
    let (_, mine) = h.get("/api/account/badges", Some(&member)).await;
    assert_eq!(mine["badges"][0]["name"], "FEM BOY");

    // Taking it back from one person leaves the other.
    let id = body["id"].as_i64().unwrap();
    h.delete(
        &format!("/api/admin/badges/{id}/holders/{owner_id}"),
        Some(&owner),
    )
    .await;
    assert_eq!(h.count("SELECT COUNT(*) FROM badge_award").await, 1);

    // An account deleted takes its awards; the badge stays.
    let (code, _) = h
        .delete(&format!("/api/admin/users/{member_id}"), Some(&owner))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(h.count("SELECT COUNT(*) FROM badge_award").await, 0);
    assert_eq!(h.count("SELECT COUNT(*) FROM badge").await, 1);
}

#[tokio::test]
async fn editing_a_badge_keeps_its_pictures_unless_new_ones_come() {
    let h = Harness::new().await;
    let (owner, _) = two_accounts(&h).await;
    let id = make_badge(&h, &owner, "Albert Heijn").await;
    let (code, body) = h
        .put(
            &format!("/api/admin/badges/{id}"),
            json!({ "name": "AH", "description": "" }),
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    let (_, body) = h
        .get(&format!("/api/admin/badges/{id}"), Some(&owner))
        .await;
    assert_eq!(body["badge"]["name"], "AH");
    assert_eq!(body["badge"]["description"], Value::Null);
    assert_eq!(body["settings"]["tolerance"], 30);
    let (code, _, _, bytes) = h
        .send(
            "GET",
            &format!("/api/admin/badges/{id}/original"),
            None,
            Some(&owner),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert!(bytes.starts_with(b"\x89PNG"));
}

#[tokio::test]
async fn a_profile_wears_only_colours_from_the_list() {
    let h = Harness::new().await;
    let (owner, member) = two_accounts(&h).await;
    h.put(
        "/api/account/profile",
        json!({ "public": true }),
        Some(&member),
    )
    .await;

    let (code, body) = h
        .put(
            "/api/account/profile",
            json!({ "profile_color": "teal", "profile_shade": "dark" }),
            Some(&member),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body:?}");
    assert_eq!(body["profile_color"], "teal");
    assert_eq!(body["profile_shade"], "dark");

    // Everyone who opens the profile sees it.
    let (_, profile) = h.get("/api/profiles/zjef", Some(&owner)).await;
    assert_eq!(profile["profile_color"], "teal");
    assert_eq!(profile["profile_shade"], "dark");

    for bad in [
        json!({ "profile_color": "#00ffaa" }),
        json!({ "profile_color": "fuchsia" }),
        json!({ "profile_color": "teal", "profile_shade": "neon" }),
        json!({ "profile_shade": "rgb(0,0,0)" }),
    ] {
        let (code, body) = h
            .put("/api/account/profile", bad.clone(), Some(&member))
            .await;
        assert_eq!(code, StatusCode::BAD_REQUEST, "{bad}");
        assert!(
            body["error"].as_str().unwrap().contains("colour"),
            "{body:?}"
        );
    }
    let (_, me) = h.get("/api/account", Some(&member)).await;
    assert_eq!(me["profile_color"], "teal", "a refusal changes nothing");

    // A shade alone keeps the colour; an empty colour goes back to the theme's.
    let (_, me) = h
        .put(
            "/api/account/profile",
            json!({ "profile_shade": "light" }),
            Some(&member),
        )
        .await;
    assert_eq!(
        (me["profile_color"].as_str(), me["profile_shade"].as_str()),
        (Some("teal"), Some("light"))
    );
    let (_, me) = h
        .put(
            "/api/account/profile",
            json!({ "profile_color": "" }),
            Some(&member),
        )
        .await;
    assert_eq!(me["profile_color"], Value::Null);
    assert_eq!(me["profile_shade"], Value::Null);
}

#[tokio::test]
async fn a_banner_takes_only_the_layouts_and_heights_offered() {
    let h = Harness::new().await;
    let (owner, member) = two_accounts(&h).await;
    h.put(
        "/api/account/profile",
        json!({ "public": true }),
        Some(&member),
    )
    .await;

    let (code, me) = h
        .put(
            "/api/account/profile",
            json!({ "banner_layout": "vertical", "banner_height": "full", "banner_position": "30% 70%" }),
            Some(&member),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{me:?}");
    assert_eq!(me["banner_layout"], "vertical");
    assert_eq!(me["banner_height"], "full");
    assert_eq!(
        me["banner_position"], "30% 70%",
        "the position stays its own"
    );

    let (_, profile) = h.get("/api/profiles/zjef", Some(&owner)).await;
    assert_eq!(profile["banner_layout"], "vertical");
    assert_eq!(profile["banner_height"], "full");

    for bad in [
        json!({ "banner_layout": "sideways" }),
        json!({ "banner_layout": "Vertical" }),
        json!({ "banner_height": "huge" }),
        json!({ "banner_height": "100vh" }),
    ] {
        let (code, _) = h
            .put("/api/account/profile", bad.clone(), Some(&member))
            .await;
        assert_eq!(code, StatusCode::BAD_REQUEST, "{bad}");
    }

    let (_, me) = h
        .put(
            "/api/account/profile",
            json!({ "banner_layout": "", "banner_height": "" }),
            Some(&member),
        )
        .await;
    assert_eq!(me["banner_layout"], Value::Null);
    assert_eq!(me["banner_height"], Value::Null);
}
