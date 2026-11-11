//! Administration → Database keeps every secret back and cannot write.
//!
//! The first test reads the real schema, so a migration that adds a column
//! named like a secret fails here until `dbview::HIDDEN` or `dbview::PLAIN`
//! says what it is.

use frog::dbview::{self, Hide};

async fn migrated() -> (tempfile::TempDir, String, sqlx::SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite:{}?mode=rwc",
        dir.path()
            .join("view.db")
            .to_string_lossy()
            .replace('\\', "/")
    );
    let pool = frog::db::connect(&url).await.unwrap();
    (dir, url, pool)
}

#[tokio::test]
async fn every_column_named_like_a_secret_is_kept_back_or_explained() {
    let (_dir, url, _pool) = migrated().await;
    let mut conn = dbview::reader(&url).await.unwrap();
    let mut missed = Vec::new();
    for table in dbview::tables(&mut conn).await.unwrap() {
        let (_, columns) = dbview::columns(&mut conn, &table.name)
            .await
            .unwrap()
            .unwrap();
        for column in columns {
            let lower = column.name.to_ascii_lowercase();
            let sounds_secret = dbview::SECRET_WORDS.iter().any(|w| lower.contains(w));
            let explained = dbview::PLAIN
                .iter()
                .any(|(t, c, _)| *t == table.name && *c == column.name);
            if sounds_secret && column.hidden.is_none() && !explained {
                missed.push(format!("{}.{}", table.name, column.name));
            }
        }
    }
    assert!(
        missed.is_empty(),
        "add these to dbview::HIDDEN, or to dbview::PLAIN with the reason: {missed:?}"
    );

    // And every listed column still exists, so the list cannot rot quietly.
    for (table, column, _) in dbview::HIDDEN.iter().map(|(t, c, h)| (t, c, Some(*h))) {
        let (_, columns) = dbview::columns(&mut conn, table)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{table} is gone"));
        assert!(
            columns.iter().any(|c| c.name == *column),
            "{table}.{column} is gone; take it off dbview::HIDDEN"
        );
    }
}

#[tokio::test]
async fn the_reader_cannot_write() {
    let (_dir, url, pool) = migrated().await;
    let mut conn = dbview::reader(&url).await.unwrap();
    for sql in [
        "INSERT INTO title (kind, name) VALUES ('movie', 'Nope')",
        "UPDATE app_user SET role = 'owner'",
        "DELETE FROM _sqlx_migrations",
        "DROP TABLE title",
        "CREATE TABLE nope (id INTEGER)",
        "PRAGMA query_only = OFF; DELETE FROM _sqlx_migrations",
    ] {
        assert!(
            sqlx::raw_sql(sql).execute(&mut conn).await.is_err(),
            "the reader ran: {sql}"
        );
    }
    let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(left > 10, "nothing was deleted");
}

#[tokio::test]
async fn blobs_and_cached_answers_go_out_as_their_size() {
    assert_eq!(
        dbview::hidden("tmdb_cache", "body", "TEXT"),
        Some(Hide::Size)
    );
    assert_eq!(
        dbview::hidden("image_cache", "bytes", "BLOB"),
        Some(Hide::Size)
    );
    assert_eq!(
        dbview::hidden("push_vapid", "private_pkcs8", "BLOB"),
        Some(Hide::Secret)
    );
}
