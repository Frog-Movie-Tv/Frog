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

    let migrator = migrator();
    heal_line_endings(&pool, &migrator).await?;
    migrator.run(&pool).await?;
    rebuild_title(&pool).await?;
    mark_generation(&pool).await?;

    fill_search_names(&pool).await?;
    Ok(pool)
}

/// The `title` table as Frog keeps it: its own id and the metadata it shows.
/// Which provider knows it lives in `title_external`. The column names
/// `tmdb_status` and `tmdb_vote` are kept from when TMDB was the only
/// source; they hold the primary provider's values.
const TITLE_TABLE: &str = "CREATE TABLE title_rebuilt (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    kind             TEXT    NOT NULL CHECK (kind IN ('movie', 'tv')),
    name             TEXT    NOT NULL,
    original_name    TEXT,
    overview         TEXT,
    poster_path      TEXT,
    backdrop_path    TEXT,
    logo_path        TEXT,
    release_date     TEXT,
    last_air_date    TEXT,
    tmdb_status      TEXT,
    runtime          INTEGER,
    episode_count    INTEGER,
    season_count     INTEGER,
    genres_json      TEXT    NOT NULL DEFAULT '[]',
    tmdb_vote        REAL,
    collection_id    INTEGER,
    accent_json      TEXT,
    refreshed_at     TEXT,
    people_synced_at TEXT,
    search_name      TEXT,
    next_air_date    TEXT,
    next_air_season  INTEGER,
    next_air_episode INTEGER,
    merged_into      INTEGER REFERENCES title (id) ON DELETE SET NULL
)";

/// The columns copied across, which are every column `TITLE_TABLE` has.
const TITLE_COLUMNS: &str = "id, kind, name, original_name, overview, poster_path, backdrop_path,
    logo_path, release_date, last_air_date, tmdb_status, runtime, episode_count, season_count,
    genres_json, tmdb_vote, collection_id, accent_json, refreshed_at, people_synced_at,
    search_name, next_air_date, next_air_season, next_air_episode, merged_into";

/// Drop the provider ids `title` used to carry (`tmdb_id`, `anilist_id`,
/// `imdb_id`) once `202610301000_catalog_identity` has copied them into
/// `title_external`.
///
/// This is a migration that SQLite cannot run inside sqlx's migration
/// transaction: a column in a UNIQUE constraint only goes by rebuilding the
/// table, and rebuilding a table others reference needs foreign keys off,
/// which SQLite ignores inside a transaction. With them on, the old table's
/// drop would cascade into every library, watch and episode. So it runs here,
/// on one connection with foreign keys off, in its own transaction, and only
/// while `title` still has `tmdb_id`: running it again does nothing, and an
/// interrupted run leaves the old table as it was. Ids are copied as they
/// are and the id sequence is carried over, so no Frog id is ever reused.
pub async fn rebuild_title(pool: &SqlitePool) -> anyhow::Result<()> {
    let (pending,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM pragma_table_info('title') WHERE name = 'tmdb_id'")
            .fetch_one(pool)
            .await?;
    if pending == 0 {
        return Ok(());
    }
    let mut conn = pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    let result = rebuild_title_on(&mut conn).await;
    // Back on before the connection returns to the pool, whatever happened.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await?;
    result
}

/// The schema generation this binary leaves, in SQLite's `user_version`.
/// It goes up only when a release changes the schema so that the release
/// before it can no longer run on it. `deploy/update.sh` compares it with
/// the pre-update snapshot: if a failed update raised it, starting the
/// previous image would look healthy (its health check is `SELECT 1`) and
/// break on every title, so it restores the snapshot instead. Generation 1
/// is the catalog: `title` without provider ids.
pub const SCHEMA_GENERATION: i64 = 1;

async fn mark_generation(pool: &SqlitePool) -> anyhow::Result<()> {
    let (current,): (i64,) = sqlx::query_as("PRAGMA user_version")
        .fetch_one(pool)
        .await?;
    if current < SCHEMA_GENERATION {
        sqlx::query(&format!("PRAGMA user_version = {SCHEMA_GENERATION}"))
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// What `202610301000_catalog_identity` did, again, for rows an older binary
/// wrote after it ran. If this rebuild ever fails, `deploy/update.sh` can put
/// the previous image back on the migrated database, and that binary adds
/// titles with only `tmdb_id` and saves pins as `{ kind, tmdb_id }`. Without
/// this, the next rebuild would leave those titles with no provider.
const CATCH_UP: [&str; 5] = [
    "INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
     SELECT t.id, 'tmdb', t.kind || '/' || t.tmdb_id, 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
       FROM title t
      WHERE t.tmdb_id > 0
        AND NOT EXISTS (SELECT 1 FROM title_external x WHERE x.title_id = t.id AND x.is_primary = 1)",
    "INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
     SELECT t.id, 'anilist', CAST(-t.tmdb_id AS TEXT), 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
       FROM title t
      WHERE t.tmdb_id < 0
        AND NOT EXISTS (SELECT 1 FROM title_external x WHERE x.title_id = t.id AND x.is_primary = 1)",
    "INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
     SELECT id, 'anilist', CAST(anilist_id AS TEXT), 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
       FROM title WHERE anilist_id IS NOT NULL AND tmdb_id > 0",
    "INSERT OR IGNORE INTO title_external (title_id, provider, external_id, is_primary, added_at)
     SELECT id, 'imdb', imdb_id, 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
       FROM title WHERE imdb_id IS NOT NULL AND imdb_id != '' ORDER BY id",
    "UPDATE app_user
        SET pins_json = (
            SELECT COALESCE(json_group_array(json_object('title_id', title_id)), '[]')
              FROM (SELECT t.id AS title_id
                      FROM json_each(app_user.pins_json) p
                      JOIN title t ON t.kind = json_extract(p.value, '$.kind')
                                  AND t.tmdb_id = json_extract(p.value, '$.tmdb_id')
                     ORDER BY p.key))
      WHERE pins_json IS NOT NULL AND json_valid(pins_json)
        AND EXISTS (SELECT 1 FROM json_each(app_user.pins_json) p
                     WHERE json_extract(p.value, '$.tmdb_id') IS NOT NULL)",
];

async fn rebuild_title_on(conn: &mut sqlx::SqliteConnection) -> anyhow::Result<()> {
    use sqlx::Connection;
    let mut tx = conn.begin().await?;
    for sql in CATCH_UP {
        sqlx::query(sql).execute(&mut *tx).await?;
    }
    // Every title must have its provider before the ids go: a title an
    // older binary wrote under an id another title already holds is not
    // one this can settle by itself.
    let (unmapped,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM title t
          WHERE (t.tmdb_id IS NOT NULL AND t.tmdb_id != 0)
            AND NOT EXISTS (SELECT 1 FROM title_external x WHERE x.title_id = t.id AND x.is_primary = 1)",
    )
    .fetch_one(&mut *tx)
    .await?;
    if unmapped > 0 {
        anyhow::bail!("{unmapped} titles have no provider mapping; title was not rebuilt");
    }
    let (sequence,): (Option<i64>,) =
        sqlx::query_as("SELECT MAX(seq) FROM sqlite_sequence WHERE name = 'title'")
            .fetch_one(&mut *tx)
            .await?;
    let (before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM title")
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(TITLE_TABLE).execute(&mut *tx).await?;
    sqlx::query(&format!(
        "INSERT INTO title_rebuilt ({TITLE_COLUMNS}) SELECT {TITLE_COLUMNS} FROM title"
    ))
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE title").execute(&mut *tx).await?;
    sqlx::query("ALTER TABLE title_rebuilt RENAME TO title")
        .execute(&mut *tx)
        .await?;
    sqlx::query("CREATE INDEX idx_title_name ON title (name)")
        .execute(&mut *tx)
        .await?;
    if let Some(seq) = sequence {
        sqlx::query("UPDATE sqlite_sequence SET seq = MAX(seq, ?) WHERE name = 'title'")
            .bind(seq)
            .execute(&mut *tx)
            .await?;
    }
    let (after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM title")
        .fetch_one(&mut *tx)
        .await?;
    let broken: Vec<(String,)> = sqlx::query_as("SELECT \"table\" FROM pragma_foreign_key_check")
        .fetch_all(&mut *tx)
        .await?;
    if after != before || !broken.is_empty() {
        // Dropping the transaction rolls it back: the old table stays.
        anyhow::bail!(
            "rebuilding title would lose rows ({before} → {after}) or break references in {:?}",
            broken.iter().map(|b| b.0.as_str()).collect::<Vec<_>>()
        );
    }
    tx.commit().await?;
    tracing::info!(titles = after, "title rebuilt without provider ids");
    Ok(())
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

/// Every migration this binary carries. A database may already hold newer
/// ones: when an update fails its health check, `deploy/update.sh` goes back
/// to the previous image, and that binary has to start on a schema one step
/// ahead of it rather than refuse and leave the site down. That only works
/// because migrations stay additive (see `docs/CONTRIBUTING.md`).
pub fn migrator() -> Migrator {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
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
