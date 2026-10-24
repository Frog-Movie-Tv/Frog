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

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Pull the filesystem path out of a `sqlite:` URL so we can mkdir -p for it.
fn sqlite_path(url: &str) -> Option<String> {
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
