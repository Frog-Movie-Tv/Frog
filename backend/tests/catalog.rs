//! Frog's own catalog: a database from before titles had ids of their own,
//! brought up to date, and what the identity model then guarantees — one
//! title per provider id, one primary provider per title, merges that keep
//! resolving, and nobody's history touched.

mod common;

use std::borrow::Cow;
use std::str::FromStr;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use frog::catalog::{self, ExternalId};
use frog::config::Config;
use frog::state::AppState;

/// The migration that gives titles provider rows; everything before it is a
/// database as it was.
const CATALOG: i64 = 202610301000;

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(sql)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
        .0
}

fn url_in(dir: &tempfile::TempDir) -> String {
    format!(
        "sqlite:{}?mode=rwc",
        dir.path()
            .join("frog.db")
            .to_string_lossy()
            .replace('\\', "/")
    )
}

/// Every personal row, per account, as one comparable list.
async fn personal(pool: &SqlitePool) -> Vec<(String, i64, String)> {
    let mut out = Vec::new();
    for (table, sql) in [
        (
            "library",
            "SELECT user_id, title_id || ':' || status || ':' || COALESCE(rating, '') ||
                    ':' || favourite || ':' || COALESCE(notes, '') FROM library_entry",
        ),
        (
            "watch",
            "SELECT user_id, title_id || ':' || COALESCE(episode_id, '') || ':' || watched_at ||
                    ':' || source FROM watch_event",
        ),
        (
            "season",
            "SELECT user_id, title_id || ':' || season_number || ':' || rating FROM season_rating",
        ),
        (
            "shelf",
            "SELECT s.user_id, i.title_id || ':' || i.position FROM shelf_item i
               JOIN shelf s ON s.id = i.shelf_id",
        ),
        (
            "anilist",
            "SELECT user_id, media_id || ':' || COALESCE(title_id, '') || ':' || state
               FROM anilist_entry",
        ),
    ] {
        let rows: Vec<(i64, String)> = sqlx::query_as(sql).fetch_all(pool).await.unwrap();
        out.extend(rows.into_iter().map(|(u, r)| (table.to_string(), u, r)));
    }
    out.sort();
    out
}

/// A database as the release before this one left it: TMDB titles, an anime
/// TMDB lacks (its AniList id made negative), one folded into a TMDB title,
/// IMDb ids, two accounts with libraries, watches, shelves, season ratings,
/// an AniList placement and profile pins.
async fn old_database(url: &str) {
    let opts = SqliteConnectOptions::from_str(url)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();
    let all = sqlx::migrate!("./migrations");
    let old = Migrator {
        migrations: Cow::Owned(
            all.migrations
                .iter()
                .filter(|m| m.version < CATALOG)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    old.run(&pool).await.unwrap();
    for sql in [
        "INSERT INTO app_user (id, username, password_hash, created_at, pins_json) VALUES
            (1, 'mihai', 'x', '2026-01-01T00:00:00Z',
             '[{\"kind\":\"movie\",\"tmdb_id\":550},{\"kind\":\"tv\",\"tmdb_id\":1399}]'),
            (2, 'zjef', 'x', '2026-01-01T00:00:00Z', '[]')",
        "INSERT INTO title (id, kind, tmdb_id, name, imdb_id, anilist_id, merged_into) VALUES
            (1, 'movie', 550, 'Fight Club', 'tt0137523', NULL, NULL),
            (2, 'tv', 1399, 'Thrones', 'tt0944947', NULL, NULL),
            (3, 'tv', -21, 'Only On AniList', NULL, 21, NULL),
            (4, 'tv', -99, 'Folded In', NULL, 99, 2),
            (9, 'tv', 550, 'A Show That Shares A Film Id', NULL, NULL, NULL)",
        // A title made and taken out again, so the id sequence is ahead of
        // the highest id there is.
        "UPDATE sqlite_sequence SET seq = 12 WHERE name = 'title'",
        "INSERT INTO episode (id, title_id, season_number, episode_number) VALUES
            (1, 2, 1, 1), (2, 2, 1, 2), (3, 3, 1, 1)",
        "INSERT INTO library_entry (user_id, title_id, status, rating, notes, added_at, updated_at)
         VALUES (1, 1, 'completed', 90, 'mine', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (1, 2, 'watching', NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (1, 3, 'watching', NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (2, 2, 'planned', NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, source) VALUES
            (1, 1, NULL, '2026-01-02T00:00:00Z', 'manual'),
            (1, 2, 1, '2026-01-03T00:00:00Z', 'manual'),
            (1, 3, 3, '2026-01-04T00:00:00Z', 'anilist'),
            (2, 2, 2, '2026-01-05T00:00:00Z', 'import')",
        "INSERT INTO season_rating (user_id, title_id, season_number, first_episode, rating, updated_at)
         VALUES (1, 2, 1, 1, 80, '2026-01-03T00:00:00Z')",
        "INSERT INTO shelf (id, user_id, name, kind, position, created_at)
         VALUES (1, 1, 'Mine', 'manual', 0, '2026-01-01T00:00:00Z')",
        "INSERT INTO shelf_item (shelf_id, title_id, position) VALUES (1, 3, 0), (1, 1, 1)",
        "INSERT INTO anilist_entry (user_id, media_id, name, state, title_id, season, updated_at)
         VALUES (1, 300, 'Thrones Anime', 'placed', 2, 1, '2026-01-01T00:00:00Z')",
    ] {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
    }
    pool.close().await;
}

#[tokio::test]
async fn an_older_catalog_gets_ids_of_its_own_and_loses_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let url = url_in(&dir);
    old_database(&url).await;
    let before = {
        let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
        let rows = personal(&pool).await;
        pool.close().await;
        rows
    };
    assert_eq!(before.len(), 12, "{before:?}");

    let pool = frog::db::connect(&url).await.unwrap();

    // Nobody's history moved: same rows, same titles, same ids.
    assert_eq!(personal(&pool).await, before);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM title").await, 5);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM pragma_foreign_key_check").await,
        0
    );
    // The provider columns are gone; only the catalog's own are left.
    for column in ["tmdb_id", "anilist_id", "imdb_id"] {
        assert_eq!(
            count(
                &pool,
                &format!("SELECT COUNT(*) FROM pragma_table_info('title') WHERE name = '{column}'")
            )
            .await,
            0,
            "{column}"
        );
    }

    // Every title is found by what its provider calls it.
    let find = |id: ExternalId| {
        let pool = pool.clone();
        async move { catalog::find(&pool, &id).await.unwrap() }
    };
    assert_eq!(find(ExternalId::tmdb("movie", 550)).await, Some(1));
    assert_eq!(
        find(ExternalId::tmdb("tv", 550)).await,
        Some(9),
        "kinds kept apart"
    );
    assert_eq!(find(ExternalId::tmdb("tv", 1399)).await, Some(2));
    assert_eq!(find(ExternalId::AniList(21)).await, Some(3));
    assert_eq!(find(ExternalId::Imdb("tt0137523".into())).await, Some(1));
    // A folded title's id still answers, with the title it went to.
    assert_eq!(find(ExternalId::AniList(99)).await, Some(2));
    assert_eq!(
        catalog::find_exact(&pool, &ExternalId::AniList(99))
            .await
            .unwrap(),
        Some(4)
    );

    // Exactly one primary provider per title, and it is the right one.
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM title t
              WHERE (SELECT COUNT(*) FROM title_external x
                      WHERE x.title_id = t.id AND x.is_primary = 1) != 1"
        )
        .await,
        0
    );
    assert_eq!(
        catalog::primary(&pool, 3).await.unwrap(),
        Some(ExternalId::AniList(21))
    );
    assert_eq!(
        catalog::primary(&pool, 1).await.unwrap(),
        Some(ExternalId::tmdb("movie", 550))
    );

    // What title 2 answers to, merged title included.
    let ids = catalog::externals(&pool, 2).await.unwrap();
    assert_eq!(ids.primary, Some("tmdb"));
    assert_eq!(ids.anilist, vec![99]);
    assert_eq!(ids.imdb, vec!["tt0944947".to_string()]);

    // Pins name Frog's titles now, in the order they were.
    let (pins,): (String,) = sqlx::query_as("SELECT pins_json FROM app_user WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pins, r#"[{"title_id":1},{"title_id":2}]"#);

    // A new title never takes an id a title once had.
    sqlx::query("INSERT INTO title (kind, name) VALUES ('movie', 'New')")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(count(&pool, "SELECT MAX(id) FROM title").await, 13);

    // Starting again changes nothing.
    pool.close().await;
    let pool = frog::db::connect(&url).await.unwrap();
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM title_external").await, 7);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM title").await, 6);
}

#[tokio::test]
async fn a_fresh_database_has_the_catalog_and_nothing_older() {
    let dir = tempfile::tempdir().unwrap();
    let pool = frog::db::connect(&url_in(&dir)).await.unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM pragma_table_info('title') WHERE name = 'tmdb_id'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'title_external'"
        )
        .await,
        1
    );
}

async fn state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = url_in(&dir);
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
    };
    let pool = frog::db::connect(&url).await.unwrap();
    let state = AppState::new(pool, config);
    common::add_title_seed(&state).await;
    (state, dir)
}

/// The database itself refuses what would make the catalog ambiguous: one
/// provider id on two titles, two primaries on one title, IMDb as a source.
#[tokio::test]
async fn the_catalog_refuses_ambiguous_mappings() {
    let (state, _dir) = state().await;
    let pool = &state.pool;
    for sql in [
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (1, 'tv', 1399, 'Thrones')",
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (2, 'tv', -21, 'Anime')",
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    let refused = |sql: &'static str| async move { sqlx::query(sql).execute(pool).await.is_err() };
    assert!(
        refused(
            "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
             VALUES (2, 'tmdb', 'tv/1399', 0, 'now')"
        )
        .await,
        "one TMDB id on two titles"
    );
    assert!(
        refused(
            "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
             VALUES (1, 'anilist', '77', 1, 'now')"
        )
        .await,
        "a second primary"
    );
    assert!(
        refused(
            "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
             VALUES (1, 'imdb', 'tt1', 1, 'now')"
        )
        .await,
        "IMDb as a source"
    );
    assert!(
        refused(
            "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
             VALUES (1, 'tvdb', '1', 0, 'now')"
        )
        .await,
        "a provider Frog does not know"
    );
    // An alias of another provider is fine.
    assert!(
        !refused(
            "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
             VALUES (1, 'anilist', '78', 0, 'now')"
        )
        .await
    );
}

/// Two titles with different providers turn out to be one. Each account's
/// data moves on its own; the old title's AniList id then answers with the
/// survivor, and the survivor lists it among its own.
#[tokio::test]
async fn merging_titles_of_different_providers_keeps_ids_resolving_and_accounts_apart() {
    let (state, _dir) = state().await;
    let pool = &state.pool;
    for sql in [
        "INSERT INTO app_user (id, username, password_hash, created_at) VALUES
            (1, 'mihai', 'x', 'now'), (2, 'zjef', 'x', 'now')",
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (1, 'tv', 1399, 'On TMDB')",
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (2, 'tv', -21, 'On AniList')",
        "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
         VALUES (1, 'imdb', 'tt0944947', 0, 'now'), (2, 'anilist', '22', 0, 'now')",
        "INSERT INTO library_entry (user_id, title_id, status, rating, added_at, updated_at) VALUES
            (1, 2, 'watching', 70, 'a', 'a'),
            (2, 2, 'planned', NULL, 'a', 'a'),
            (2, 1, 'completed', 90, 'a', 'a')",
    ] {
        sqlx::query(sql)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
    }

    // The first account's data moves; the second still keeps the old title.
    assert!(frog::store::move_title_data(pool, 1, 2, 1, "test")
        .await
        .unwrap());
    assert_eq!(
        count(
            pool,
            "SELECT COUNT(*) FROM library_entry WHERE title_id = 2"
        )
        .await,
        1,
        "only the first account's entry moved"
    );
    assert_eq!(
        catalog::find(pool, &ExternalId::AniList(21)).await.unwrap(),
        Some(2),
        "still somebody's title, so it is still itself"
    );

    // The second account's moves, onto a title it already has: one entry,
    // the further-along status, its own rating kept. A show's own rating is
    // never one AniList entry's score (that is kept in `title_merge`), so
    // the first account's 70 does not become the show's.
    assert!(frog::store::move_title_data(pool, 2, 2, 1, "test")
        .await
        .unwrap());
    let entries: Vec<(i64, i64, String, Option<f64>)> = sqlx::query_as(
        "SELECT user_id, title_id, status, rating FROM library_entry ORDER BY user_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        entries,
        vec![
            (1, 1, "watching".into(), None),
            (2, 1, "completed".into(), Some(90.0))
        ]
    );

    // Nobody keeps it now: both its AniList ids answer with the survivor.
    for id in [21, 22] {
        assert_eq!(
            catalog::find(pool, &ExternalId::AniList(id)).await.unwrap(),
            Some(1),
            "anilist {id}"
        );
    }
    let ids = catalog::externals(pool, 1).await.unwrap();
    assert_eq!(ids.primary, Some("tmdb"));
    assert_eq!(ids.anilist, vec![21, 22]);
    assert_eq!(ids.imdb, vec!["tt0944947".to_string()]);
    // The survivor's own provider is unchanged, and so is the old title's.
    assert_eq!(
        catalog::primary(pool, 1).await.unwrap(),
        Some(ExternalId::tmdb("tv", 1399))
    );
    assert_eq!(
        catalog::primary(pool, 2).await.unwrap(),
        Some(ExternalId::AniList(21))
    );
    assert_eq!(
        count(pool, "SELECT COUNT(*) FROM title_merge").await,
        2,
        "each account's move is on record"
    );
}

/// A provider that no longer has a title takes nothing with it.
#[tokio::test]
async fn a_provider_losing_a_title_keeps_the_title_and_its_history() {
    let (state, _dir) = state().await;
    let pool = &state.pool;
    for sql in [
        "INSERT INTO app_user (id, username, password_hash, created_at) VALUES (1, 'm', 'x', 'now')",
        "INSERT INTO title_seed (id, kind, tmdb_id, name) VALUES (1, 'movie', 550, 'Kept')",
        "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
         VALUES (1, 1, 'completed', 'a', 'a')",
        "INSERT INTO watch_event (user_id, title_id, watched_at) VALUES (1, 1, '2026-01-01')",
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    catalog::mark_gone(pool, &ExternalId::tmdb("movie", 550))
        .await
        .unwrap();
    assert_eq!(
        count(
            pool,
            "SELECT COUNT(*) FROM title_external WHERE gone_at IS NOT NULL"
        )
        .await,
        1
    );
    // Still found, still opens, history intact. No TMDB key here, and TMDB
    // has no cached answer, so it opens with what Frog kept.
    assert_eq!(
        catalog::find(pool, &ExternalId::tmdb("movie", 550))
            .await
            .unwrap(),
        Some(1)
    );
    assert_eq!(count(pool, "SELECT COUNT(*) FROM watch_event").await, 1);
    assert_eq!(count(pool, "SELECT COUNT(*) FROM library_entry").await, 1);
    assert_eq!(catalog::get_title(pool, 1).await.unwrap().name, "Kept");
}

/// A title made from an AniList entry alone has no genres, and must still say
/// so as JSON: the profile, stats and Home all read genres with `json_each`,
/// which fails the whole query on anything else.
#[tokio::test]
async fn an_anilist_only_title_reads_like_any_other() {
    let (state, _dir) = state().await;
    let pool = &state.pool;
    let title_id = catalog::ensure_local_title(
        pool,
        &catalog::LocalTitle {
            anilist_id: 21,
            kind: "tv",
            name: "Only on AniList",
            original_name: None,
            release_date: None,
            episodes: Some(12),
            status: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        catalog::primary(pool, title_id).await.unwrap(),
        Some(ExternalId::AniList(21))
    );
    let genres: (i64,) = sqlx::query_as(
        "SELECT COUNT(g.value) FROM title t, json_each(t.genres_json) g WHERE t.id = ?",
    )
    .bind(title_id)
    .fetch_one(pool)
    .await
    .expect("genres are readable JSON");
    assert_eq!(genres.0, 0);
}

/// If the rebuild ever fails, `deploy/update.sh` may put the previous image
/// back on a database the catalog migration has already run on. That binary
/// keeps adding titles with only `tmdb_id` and saving pins its own way. The
/// next start must give those titles their provider before the column goes.
#[tokio::test]
async fn titles_an_older_binary_adds_after_the_migration_keep_their_provider() {
    let dir = tempfile::tempdir().unwrap();
    let url = url_in(&dir);
    old_database(&url).await;
    {
        let pool = SqlitePoolOptions::new()
            .connect_with(SqliteConnectOptions::from_str(&url).unwrap())
            .await
            .unwrap();
        // The migrations ran; the rebuild did not.
        frog::db::migrator().run(&pool).await.unwrap();
        for sql in [
            "INSERT INTO title (id, kind, tmdb_id, name, imdb_id) VALUES
                (13, 'movie', 603, 'The Matrix', 'tt0133093')",
            "INSERT INTO library_entry (user_id, title_id, status, added_at, updated_at)
             VALUES (2, 13, 'completed', '2026-02-01T00:00:00Z', '2026-02-01T00:00:00Z')",
            "UPDATE app_user SET pins_json = '[{\"kind\":\"movie\",\"tmdb_id\":603}]' WHERE id = 2",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        pool.close().await;
    }

    let pool = frog::db::connect(&url).await.unwrap();
    assert_eq!(
        catalog::find(&pool, &ExternalId::tmdb("movie", 603))
            .await
            .unwrap(),
        Some(13)
    );
    assert_eq!(
        catalog::find(&pool, &ExternalId::Imdb("tt0133093".into()))
            .await
            .unwrap(),
        Some(13)
    );
    let (pins,): (String,) = sqlx::query_as("SELECT pins_json FROM app_user WHERE id = 2")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pins, r#"[{"title_id":13}]"#);
    // Pins the migration already converted are left as they are.
    let (mine,): (String,) = sqlx::query_as("SELECT pins_json FROM app_user WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mine, r#"[{"title_id":1},{"title_id":2}]"#);
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM library_entry WHERE title_id = 13"
        )
        .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT user_version FROM pragma_user_version").await,
        frog::db::SCHEMA_GENERATION
    );
}
