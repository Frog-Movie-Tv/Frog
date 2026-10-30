//! The picture cache held to its budget against a real database: only cached
//! pictures go, the least recently served first, running it again is
//! harmless, and a cache it cannot read is reported rather than half-done.

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use frog::images::{evict, Evicted, MAX_IDLE_DAYS};

async fn database() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite:{}?mode=rwc",
        dir.path()
            .join("frog.db")
            .to_string_lossy()
            .replace('\\', "/")
    );
    (frog::db::connect(&url).await.unwrap(), dir)
}

/// A picture of `size` bytes last served `days_ago`.
async fn picture(pool: &SqlitePool, key: &str, size: usize, days_ago: i64, now: DateTime<Utc>) {
    let at = (now - Duration::days(days_ago)).to_rfc3339();
    sqlx::query(
        "INSERT INTO image_cache (path, content_type, bytes, fetched_at, served_at, size)
         VALUES (?, 'image/jpeg', ?, ?, ?, ?)",
    )
    .bind(key)
    .bind(vec![7u8; size])
    .bind(&at)
    .bind(&at)
    .bind(size as i64)
    .execute(pool)
    .await
    .unwrap();
}

async fn keys(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_as::<_, (String,)>("SELECT path FROM image_cache ORDER BY path")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.0)
        .collect()
}

#[tokio::test]
async fn the_cache_is_trimmed_oldest_first_and_nothing_else_is_touched() {
    let (pool, _dir) = database().await;
    let now = Utc::now();
    picture(&pool, "w342/old.jpg", 400, 30, now).await;
    picture(&pool, "w342/older.jpg", 400, 60, now).await;
    picture(&pool, "w342/new.jpg", 400, 1, now).await;
    picture(&pool, "anilist/media/idle.jpg", 10, MAX_IDLE_DAYS + 1, now).await;
    // Something else in the database, which eviction must leave alone.
    sqlx::query(
        "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
         VALUES ('/movie/1', '{}', 'a', 'z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // 1,210 bytes against 1,000: the idle one goes whatever the budget, then
    // the oldest until 900 or less is left.
    let gone = evict(&pool, 1_000, now).await.unwrap();
    assert_eq!(
        gone,
        Evicted {
            pictures: 2,
            bytes: 410
        }
    );
    assert_eq!(keys(&pool).await, ["w342/new.jpg", "w342/old.jpg"]);
    let (others,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tmdb_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(others, 1);

    // Again, with nothing over: nothing happens.
    assert_eq!(evict(&pool, 1_000, now).await.unwrap(), Evicted::default());
    assert_eq!(keys(&pool).await.len(), 2);
}

#[tokio::test]
async fn a_picture_with_no_size_or_an_unreadable_date_is_still_counted() {
    let (pool, _dir) = database().await;
    let now = Utc::now();
    // As a row from before the budget looked, or a date nobody can read.
    sqlx::query(
        "INSERT INTO image_cache (path, content_type, bytes, fetched_at, served_at, size)
         VALUES ('w92/a.jpg', 'image/jpeg', ?, 'not a date', NULL, NULL)",
    )
    .bind(vec![1u8; 50])
    .execute(&pool)
    .await
    .unwrap();
    picture(&pool, "w92/b.jpg", 50, 1, now).await;
    // The unreadable one reads as the oldest there is, so it goes first.
    let gone = evict(&pool, 60, now).await.unwrap();
    assert_eq!(gone.pictures, 1);
    assert_eq!(keys(&pool).await, ["w92/b.jpg"]);
}

#[tokio::test]
async fn a_cache_that_cannot_be_read_is_an_error_not_a_partial_trim() {
    let (pool, _dir) = database().await;
    let now = Utc::now();
    picture(&pool, "w92/a.jpg", 50, 400, now).await;
    sqlx::query("ALTER TABLE image_cache RENAME TO image_cache_away")
        .execute(&pool)
        .await
        .unwrap();
    assert!(evict(&pool, 10, now).await.is_err());
    sqlx::query("ALTER TABLE image_cache_away RENAME TO image_cache")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(keys(&pool).await, ["w92/a.jpg"], "nothing went");
}
