use sha2::{Digest, Sha384};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

/// Open the SQLite pool, creating the file and parent directory if needed, then
/// run migrations. WAL + a busy timeout keep the background worker and the HTTP
/// handlers from tripping over each other.
pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    if let Some(path) = sqlite_path(database_url) {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
    }

    let opts = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(15))
        .connect_with(opts)
        .await?;

    let migrator = sqlx::migrate!("./migrations");
    heal_line_endings(&pool, &migrator).await?;
    migrator.run(&pool).await?;

    fill_search_names(&pool).await?;
    Ok(pool)
}

/// Give every title its folded `search_name`. The migration that added the
/// column cannot compute it (SQL has no way to take accents out), and a
/// binary from before it writes titles without one, so this runs at every
/// start and only touches rows that still lack it.
pub async fn fill_search_names(pool: &SqlitePool) -> anyhow::Result<()> {
    let rows: Vec<(i64, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, original_name FROM title WHERE search_name IS NULL")
            .fetch_all(pool)
            .await?;
    if rows.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    for (id, name, original) in &rows {
        sqlx::query("UPDATE title SET search_name = ? WHERE id = ?")
            .bind(crate::text::search_name(name, original.as_deref()))
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    tracing::info!(titles = rows.len(), "search names filled");
    Ok(())
}

/// sqlx checksums each migration byte for byte, and a Windows checkout can turn
/// LF into CRLF (or back). A database migrated from one checkout would then
/// refuse to start under a build from another, though nothing really changed.
/// Before migrating, accept a stored checksum that differs only in line
/// endings and record the current one. Any other change is still refused.
pub async fn heal_line_endings(pool: &SqlitePool, migrator: &Migrator) -> anyhow::Result<()> {
    // A fresh database has no history yet.
    let Ok(applied) =
        sqlx::query_as::<_, (i64, Vec<u8>)>("SELECT version, checksum FROM _sqlx_migrations")
            .fetch_all(pool)
            .await
    else {
        return Ok(());
    };

    for m in migrator.iter() {
        let Some((_, stored)) = applied.iter().find(|(v, _)| *v == m.version) else {
            continue;
        };
        if stored.as_slice() == m.checksum.as_ref() {
            continue;
        }
        let lf = m.sql.replace("\r\n", "\n");
        let crlf = lf.replace('\n', "\r\n");
        let same_sql = [lf, crlf]
            .iter()
            .any(|sql| Sha384::digest(sql.as_bytes()).as_slice() == stored.as_slice());
        if same_sql {
            sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
                .bind(m.checksum.as_ref())
                .bind(m.version)
                .execute(pool)
                .await?;
            tracing::info!(version = m.version, "migration line endings reconciled");
        }
    }
    Ok(())
}

/// Pull the filesystem path out of a `sqlite:` URL so we can mkdir -p for it.
pub fn sqlite_path(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))?;
    if rest == ":memory:" || rest.starts_with(":memory:") {
        return None;
    }
    Some(rest.split('?').next().unwrap_or(rest).to_string())
}

#[cfg(test)]
mod tests {
    use super::sqlite_path;

    #[test]
    fn extracts_path_from_url_forms() {
        assert_eq!(
            sqlite_path("sqlite:./data/frog.db?mode=rwc"),
            Some("./data/frog.db".into())
        );
        assert_eq!(sqlite_path("sqlite:///tmp/a.db"), Some("/tmp/a.db".into()));
        assert_eq!(sqlite_path("sqlite::memory:"), None);
    }
}
