//! An installation from before the AniList overhaul, with data in it, brought
//! up to date the way a running Frog is: nothing is lost, and old episode
//! ratings become season ratings only where that is unambiguous.

use std::borrow::Cow;
use std::str::FromStr;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// The first migration this branch adds; everything before it is what the
/// live instance already ran.
const FIRST_NEW: i64 = 202610291700;

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .0
}

#[tokio::test]
async fn an_older_database_migrates_without_losing_anything() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite:{}?mode=rwc",
        dir.path()
            .join("old.db")
            .to_string_lossy()
            .replace('\\', "/")
    );

    // The schema as it was.
    {
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();
        let all = sqlx::migrate!("./migrations");
        let old = Migrator {
            migrations: Cow::Owned(
                all.migrations
                    .iter()
                    .filter(|m| m.version < FIRST_NEW)
                    .cloned()
                    .collect(),
            ),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        };
        old.run(&pool).await.unwrap();

        for sql in [
            "INSERT INTO app_user (id, username, password_hash, created_at)
             VALUES (1, 'mihai', 'x', '2026-01-01T00:00:00Z'), (2, 'zjef', 'x', '2026-01-01T00:00:00Z')",
            "INSERT INTO title (id, kind, tmdb_id, name) VALUES (1, 'tv', 10, 'Pond Show'), (2, 'tv', 20, 'Anime Show')",
            "INSERT INTO episode (id, title_id, season_number, episode_number, air_date) VALUES
                (1, 1, 1, 1, '2020-01-01'), (2, 1, 1, 2, '2020-01-08'), (3, 1, 1, 3, '2020-01-15'),
                (4, 1, 2, 1, '2021-01-01'), (5, 1, 2, 2, '2021-01-08'),
                (6, 2, 1, 1, '2022-01-01'), (7, 2, 1, 2, '2022-01-08')",
            "INSERT INTO library_entry (user_id, title_id, status, rating, notes, added_at, updated_at) VALUES
                (1, 1, 'watching', 75, 'keep me', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (2, 1, 'planned', NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (1, 2, 'watching', NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, source) VALUES
                (1, 1, 1, '2026-01-02T00:00:00Z', 'manual'), (1, 1, 2, '2026-01-03T00:00:00Z', 'manual'),
                (1, 1, 3, '2026-01-04T00:00:00Z', 'manual'), (2, 1, 1, '2026-01-05T00:00:00Z', 'manual')",
            // Season 1 rated 80 throughout; season 2 rated two ways; the other
            // account rated one episode; and an anime season AniList scores.
            "INSERT INTO episode_state (user_id, episode_id, rating, notes, updated_at) VALUES
                (1, 1, 80, NULL, '2026-01-02T00:00:00Z'), (1, 2, 80, 'great', '2026-01-03T00:00:00Z'),
                (1, 3, 80, NULL, '2026-01-04T00:00:00Z'),
                (1, 4, 70, NULL, '2026-01-05T00:00:00Z'), (1, 5, 90, NULL, '2026-01-06T00:00:00Z'),
                (2, 1, 40, NULL, '2026-01-05T00:00:00Z'),
                (1, 6, 60, NULL, '2026-01-07T00:00:00Z')",
            "INSERT INTO anilist_link (user_id, anilist_user_id, anilist_name, token, expires_at,
                                       connected_at, synced_at)
             VALUES (1, 77, 'pondwatcher', 't', '2099-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                     '2026-01-08T00:00:00Z')",
            "INSERT INTO anilist_entry (user_id, media_id, name, state, title_id, season,
                                        episode_offset, al_status, al_progress, al_score, updated_at)
             VALUES (1, 500, 'Anime Show', 'placed', 2, 1, 0, 'CURRENT', 1, 85, '2026-01-08T00:00:00Z')",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        pool.close().await;
    }

    // Started by a new build: every migration since runs on it.
    let pool = frog::db::connect(&url).await.unwrap();

    assert_eq!(count(&pool, "SELECT COUNT(*) FROM library_entry").await, 3);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM watch_event").await, 4);
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM episode_state WHERE rating IS NOT NULL"
        )
        .await,
        7,
        "every old episode rating is still there"
    );
    let notes: (Option<f64>, Option<String>) = sqlx::query_as(
        "SELECT rating, notes FROM library_entry WHERE user_id = 1 AND title_id = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(notes, (Some(75.0), Some("keep me".into())));

    let seasons: Vec<(i64, i64, i64, f64)> = sqlx::query_as(
        "SELECT user_id, title_id, season_number, rating FROM season_rating
          ORDER BY user_id, title_id, season_number",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        seasons,
        vec![
            // Unanimous: carried over.
            (1, 1, 1, 80.0),
            // AniList's score on the season its entry covers.
            (1, 2, 1, 85.0),
            (2, 1, 1, 40.0),
        ],
        "season 2 disagreed and stays as episode ratings; the anime season \
         keeps AniList's score rather than an episode's"
    );

    // The new state starts empty rather than made up.
    let link: (Option<String>, i64, Option<String>, bool) = sqlx::query_as(
        "SELECT synced_at, failures, retry_at, needs_reconnect FROM anilist_link WHERE user_id = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(link, (Some("2026-01-08T00:00:00Z".into()), 0, None, false));
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM title WHERE anilist_id IS NOT NULL"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM library_entry WHERE progress IS NOT NULL"
        )
        .await,
        0
    );

    // Starting again changes nothing.
    pool.close().await;
    let pool = frog::db::connect(&url).await.unwrap();
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM season_rating").await, 3);
}
