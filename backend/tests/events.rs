//! Events: a pack becomes Frog's, Home shows it to whoever has not hidden it,
//! the premiere is marked once, an administrator's Off hides it from
//! everyone, and its two badges mean what they say. The moment it opens is
//! pinned relative to now in each test, so none of them depends on the day
//! they run.

use chrono::{DateTime, Duration, Utc};
use frog::clock::Stamp;
use std::collections::HashSet;

use frog::awards;
use frog::config::Config;
use frog::events::{pack, records, sync, view};
use frog::state::AppState;
use frog::store::ItemFacts;

mod common;

const SLUG: &str = "avengers-doomsday";

struct Harness {
    state: AppState,
    _dir: tempfile::TempDir,
    event: i64,
    target: i64,
    /// Every title the pack names, with its episodes when it is a show.
    titles: Vec<(i64, bool, Vec<i64>)>,
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

        // What the pack names, written as TMDB would have given it.
        let p = pack::get(SLUG).expect("the Doomsday pack");
        let target = seed(&state, &p.target.kind, p.target.tmdb, "Avengers: Doomsday").await;
        let mut titles = Vec::new();
        for (_, e) in p.entries() {
            let id = seed(&state, &e.kind, e.tmdb, &format!("Entry {}", e.tmdb)).await;
            let mut eps = Vec::new();
            if e.kind == "tv" {
                let [from, to] = e.seasons.unwrap_or([1, 1]);
                for season in from..=to {
                    for n in 1..=3 {
                        let (ep,): (i64,) = sqlx::query_as(
                            "INSERT INTO episode (title_id, season_number, episode_number,
                                                  air_date, runtime)
                             VALUES (?, ?, ?, '2021-01-01', 45) RETURNING id",
                        )
                        .bind(id)
                        .bind(season)
                        .bind(n)
                        .fetch_one(&state.pool)
                        .await
                        .unwrap();
                        eps.push(ep);
                    }
                }
            }
            titles.push((id, e.essential, eps));
        }
        sync::fill(&state, p).await.expect("fill");
        let row = records::by_slug(&state.pool, SLUG)
            .await
            .unwrap()
            .expect("the event's row");
        Self {
            state,
            _dir: dir,
            event: row.id,
            target,
            titles,
        }
    }

    async fn user(&self, name: &str) -> i64 {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO app_user (username, password_hash, role, created_at, profile_public)
             VALUES (?, 'x', 'user', ?, 1) RETURNING id",
        )
        .bind(name)
        .bind(Utc::now().stamp())
        .fetch_one(&self.state.pool)
        .await
        .unwrap();
        frog::announce::set_time_zone(&self.state.pool, id, "UTC")
            .await
            .unwrap();
        id
    }

    async fn opens(&self, at: DateTime<Utc>) {
        records::set_pinned(&self.state.pool, self.event, Some(at))
            .await
            .unwrap();
    }

    /// A watch of a film or an episode, recorded `how` as it was watched.
    async fn watch(
        &self,
        user: i64,
        title: i64,
        episode: Option<i64>,
        at: DateTime<Utc>,
        how: &str,
    ) {
        sqlx::query(
            "INSERT OR IGNORE INTO library_entry (user_id, title_id, status, added_at, updated_at)
             VALUES (?, ?, 'completed', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(user)
        .bind(title)
        .execute(&self.state.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, source,
                                      logged_at, logged_how)
             VALUES (?, ?, ?, ?, 'manual', ?, ?)",
        )
        .bind(user)
        .bind(title)
        .bind(episode)
        .bind(at.stamp())
        .bind(at.stamp())
        .bind(how)
        .execute(&self.state.pool)
        .await
        .unwrap();
    }

    /// Every essential, the last one at `by`, the ones before at a pace
    /// someone could watch.
    async fn every_essential(&self, user: i64, by: DateTime<Utc>) {
        let mut at = by;
        for (id, essential, eps) in self.titles.iter().rev() {
            if !essential {
                continue;
            }
            if eps.is_empty() {
                self.watch(user, *id, None, at, "tick").await;
                at -= Duration::hours(3);
            }
            for ep in eps.iter().rev() {
                self.watch(user, *id, Some(*ep), at, "tick").await;
                at -= Duration::hours(1);
            }
            at -= Duration::hours(20);
        }
    }

    async fn home(&self, user: i64) -> Vec<serde_json::Value> {
        let facts = ItemFacts::load(&self.state.pool, user).await.unwrap();
        view::for_home(&self.state.pool, user, &facts)
            .await
            .unwrap()
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

    async fn notices(&self, user: i64) -> Vec<String> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT payload FROM notification WHERE user_id = ? AND kind = 'event'")
                .bind(user)
                .fetch_all(&self.state.pool)
                .await
                .unwrap();
        rows.into_iter().map(|(p,)| p).collect()
    }
}

async fn seed(state: &AppState, kind: &str, tmdb: i64, name: &str) -> i64 {
    if let Some(id) = common::existing_id(state, tmdb).await {
        return id;
    }
    sqlx::query(
        "INSERT INTO title_seed (kind, tmdb_id, name, runtime, release_date)
         VALUES (?, ?, ?, 120, '2020-01-01')",
    )
    .bind(kind)
    .bind(tmdb)
    .bind(name)
    .execute(&state.pool)
    .await
    .unwrap();
    let (id,): (i64,) = sqlx::query_as("SELECT MAX(id) FROM title")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn home_counts_essentials_in_watch_order() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    h.opens(Utc::now() + Duration::days(10)).await;
    let first = h.titles.iter().find(|t| t.1 && t.2.is_empty()).unwrap().0;
    h.watch(me, first, None, Utc::now() - Duration::days(1), "tick")
        .await;

    let home = h.home(me).await;
    assert_eq!(home.len(), 1, "one event on Home");
    let e = &home[0];
    assert_eq!(e["phase"], "live");
    let essentials = h.titles.iter().filter(|t| t.1).count() as i64;
    assert_eq!(e["readiness"]["essentials"], essentials);
    assert_eq!(e["readiness"]["essentials_seen"], 1);
    assert!(e["readiness"]["per_day"].as_i64().unwrap() > 0);
    assert_eq!(e["before"].as_array().unwrap().len(), h.titles.len());
    assert_eq!(
        e["before"][0]["item"]["id"], h.titles[0].0,
        "in the pack's order"
    );
}

#[tokio::test]
async fn hiding_takes_it_off_home_and_quiets_its_reminders() {
    let h = Harness::new().await;
    let shown = h.user("ana").await;
    let hider = h.user("ben").await;
    // A week to go, an hour ago: the week's reminder is due.
    h.opens(Utc::now() + Duration::days(7) - Duration::hours(1))
        .await;
    records::set_hidden(&h.state.pool, hider, h.event, true)
        .await
        .unwrap();

    assert!(h.home(hider).await.is_empty());
    let list = view::list(&h.state.pool, hider, false).await.unwrap();
    let listed = &list["events"][0];
    assert_eq!(listed["hidden"], true, "still on Events, marked hidden");

    sync::tell(&h.state, Utc::now()).await.unwrap();
    sync::tell(&h.state, Utc::now()).await.unwrap();
    let told = h.notices(shown).await;
    assert_eq!(told.len(), 1, "once, however often the worker runs");
    assert!(told[0].contains("A week to Doomsday"), "{told:?}");
    assert!(h.notices(hider).await.is_empty());

    // Shown again, Home has it back.
    records::set_hidden(&h.state.pool, hider, h.event, false)
        .await
        .unwrap();
    assert_eq!(h.home(hider).await.len(), 1);
}

#[tokio::test]
async fn the_premiere_is_marked_seen_once() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    assert!(records::mark_seen(&h.state.pool, me, h.event)
        .await
        .unwrap());
    assert!(!records::mark_seen(&h.state.pool, me, h.event)
        .await
        .unwrap());
    h.opens(Utc::now() - Duration::hours(1)).await;
    assert_eq!(h.home(me).await[0]["seen"], true);
}

#[tokio::test]
async fn off_hides_it_from_everyone_but_administrators() {
    let h = Harness::new().await;
    let me = h.user("ana").await;
    h.opens(Utc::now() + Duration::days(3)).await;
    records::set_state(&h.state.pool, h.event, false)
        .await
        .unwrap();
    assert!(h.home(me).await.is_empty());
    let mine = view::list(&h.state.pool, me, false).await.unwrap();
    assert!(mine["events"].as_array().unwrap().is_empty());
    let admin = view::list(&h.state.pool, me, true).await.unwrap();
    assert_eq!(admin["events"][0]["on"], false);
}

#[tokio::test]
async fn ready_is_given_at_zero_and_opening_night_only_for_a_watch_logged_then() {
    let h = Harness::new().await;
    let zero = Utc::now() - Duration::days(2);
    h.opens(zero).await;

    // Every essential before it opened.
    let ready = h.user("ana").await;
    h.every_essential(ready, zero - Duration::hours(2)).await;
    // The film itself, the evening it opened, ticked as it ended.
    let opening = h.user("ben").await;
    h.watch(opening, h.target, None, zero + Duration::hours(3), "tick")
        .await;
    // The same evening, but marked with many others at once.
    let bulk = h.user("cy").await;
    h.watch(bulk, h.target, None, zero + Duration::hours(3), "bulk")
        .await;
    // Every essential, but the last after it opened.
    let late = h.user("dee").await;
    h.every_essential(late, zero + Duration::hours(5)).await;

    for u in [ready, opening, bulk, late] {
        awards::run(&h.state, u, true).await.expect("run");
    }
    let ready_key = pack::ready_key(SLUG);
    let opening_key = pack::opening_key(SLUG);
    assert!(h.held(ready).await.contains(&ready_key));
    assert!(
        !h.held(late).await.contains(&ready_key),
        "seen in time, each one"
    );
    assert!(h.held(opening).await.contains(&opening_key));
    assert!(
        !h.held(bulk).await.contains(&opening_key),
        "a bulk mark is no evening out"
    );
}
