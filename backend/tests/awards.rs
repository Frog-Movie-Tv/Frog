//! Badges Frog gives on its own: that a badge means what it says. Watches
//! here are written the way each part of Frog writes them, with the moment
//! they were recorded set by hand where a test needs a day that is over.

use chrono::{Duration, NaiveDate, Utc};
use std::collections::HashSet;

use frog::awards;
use frog::config::Config;
use frog::state::AppState;

mod common;

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
            extension_releases: "http://127.0.0.1:9".into(),
            mdblist: None,
        };
        let pool = frog::db::connect(&url).await.expect("database");
        let state = AppState::new(pool, config);
        common::add_title_seed(&state).await;
        awards::give::seed(&state.pool).await.expect("seed");
        Self { state, _dir: dir }
    }

    async fn user(&self, name: &str) -> i64 {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO app_user (username, password_hash, role, created_at, profile_public)
             VALUES (?, 'x', 'user', ?, 1) RETURNING id",
        )
        .bind(name)
        .bind(Utc::now().to_rfc3339())
        .fetch_one(&self.state.pool)
        .await
        .unwrap();
        id
    }

    async fn zone(&self, user: i64, zone: &str) {
        frog::announce::set_time_zone(&self.state.pool, user, zone)
            .await
            .unwrap();
    }

    async fn title(&self, kind: &str, name: &str, runtime: i64, genres: &str) -> i64 {
        sqlx::query(
            "INSERT INTO title_seed (kind, tmdb_id, name, runtime, genres_json, release_date)
             VALUES (?, (SELECT COALESCE(MAX(id), 0) + 1000 FROM title), ?, ?, ?, '2001-05-01')",
        )
        .bind(kind)
        .bind(name)
        .bind(runtime)
        .bind(genres)
        .execute(&self.state.pool)
        .await
        .unwrap();
        let (id,): (i64,) = sqlx::query_as("SELECT MAX(id) FROM title")
            .fetch_one(&self.state.pool)
            .await
            .unwrap();
        id
    }

    async fn keep(&self, user: i64, title: i64, status: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO library_entry (user_id, title_id, status, added_at, updated_at)
             VALUES (?, ?, ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(user)
        .bind(title)
        .bind(status)
        .execute(&self.state.pool)
        .await
        .unwrap();
    }

    async fn film(&self, user: i64, name: &str) -> i64 {
        let id = self.title("movie", name, 110, "[\"Drama\"]").await;
        self.keep(user, id, "completed").await;
        id
    }

    /// A show of one season, every episode out, `runtime` minutes each.
    async fn show(&self, user: i64, name: &str, episodes: i64) -> (i64, Vec<i64>) {
        let id = self.title("tv", name, 24, "[\"Drama\"]").await;
        self.keep(user, id, "watching").await;
        let mut eps = Vec::new();
        for n in 1..=episodes {
            let (e,): (i64,) = sqlx::query_as(
                "INSERT INTO episode (title_id, season_number, episode_number, air_date, runtime)
                 VALUES (?, 1, ?, '2020-01-01', 24) RETURNING id",
            )
            .bind(id)
            .bind(n)
            .fetch_one(&self.state.pool)
            .await
            .unwrap();
            eps.push(e);
        }
        (id, eps)
    }

    /// A watch recorded `how`, at `logged` (rfc3339), of something watched at
    /// `watched`.
    async fn watch(
        &self,
        user: i64,
        title: i64,
        episode: Option<i64>,
        watched: &str,
        logged: &str,
        how: &str,
    ) -> i64 {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, source,
                                      logged_at, logged_how)
             VALUES (?, ?, ?, ?, 'manual', ?, ?) RETURNING id",
        )
        .bind(user)
        .bind(title)
        .bind(episode)
        .bind(watched)
        .bind(logged)
        .bind(how)
        .fetch_one(&self.state.pool)
        .await
        .unwrap();
        id
    }

    async fn run(&self, user: i64) -> awards::give::Settled {
        awards::run(&self.state, user, true).await.expect("run")
    }

    async fn held(&self, user: i64) -> HashSet<String> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT b.rule FROM badge_award a JOIN badge b ON b.id = a.badge_id
              WHERE a.user_id = ? AND b.rule IS NOT NULL",
        )
        .bind(user)
        .fetch_all(&self.state.pool)
        .await
        .unwrap();
        rows.into_iter().map(|(r,)| r).collect()
    }

    async fn badge_notices(&self, user: i64) -> i64 {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM notification WHERE user_id = ? AND kind = 'badge'",
        )
        .bind(user)
        .fetch_one(&self.state.pool)
        .await
        .unwrap();
        n
    }
}

/// `hh:mm` on the day before today, UTC.
fn yesterday(hm: &str) -> String {
    let day = Utc::now().date_naive() - Duration::days(1);
    format!("{day}T{hm}:00Z")
}

fn day_at(day: NaiveDate, hm: &str) -> String {
    format!("{day}T{hm}:00Z")
}

const MOMENTS: [&str; 6] = [
    "double-feature",
    "triple-bill",
    "film-festival",
    "marathon",
    "ultramarathon",
    "one-sitting",
];

#[tokio::test]
async fn old_films_entered_today_count_for_totals_only() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    for i in 0..5 {
        let f = h.film(me, &format!("Old {i}")).await;
        let logged = yesterday(&format!("20:{:02}", i * 2));
        h.watch(
            me,
            f,
            None,
            &format!("2019-03-1{i}T20:00:00Z"),
            &logged,
            "tick",
        )
        .await;
    }
    h.run(me).await;
    let held = h.held(me).await;
    assert!(held.contains("films.1"), "{held:?}");
    for m in MOMENTS {
        assert!(!held.contains(m), "{m} from films watched years ago");
    }
}

#[tokio::test]
async fn three_films_at_a_real_pace_are_a_triple_bill_once_the_day_is_over() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    for (i, at) in ["13:00", "15:10", "17:20"].iter().enumerate() {
        let f = h.film(me, &format!("Film {i}")).await;
        h.watch(me, f, None, &yesterday(at), &yesterday(at), "tick")
            .await;
    }
    h.run(me).await;
    let held = h.held(me).await;
    assert!(
        held.contains("triple-bill") && held.contains("double-feature"),
        "{held:?}"
    );

    // The same evening today is not over yet.
    let you = h.user("ben").await;
    let now = Utc::now();
    for i in 0..3 {
        let f = h.film(you, &format!("Today {i}")).await;
        let at = (now - Duration::minutes(10) - Duration::hours(2 * (2 - i))).to_rfc3339();
        h.watch(you, f, None, &at, &at, "tick").await;
    }
    h.run(you).await;
    assert!(!h.held(you).await.contains("triple-bill"));
}

#[tokio::test]
async fn five_films_ticked_within_two_minutes_are_no_triple_bill() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    for i in 0..5 {
        let f = h.film(me, &format!("Quick {i}")).await;
        let at = yesterday(&format!("20:0{}", i / 2));
        h.watch(me, f, None, &at, &at, "tick").await;
    }
    h.run(me).await;
    let held = h.held(me).await;
    assert!(held.contains("films.1"));
    assert!(!held.contains("double-feature") && !held.contains("triple-bill"));
}

#[tokio::test]
async fn twelve_episodes_at_a_watching_pace_are_a_marathon_and_in_minutes_are_not() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    let (show, eps) = h.show(me, "Long Night", 12).await;
    let start = Utc::now().date_naive() - Duration::days(1);
    for (i, e) in eps.iter().enumerate() {
        let at = (start.and_hms_opt(9, 0, 0).unwrap().and_utc() + Duration::minutes(25 * i as i64))
            .to_rfc3339();
        h.watch(me, show, Some(*e), &at, &at, "tick").await;
    }
    h.run(me).await;
    let held = h.held(me).await;
    assert!(
        held.contains("marathon") && held.contains("ultramarathon"),
        "{held:?}"
    );
    assert!(
        held.contains("one-sitting"),
        "the whole season in a day: {held:?}"
    );

    let you = h.user("ben").await;
    let (show, eps) = h.show(you, "Short Night", 12).await;
    for (i, e) in eps.iter().enumerate() {
        let at = (start.and_hms_opt(9, 0, 0).unwrap().and_utc() + Duration::seconds(25 * i as i64))
            .to_rfc3339();
        h.watch(you, show, Some(*e), &at, &at, "tick").await;
    }
    h.run(you).await;
    let held = h.held(you).await;
    assert!(held.contains("episodes.1"));
    assert!(!held.contains("marathon"), "{held:?}");
}

#[tokio::test]
async fn a_library_update_the_same_evening_takes_the_day_with_it() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    for (i, at) in ["13:00", "15:10"].iter().enumerate() {
        let f = h.film(me, &format!("Real {i}")).await;
        h.watch(me, f, None, &yesterday(at), &yesterday(at), "tick")
            .await;
    }
    for i in 0..30 {
        let f = h.film(me, &format!("Added {i}")).await;
        let at = yesterday(&format!("21:{:02}", i));
        h.watch(me, f, None, &at, &at, "tick").await;
    }
    h.run(me).await;
    let held = h.held(me).await;
    assert!(!held.contains("double-feature"), "{held:?}");
    assert!(
        held.contains("films.10"),
        "all 32 count toward totals: {held:?}"
    );
}

#[tokio::test]
async fn a_big_import_gives_totals_and_one_notification() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    for i in 0..600 {
        let f = h.film(me, &format!("Imported {i}")).await;
        // As an import writes it: the trigger says how it came.
        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, source)
             VALUES (?, ?, NULL, ?, 'import')",
        )
        .bind(me)
        .bind(f)
        .bind(format!("2020-01-{:02}T20:00:00Z", 1 + i % 28))
        .execute(&h.state.pool)
        .await
        .unwrap();
    }
    let settled = h.run(me).await;
    let held = h.held(me).await;
    for step in [
        "films.1",
        "films.10",
        "films.50",
        "films.100",
        "films.250",
        "films.500",
        "library.500",
    ] {
        assert!(held.contains(step), "{step}: {held:?}");
    }
    assert!(settled.given.len() > 5);
    assert_eq!(h.badge_notices(me).await, 1);
    for m in MOMENTS {
        assert!(!held.contains(m));
    }
    let (how,): (String,) =
        sqlx::query_as("SELECT DISTINCT logged_how FROM watch_event WHERE user_id = ?")
            .bind(me)
            .fetch_one(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(how, "import");
}

#[tokio::test]
async fn the_trigger_tells_a_first_anilist_sync_from_a_later_one() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    let (show, eps) = h.show(me, "Frieren", 3).await;
    for (e, undated) in [(eps[0], true), (eps[1], false)] {
        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, source, undated)
             VALUES (?, ?, ?, ?, 'anilist', ?)",
        )
        .bind(me)
        .bind(show)
        .bind(e)
        .bind(Utc::now().to_rfc3339())
        .bind(undated)
        .execute(&h.state.pool)
        .await
        .unwrap();
    }
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT logged_how, logged_at FROM watch_event WHERE user_id = ? ORDER BY id",
    )
    .bind(me)
    .fetch_all(&h.state.pool)
    .await
    .unwrap();
    assert_eq!(rows[0].0, "anilist_first");
    assert_eq!(rows[1].0, "anilist");
    assert!(rows.iter().all(|r| r.1.is_some()));
}

#[tokio::test]
async fn mark_all_watched_is_catching_up() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    let (show, _) = h.show(me, "Fifty", 50).await;
    let marking: frog::bulk::Marking = serde_json::from_str(r#"{"when":"today"}"#).unwrap();
    frog::bulk::tick_aired(&h.state.pool, me, show, None, &marking, &[])
        .await
        .unwrap();
    let hows: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT logged_how FROM watch_event WHERE user_id = ?")
            .bind(me)
            .fetch_all(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(hows, vec![("bulk".to_string(),)]);
    h.run(me).await;
    let held = h.held(me).await;
    assert!(held.contains("episodes.1"));
    assert!(!held.contains("marathon") && !held.contains("one-sitting"));
}

#[tokio::test]
async fn a_badge_goes_in_its_first_day_and_stays_after() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    let mut films = Vec::new();
    for (i, at) in ["13:00", "15:10"].iter().enumerate() {
        let f = h.film(me, &format!("Pair {i}")).await;
        films.push(
            h.watch(me, f, None, &yesterday(at), &yesterday(at), "tick")
                .await,
        );
    }
    h.run(me).await;
    assert!(h.held(me).await.contains("double-feature"));
    assert_eq!(h.badge_notices(me).await, 1);

    // Removed inside the day: nothing else earns it, so it goes, and so does
    // the notification once nothing it announced is left.
    sqlx::query("DELETE FROM watch_event WHERE user_id = ?")
        .bind(me)
        .execute(&h.state.pool)
        .await
        .unwrap();
    let settled = h.run(me).await;
    assert!(settled.taken.contains(&"double-feature".to_string()));
    assert!(!h.held(me).await.contains("double-feature"));
    assert!(!h.held(me).await.contains("films.1"));
    assert_eq!(h.badge_notices(me).await, 0);

    // Earned again, then a day passes: now it stays whatever happens.
    let you = h.user("ben").await;
    for (i, at) in ["13:00", "15:10"].iter().enumerate() {
        let f = h.film(you, &format!("Keep {i}")).await;
        h.watch(you, f, None, &yesterday(at), &yesterday(at), "tick")
            .await;
    }
    h.run(you).await;
    sqlx::query("UPDATE badge_award SET awarded_at = ? WHERE user_id = ?")
        .bind((Utc::now() - Duration::hours(30)).to_rfc3339())
        .bind(you)
        .execute(&h.state.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM watch_event WHERE user_id = ?")
        .bind(you)
        .execute(&h.state.pool)
        .await
        .unwrap();
    h.run(you).await;
    assert!(h.held(you).await.contains("double-feature"));
}

#[tokio::test]
async fn remove_then_undo_keeps_the_badge_and_says_nothing_more() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    let mut titles = Vec::new();
    // Recorded an hour ago and the hour before, on yesterday's watches: undo
    // carries what was recorded this recently.
    let start = Utc::now() - Duration::hours(3);
    for i in 0..2 {
        let f = h.film(me, &format!("Undo {i}")).await;
        titles.push(f);
        let at = (start - Duration::hours(2 * i)).to_rfc3339();
        h.watch(me, f, None, &at, &at, "tick").await;
    }
    let before = frog::bulk::remove(&h.state.pool, me, &titles)
        .await
        .unwrap();
    frog::store::restore_entries(&h.state.pool, me, &before, false)
        .await
        .unwrap();
    let hows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT logged_how, logged_at FROM watch_event WHERE user_id = ?")
            .bind(me)
            .fetch_all(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(hows.len(), 2);
    assert!(
        hows.iter().all(|(how, at)| how == "tick" && at.is_some()),
        "{hows:?}"
    );
}

#[tokio::test]
async fn a_streak_is_counted_on_the_accounts_own_days() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    h.zone(me, "Pacific/Kiritimati").await;
    let (show, eps) = h.show(me, "Late", 3).await;
    let x = Utc::now().date_naive() - Duration::days(4);
    // 23:00, then 01:00 and 01:00 a day later, in a zone fourteen hours ahead:
    // three days there, two in UTC.
    let times = [
        day_at(x, "09:00"),
        day_at(x, "11:00"),
        day_at(x + Duration::days(1), "11:00"),
    ];
    for (e, at) in eps.iter().zip(&times) {
        h.watch(me, show, Some(*e), at, at, "tick").await;
    }
    h.run(me).await;
    assert!(h.held(me).await.contains("streak.3"));

    let you = h.user("ben").await;
    let (show, eps) = h.show(you, "Late Too", 3).await;
    for (e, at) in eps.iter().zip(&times) {
        h.watch(you, show, Some(*e), at, at, "tick").await;
    }
    h.run(you).await;
    assert!(!h.held(you).await.contains("streak.3"));
}

#[tokio::test]
async fn taste_twin_never_reads_a_profile_that_is_off() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    let them = h.user("ben").await;
    sqlx::query("UPDATE app_user SET profile_public = 0 WHERE id = ?")
        .bind(them)
        .execute(&h.state.pool)
        .await
        .unwrap();
    for i in 0..6 {
        let f = h.title("movie", &format!("Shared {i}"), 100, "[]").await;
        for (who, rating) in [(me, 80.0), (them, 82.0)] {
            sqlx::query(
                "INSERT INTO library_entry (user_id, title_id, status, rating, added_at, updated_at)
                 VALUES (?, ?, 'completed', ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            )
            .bind(who)
            .bind(f)
            .bind(rating)
            .execute(&h.state.pool)
            .await
            .unwrap();
        }
    }
    h.run(me).await;
    assert!(!h.held(me).await.contains("taste-twin"));
    sqlx::query("UPDATE app_user SET profile_public = 1 WHERE id = ?")
        .bind(them)
        .execute(&h.state.pool)
        .await
        .unwrap();
    h.run(me).await;
    assert!(h.held(me).await.contains("taste-twin"));
}

#[tokio::test]
async fn a_hand_made_badge_keeps_its_name_and_frogs_waits() {
    let h = Harness::new().await;
    // Made by hand before this release: take the automatic one out first.
    sqlx::query("DELETE FROM badge WHERE rule = 'streak.7'")
        .execute(&h.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO badge (name, description, large, small, original, original_mime,
                            settings_json, created_at, updated_at)
         VALUES ('Habit', 'Ours', x'01', x'02', x'03', 'image/png', '{}', 'then', 'then')",
    )
    .execute(&h.state.pool)
    .await
    .unwrap();
    awards::give::seed(&h.state.pool).await.unwrap();
    let (desc, rule): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT description, rule FROM badge WHERE name = 'Habit'")
            .fetch_one(&h.state.pool)
            .await
            .unwrap();
    assert_eq!(desc.as_deref(), Some("Ours"));
    assert_eq!(rule, None);
    let waiting = awards::give::waiting(&h.state.pool).await.unwrap();
    assert_eq!(
        waiting.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
        vec!["streak.7"]
    );
}

#[tokio::test]
async fn an_automatic_badge_is_drawn_and_cannot_be_deleted() {
    let h = Harness::new().await;
    let (id,): (i64,) = sqlx::query_as("SELECT id FROM badge WHERE rule = 'films.1'")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    let (png, _) = frog::badges::picture(&h.state.pool, id, frog::badges::Size::Small)
        .await
        .unwrap()
        .expect("drawn");
    assert_eq!(frog::badges::png_size(&png), Some((128, 128)));
    assert!(frog::badges::remove(&h.state.pool, id).await.is_err());
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM badge WHERE rule IS NOT NULL")
        .fetch_one(&h.state.pool)
        .await
        .unwrap();
    assert_eq!(n as usize, awards::catalogue::all().len());
}

#[tokio::test]
async fn a_badge_is_given_once_however_far_past_it_a_library_goes() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    // Fifteen genres in all, met by the eighth film; that film and the two
    // after it go on naming genres already counted, which must not meet
    // Genre Hopper again.
    for i in 0..10 {
        let (a, b) = match i {
            0..=6 => (i * 2, i * 2 + 1),
            7 => (14, 0),
            _ => (0, 1),
        };
        let genres = format!("[\"Genre {a}\", \"Genre {b}\"]");
        let f = h.title("movie", &format!("Film {i}"), 100, &genres).await;
        h.keep(me, f, "completed").await;
        h.watch(
            me,
            f,
            None,
            &format!("2020-01-1{i}T20:00:00Z"),
            &yesterday("20:00"),
            "import",
        )
        .await;
    }
    let settled = h.run(me).await;
    let hops = settled
        .given
        .iter()
        .filter(|k| k.as_str() == "genre-hopper")
        .count();
    assert_eq!(hops, 1, "{:?}", settled.given);
    let mut keys = settled.given.clone();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), settled.given.len(), "{:?}", settled.given);
}
