//! What Administration reads and changes: every account, and how big the
//! instance is. Who may do what to whom is `routes::admin`'s check.

use sqlx::SqlitePool;

use crate::auth::Role;
use crate::error::AppResult;

/// (id, username, role, disabled, created_at, titles tracked, last sign-in)
pub type UserRow = (i64, String, String, bool, String, i64, Option<String>);

/// Every account, the owner first, then administrators, then by name.
pub async fn users(pool: &SqlitePool) -> AppResult<Vec<UserRow>> {
    Ok(sqlx::query_as(
        "SELECT u.id, u.username, u.role, u.disabled, u.created_at,
                (SELECT COUNT(*) FROM library_entry l WHERE l.user_id = u.id),
                u.last_sign_in_at
           FROM app_user u
          ORDER BY CASE u.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
                   u.username COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?)
}

/// An account's role and whether it is switched off, when it exists.
pub async fn standing(pool: &SqlitePool, id: i64) -> AppResult<Option<(Role, bool)>> {
    let row: Option<(String, bool)> =
        sqlx::query_as("SELECT role, disabled FROM app_user WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(role, disabled)| (Role::from_db(&role), disabled)))
}

pub async fn set_role(pool: &SqlitePool, id: i64, role: Role) -> AppResult<()> {
    sqlx::query("UPDATE app_user SET role = ? WHERE id = ?")
        .bind(role.as_db())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_disabled(pool: &SqlitePool, id: i64, disabled: bool) -> AppResult<()> {
    sqlx::query("UPDATE app_user SET disabled = ? WHERE id = ?")
        .bind(disabled)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove an account; everything it owns goes with it (the keys cascade).
pub async fn delete_account(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM app_user WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// How big the instance is.
pub struct Figures {
    pub database_bytes: i64,
    /// Where those bytes go, each once: they add up to `database_bytes`.
    pub storage: Storage,
    pub accounts: i64,
    /// Titles this Frog knows, by its own id: all, films, shows, manga. One
    /// folded into another counts once.
    pub titles: (i64, i64, i64, i64),
}

pub async fn figures(pool: &SqlitePool) -> AppResult<Figures> {
    let (pages, page_size): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT page_count FROM pragma_page_count()), (SELECT page_size FROM pragma_page_size())",
    )
    .fetch_one(pool)
    .await?;
    let storage = storage(pool, pages * page_size).await?;
    let titles: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(kind = 'movie'), 0), COALESCE(SUM(kind = 'tv'), 0),
                COALESCE(SUM(kind = 'manga'), 0)
           FROM title WHERE merged_into IS NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(Figures {
        database_bytes: pages * page_size,
        storage,
        accounts: crate::auth::account_count(pool).await?,
        titles,
    })
}

/// The database file, split by what it holds. Pictures are kept inside the
/// database, so "the database and the artwork" counted them twice.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Storage {
    /// Everything the accounts made: libraries, watches, ratings, shelves,
    /// settings, notifications, sessions, badges given.
    pub yours: i64,
    /// What the titles are, shared by everyone: titles, episodes, credits.
    pub catalogue: i64,
    /// Answers kept from TMDB, AniList, TVmaze and the rating sites, which
    /// Frog asks for again when they are gone.
    pub caches: i64,
    /// Posters and backdrops kept offline, and pictures people uploaded.
    pub pictures: i64,
    /// Pages SQLite freed and keeps for reuse; a `VACUUM` gives them back.
    pub free: i64,
}

/// Which part of `Storage` a table belongs to. A table not named here is an
/// account's own, which is where anything new usually lands.
fn part_of(table: &str) -> &'static str {
    match table {
        "image_cache" | "user_image" => "pictures",
        "tmdb_cache" | "anilist_media" | "anilist_chain" | "anime_map" | "anime_map_state"
        | "airing_show" | "airing_episode" | "airing_film" | "airing_state" | "airtime_show"
        | "title_rating" | "title_rating_check" | "title_logo_check" | "studio_match"
        | "title_character" => "caches",
        "title" | "title_external" | "title_merge" | "episode" | "title_person" | "badge"
        | "push_vapid" => "catalogue",
        t if t.starts_with("sqlite_") || t.starts_with("_sqlx") => "catalogue",
        _ => "yours",
    }
}

/// SQLite's own page count per table and its indexes (`dbstat`), so every
/// byte of the file is counted once. Reading it walks the whole file, so the
/// answer is kept for ten minutes.
async fn storage(pool: &SqlitePool, total: i64) -> AppResult<Storage> {
    static KEPT: std::sync::Mutex<Option<(std::time::Instant, Storage)>> =
        std::sync::Mutex::new(None);
    if let Ok(kept) = KEPT.lock() {
        if let Some((at, s)) = kept.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(600) {
                return Ok(s.clone());
            }
        }
    }
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT COALESCE(m.tbl_name, d.name), SUM(d.pgsize)
           FROM dbstat d LEFT JOIN sqlite_master m ON m.name = d.name
          GROUP BY 1",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Storage::default();
    for (table, bytes) in rows {
        match part_of(&table) {
            "pictures" => out.pictures += bytes,
            "caches" => out.caches += bytes,
            "catalogue" => out.catalogue += bytes,
            _ => out.yours += bytes,
        }
    }
    // What no table holds: the free list, and the header page.
    out.free = (total - out.pictures - out.caches - out.catalogue - out.yours).max(0);
    if let Ok(mut kept) = KEPT.lock() {
        *kept = Some((std::time::Instant::now(), out.clone()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::part_of;

    #[test]
    fn every_table_lands_in_one_part() {
        assert_eq!(part_of("image_cache"), "pictures");
        assert_eq!(part_of("tmdb_cache"), "caches");
        assert_eq!(part_of("episode"), "catalogue");
        assert_eq!(part_of("watch_event"), "yours");
        assert_eq!(part_of("sqlite_sequence"), "catalogue");
    }
}
