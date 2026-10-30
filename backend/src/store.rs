//! One account's library: entries, progress, "up next", and moving an
//! account's data from one title to another. The titles themselves, shared
//! by everyone, are `crate::catalog`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use crate::catalog::{parse_accent, parse_genres, TitleRow};
use crate::error::{AppError, AppResult};
use crate::palette::Palette;

pub const STATUSES: [&str; 5] = ["watching", "completed", "planned", "paused", "dropped"];

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LibraryEntry {
    pub status: String,
    pub rating: Option<f64>,
    pub favourite: bool,
    pub notes: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub rewatch_count: i64,
    pub added_at: String,
    pub updated_at: String,
    /// New-episode notifications for this show.
    pub notify: bool,
}

/// A title as the grid and the rows draw it: metadata, my state, my progress.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryItem {
    #[serde(flatten)]
    pub title: TitleRow,
    pub genres: Vec<String>,
    pub accent: Palette,
    pub entry: Option<LibraryEntry>,
    pub watched_episodes: i64,
    pub total_episodes: i64,
    pub last_watched_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryPatch {
    pub status: Option<String>,
    /// 0..100 internally; the UI converts to whichever score format is set.
    pub rating: Option<Option<f64>>,
    pub favourite: Option<bool>,
    pub notes: Option<Option<String>>,
    pub notify: Option<bool>,
}

/// (status, rating, favourite, notes, started, finished, rewatches, added, notify, progress)
type MergeEntryRow = (
    String,
    Option<f64>,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    String,
    bool,
    Option<i64>,
);

/// Move one account's data from one title to another: the library entry,
/// shelves and film watches. Used when an AniList-only title turns out to be
/// a TMDB one, or a person says where it belongs.
///
/// Nothing is dropped on a clash: the further-along status wins, the rating
/// and dates already on the target stay, notes are kept side by side, and a
/// film watch AniList made up is not copied onto a film that has a watch.
/// What both sides held before is written to `title_merge`. All of it is one
/// transaction, so a failure leaves both titles as they were, and running it
/// again finds nothing left to move. Says whether anything moved.
pub async fn move_title_data(
    pool: &SqlitePool,
    user_id: i64,
    from: i64,
    to: i64,
    reason: &str,
) -> AppResult<bool> {
    if from == to {
        return Ok(false);
    }
    let mut tx = pool.begin().await?;
    let kinds: Vec<(i64, String)> = sqlx::query_as("SELECT id, kind FROM title WHERE id IN (?, ?)")
        .bind(from)
        .bind(to)
        .fetch_all(&mut *tx)
        .await?;
    let kind_of = |id: i64| kinds.iter().find(|k| k.0 == id).map(|k| k.1.clone());
    let (Some(from_kind), Some(to_kind)) = (kind_of(from), kind_of(to)) else {
        return Err(AppError::NotFound("no such title".into()));
    };

    let entry_sql = "SELECT status, rating, favourite, notes, started_at, finished_at,
                            rewatch_count, added_at, notify, progress
                       FROM library_entry WHERE user_id = ? AND title_id = ?";
    let old: Option<MergeEntryRow> = sqlx::query_as(entry_sql)
        .bind(user_id)
        .bind(from)
        .fetch_optional(&mut *tx)
        .await?;
    let have: Option<MergeEntryRow> = sqlx::query_as(entry_sql)
        .bind(user_id)
        .bind(to)
        .fetch_optional(&mut *tx)
        .await?;
    let (films,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watch_event WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
    )
    .bind(user_id)
    .bind(from)
    .fetch_one(&mut *tx)
    .await?;
    let (shelved,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
          WHERE s.user_id = ? AND i.title_id = ?",
    )
    .bind(user_id)
    .bind(from)
    .fetch_one(&mut *tx)
    .await?;
    if old.is_none() && films == 0 && shelved == 0 {
        return Ok(false);
    }

    let as_json = |e: &Option<MergeEntryRow>| {
        e.as_ref().map(|e| {
            serde_json::json!({
                "status": e.0, "rating": e.1, "favourite": e.2, "notes": e.3,
                "started_at": e.4, "finished_at": e.5, "rewatch_count": e.6,
                "added_at": e.7, "notify": e.8, "progress": e.9,
            })
        })
    };
    let before = serde_json::json!({
        "from": as_json(&old),
        "to": as_json(&have),
        "film_watches": films,
        "shelved": shelved,
    });

    if let Some(o) = &old {
        let now = Utc::now().to_rfc3339();
        let merged = match &have {
            None => o.clone(),
            Some(h) => {
                let notes = match (&h.3, &o.3) {
                    (Some(a), Some(b)) if a.trim() != b.trim() => Some(format!("{a}\n\n{b}")),
                    (a, b) => a.clone().or_else(|| b.clone()),
                };
                (
                    crate::transfer::merged_status(&h.0, &o.0),
                    h.1.or(o.1),
                    h.2 || o.2,
                    notes,
                    min_opt(&h.4, &o.4),
                    max_opt(&h.5, &o.5),
                    h.6.max(o.6),
                    h.7.clone().min(o.7.clone()),
                    h.8,
                    h.9,
                )
            }
        };
        // A film's rating is the film's. A show's own rating is not what one
        // AniList entry was scored (that lands on its season when the entry
        // is synced there), so it stays in the record rather than moving.
        let rating = if to_kind == "tv" {
            have.as_ref().and_then(|h| h.1)
        } else {
            merged.1
        };
        sqlx::query(
            "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes,
                                        started_at, finished_at, rewatch_count, added_at,
                                        updated_at, notify)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (user_id, title_id) DO UPDATE SET
                status = excluded.status, rating = excluded.rating,
                favourite = excluded.favourite, notes = excluded.notes,
                started_at = excluded.started_at, finished_at = excluded.finished_at,
                rewatch_count = excluded.rewatch_count, added_at = excluded.added_at,
                updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(to)
        .bind(&merged.0)
        .bind(rating)
        .bind(merged.2)
        .bind(&merged.3)
        .bind(&merged.4)
        .bind(&merged.5)
        .bind(merged.6)
        .bind(&merged.7)
        .bind(&now)
        .bind(merged.8)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM library_entry WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(from)
            .execute(&mut *tx)
            .await?;
    }

    if from_kind == "movie" && to_kind == "movie" {
        // Made-up watches only go across when the film has none of its own.
        sqlx::query(
            "DELETE FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                AND source = 'anilist' AND undated = 1
                AND EXISTS (SELECT 1 FROM watch_event w
                             WHERE w.user_id = ? AND w.title_id = ? AND w.episode_id IS NULL)",
        )
        .bind(user_id)
        .bind(from)
        .bind(user_id)
        .bind(to)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE watch_event SET title_id = ?
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
        )
        .bind(to)
        .bind(user_id)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    } else {
        // A film-level watch cannot become a show's: AniList's own made-up one
        // goes (the show's episodes carry it now); any logged by hand stay on
        // the old title, where the diary still has them.
        sqlx::query(
            "DELETE FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                AND source = 'anilist' AND undated = 1",
        )
        .bind(user_id)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
         SELECT i.shelf_id, ?, i.position FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
          WHERE s.user_id = ? AND i.title_id = ?",
    )
    .bind(to)
    .bind(user_id)
    .bind(from)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM shelf_item
          WHERE title_id = ? AND shelf_id IN (SELECT id FROM shelf WHERE user_id = ?)",
    )
    .bind(from)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO title_merge (user_id, from_title, to_title, reason, before_json, merged_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(reason)
    .bind(before.to_string())
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;

    // Once nobody keeps an AniList-only title, it points at where it went,
    // so an old link, and its AniList id, still find it (`catalog::find`).
    sqlx::query(
        "UPDATE title SET merged_into = ?
          WHERE id = ?
            AND EXISTS (SELECT 1 FROM title_external x
                         WHERE x.title_id = title.id AND x.provider = 'anilist'
                           AND x.is_primary = 1)
            AND NOT EXISTS (SELECT 1 FROM library_entry l WHERE l.title_id = title.id)",
    )
    .bind(to)
    .bind(from)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

fn min_opt(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.clone().min(b.clone())),
        (a, b) => a.clone().or_else(|| b.clone()),
    }
}

fn max_opt(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.clone().max(b.clone())),
        (a, b) => a.clone().or_else(|| b.clone()),
    }
}

pub async fn get_entry(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<LibraryEntry>> {
    let row = sqlx::query_as::<_, LibraryEntry>(
        "SELECT status, rating, favourite, notes, started_at, finished_at,
                rewatch_count, added_at, updated_at, notify
           FROM library_entry WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn apply_patch(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    patch: &LibraryPatch,
) -> AppResult<LibraryEntry> {
    if let Some(status) = &patch.status {
        if !STATUSES.contains(&status.as_str()) {
            return Err(AppError::BadRequest(format!(
                "status must be one of {}",
                STATUSES.join(", ")
            )));
        }
    }
    if let Some(Some(rating)) = patch.rating {
        if !(0.0..=100.0).contains(&rating) {
            return Err(AppError::BadRequest(
                "rating must be between 0 and 100".into(),
            ));
        }
    }

    let now = Utc::now().to_rfc3339();
    let existing = get_entry(pool, user_id, title_id).await?;

    let status = patch
        .status
        .clone()
        .or_else(|| existing.as_ref().map(|e| e.status.clone()))
        .unwrap_or_else(|| "planned".into());

    let rating = match patch.rating {
        Some(v) => v,
        None => existing.as_ref().and_then(|e| e.rating),
    };
    let favourite = patch
        .favourite
        .or_else(|| existing.as_ref().map(|e| e.favourite))
        .unwrap_or(false);
    let notes = match &patch.notes {
        Some(v) => v.clone(),
        None => existing.as_ref().and_then(|e| e.notes.clone()),
    };
    let notify = patch
        .notify
        .or_else(|| existing.as_ref().map(|e| e.notify))
        .unwrap_or(true);

    // Dates follow the status automatically; nobody wants to fill those in.
    let started_at = existing
        .as_ref()
        .and_then(|e| e.started_at.clone())
        .or_else(|| matches!(status.as_str(), "watching" | "completed").then(|| now.clone()));
    let finished_at = match status.as_str() {
        "completed" => existing
            .as_ref()
            .and_then(|e| e.finished_at.clone())
            .or_else(|| Some(now.clone())),
        _ => existing.as_ref().and_then(|e| e.finished_at.clone()),
    };

    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes, started_at,
                                    finished_at, rewatch_count, added_at, updated_at, notify)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, COALESCE((SELECT rewatch_count FROM library_entry WHERE user_id = ? AND title_id = ?), 0), ?, ?, ?)
         ON CONFLICT (user_id, title_id) DO UPDATE SET
            notify = excluded.notify,
            status = excluded.status,
            rating = excluded.rating,
            favourite = excluded.favourite,
            notes = excluded.notes,
            started_at = excluded.started_at,
            finished_at = excluded.finished_at,
            updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(&status)
    .bind(rating)
    .bind(favourite)
    .bind(&notes)
    .bind(&started_at)
    .bind(&finished_at)
    .bind(user_id)
    .bind(title_id)
    .bind(existing.as_ref().map(|e| e.added_at.clone()).unwrap_or_else(|| now.clone()))
    .bind(&now)
    .bind(notify)
    .execute(pool)
    .await?;

    get_entry(pool, user_id, title_id)
        .await?
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("entry vanished after write")))
}

pub async fn remove_entry(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM library_entry WHERE user_id = ? AND title_id = ?")
        .bind(user_id)
        .bind(title_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, FromRow)]
pub struct Progress {
    pub watched: i64,
    pub total: i64,
}

pub async fn progress(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<Progress> {
    // Specials (season 0) do not count towards finishing a show, and nor do
    // episodes that have not come out. The episode list is shared; only the
    // watches are mine.
    let row: (i64, i64) = sqlx::query_as(concat!(
        "SELECT
            (SELECT COUNT(DISTINCT e.id)
               FROM episode e
               JOIN watch_event w ON w.episode_id = e.id AND w.user_id = ?
              WHERE e.title_id = ? AND e.season_number > 0),
            (SELECT COUNT(*) FROM episode e WHERE e.title_id = ? AND e.season_number > 0 AND ",
        crate::released!(),
        ")"
    ))
    .bind(user_id)
    .bind(title_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;

    Ok(Progress {
        watched: row.0,
        total: row.1,
    })
}

/// The next episode of a show, and whether it is out yet.
#[derive(Debug, Serialize, Clone)]
pub struct NextEpisode {
    pub season_number: i64,
    pub episode_number: i64,
    pub name: Option<String>,
    pub still_path: Option<String>,
    pub air_date: Option<String>,
    pub aired: bool,
}

/// (season, episode, name, still_path, air_date, aired)
type NextRow = (
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
);

/// The episode to watch next: the first unwatched one after where you are,
/// not the first gap anywhere in the show. An import that ticks season 23 of
/// Pokémon but only the first two episodes of season 1 is at season 23.
///
/// Where you are is the most recently watched episode, by date. Undated
/// watches (imports, bulk ticks) say how far you got but not when, so the
/// furthest of them counts when it lies beyond the dated one. Only when
/// nothing after that is left unwatched does it fall back to the first gap.
pub async fn next_episode(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<NextEpisode>> {
    let latest: Option<(i64, i64)> = sqlx::query_as(
        "SELECT e.season_number, e.episode_number
           FROM watch_event w
           JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ? AND e.title_id = ? AND e.season_number > 0 AND w.undated = 0
          ORDER BY w.watched_at DESC, e.season_number DESC, e.episode_number DESC
          LIMIT 1",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_optional(pool)
    .await?;
    let furthest_undated: Option<(i64, i64)> = sqlx::query_as(
        "SELECT e.season_number, e.episode_number
           FROM watch_event w
           JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ? AND e.title_id = ? AND e.season_number > 0 AND w.undated = 1
          ORDER BY e.season_number DESC, e.episode_number DESC
          LIMIT 1",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_optional(pool)
    .await?;
    // Tuples compare season first, then episode.
    let here = latest.max(furthest_undated);

    // Out by `released!()`: an undated episode only once a later one aired.
    // One with no date that is not out (an announced season TMDB lists
    // early) is never next; one with a date to wait for is, as not aired.
    const NEXT: &str = concat!(
        "SELECT e.season_number, e.episode_number, e.name, e.still_path, e.air_date,
                CASE WHEN ",
        crate::released!(),
        " THEN 1 ELSE 0 END
           FROM episode e
          WHERE e.title_id = ?1
            AND e.season_number > 0
            AND (e.air_date IS NOT NULL OR ",
        crate::released!(),
        ")
            AND (e.season_number > ?3 OR (e.season_number = ?3 AND e.episode_number > ?4))
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ?2 AND w.episode_id = e.id)
          ORDER BY e.season_number, e.episode_number
          LIMIT 1"
    );
    let (season, episode) = here.unwrap_or((0, 0));
    let mut row: Option<NextRow> = sqlx::query_as(NEXT)
        .bind(title_id)
        .bind(user_id)
        .bind(season)
        .bind(episode)
        .fetch_optional(pool)
        .await?;
    if row.is_none() && here.is_some() {
        // Caught up from where you are: whatever was skipped on the way.
        row = sqlx::query_as(NEXT)
            .bind(title_id)
            .bind(user_id)
            .bind(0)
            .bind(0)
            .fetch_optional(pool)
            .await?;
    }

    Ok(row.map(
        |(season_number, episode_number, name, still_path, air_date, aired)| NextEpisode {
            season_number,
            episode_number,
            name,
            still_path,
            air_date,
            aired: aired == 1,
        },
    ))
}

pub async fn last_watched_at(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<String>> {
    // An aggregate always returns exactly one row, and that row holds NULL
    // when nothing matched -- so this has to be Option-inside-the-row, not
    // fetch_optional, or a title with no watches fails to decode. An undated
    // watch holds the day it was recorded, which is not when it was seen.
    let (last,): (Option<String>,) = sqlx::query_as(
        "SELECT MAX(watched_at) FROM watch_event
          WHERE user_id = ? AND title_id = ? AND undated = 0",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    Ok(last)
}

pub async fn build_item(
    pool: &SqlitePool,
    user_id: i64,
    title: TitleRow,
) -> AppResult<LibraryItem> {
    let entry = get_entry(pool, user_id, title.id).await?;
    let mut p = progress(pool, user_id, title.id).await?;
    if let Some(n) = local_progress(pool, user_id).await?.get(&title.id) {
        p.watched = *n;
    }
    let last = last_watched_at(pool, user_id, title.id).await?;
    let total = if p.total > 0 {
        p.total
    } else {
        title.episode_count.unwrap_or(0)
    };

    Ok(LibraryItem {
        genres: parse_genres(&title.genres_json),
        accent: parse_accent(title.accent_json.as_deref()),
        entry,
        watched_episodes: p.watched,
        total_episodes: total,
        last_watched_at: last,
        title,
    })
}

/// Episodes watched of each title that has no episode list (an AniList-only
/// title AniList lists no episodes for), as a count.
async fn local_progress(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<std::collections::HashMap<i64, i64>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT l.title_id, COALESCE(l.progress, 0)
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?
            AND EXISTS (SELECT 1 FROM title_external x
                         WHERE x.title_id = t.id AND x.provider = 'anilist' AND x.is_primary = 1)
            AND NOT EXISTS (SELECT 1 FROM episode e
                             WHERE e.title_id = t.id AND e.season_number > 0)",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Set the count of episodes watched of a title with no episode list.
pub async fn set_local_progress(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    progress: i64,
) -> AppResult<()> {
    sqlx::query("UPDATE library_entry SET progress = ? WHERE user_id = ? AND title_id = ?")
        .bind(progress.max(0))
        .bind(user_id)
        .bind(title_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A library entry with the title it belongs to, for loading them all at once.
#[derive(FromRow)]
struct EntryOfTitle {
    title_id: i64,
    #[sqlx(flatten)]
    entry: LibraryEntry,
}

/// `build_item` for a whole list, in four queries instead of three per title.
///
/// A library of six hundred titles was eighteen hundred queries and over a
/// second before the page could draw. The numbers are the same as
/// `build_item`'s: specials do not count, and the episode list is shared
/// while the watches are this account's.
pub async fn build_items(
    pool: &SqlitePool,
    user_id: i64,
    titles: Vec<TitleRow>,
) -> AppResult<Vec<LibraryItem>> {
    use std::collections::HashMap;

    let entries: HashMap<i64, LibraryEntry> = sqlx::query_as::<_, EntryOfTitle>(
        "SELECT title_id, status, rating, favourite, notes, started_at, finished_at,
                rewatch_count, added_at, updated_at, notify
           FROM library_entry WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.title_id, r.entry))
    .collect();

    let watched: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
        "SELECT e.title_id, COUNT(DISTINCT e.id)
           FROM episode e
           JOIN watch_event w ON w.episode_id = e.id AND w.user_id = ?
          WHERE e.season_number > 0
          GROUP BY e.title_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let totals: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(concat!(
        "SELECT e.title_id, COUNT(*)
           FROM episode e
          WHERE e.season_number > 0
            AND e.title_id IN (SELECT title_id FROM library_entry WHERE user_id = ?)
            AND ",
        crate::released!(),
        " GROUP BY e.title_id"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let last: HashMap<i64, Option<String>> = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT title_id, MAX(watched_at) FROM watch_event
          WHERE user_id = ? AND undated = 0 GROUP BY title_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    let counted = local_progress(pool, user_id).await?;

    Ok(titles
        .into_iter()
        .map(|title| {
            let total = totals.get(&title.id).copied().unwrap_or(0);
            let seen = counted.get(&title.id).or_else(|| watched.get(&title.id));
            LibraryItem {
                genres: parse_genres(&title.genres_json),
                accent: parse_accent(title.accent_json.as_deref()),
                entry: entries.get(&title.id).cloned(),
                watched_episodes: seen.copied().unwrap_or(0),
                total_episodes: if total > 0 {
                    total
                } else {
                    title.episode_count.unwrap_or(0)
                },
                last_watched_at: last.get(&title.id).cloned().flatten(),
                title,
            }
        })
        .collect())
}

/// What emptying a library took away.
#[derive(Debug, Default, PartialEq)]
pub struct Emptied {
    pub titles: i64,
    pub watches: i64,
    pub shelves: i64,
}

/// Take away one account's whole library: every title tracked, every watch,
/// every episode note and season rating, and every shelf. The account, its
/// settings, devices and profile stay, and so does its AniList connection.
/// One transaction: a half-emptied library is worse than either end of it.
pub async fn empty_library(pool: &SqlitePool, user_id: i64) -> AppResult<Emptied> {
    let mut tx = pool.begin().await?;
    // Counted before the deletes, so the answer says what actually went.
    let mut counted = [0i64; 3];
    for (n, sql) in counted.iter_mut().zip([
        "SELECT COUNT(*) FROM library_entry WHERE user_id = ?",
        "SELECT COUNT(*) FROM watch_event WHERE user_id = ?",
        "SELECT COUNT(*) FROM shelf WHERE user_id = ?",
    ]) {
        *n = sqlx::query_as::<_, (i64,)>(sql)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?
            .0;
    }
    let [titles, watches, shelves] = counted;
    // shelf_item goes with its shelf, on the foreign key.
    for statement in [
        "DELETE FROM watch_event WHERE user_id = ?",
        "DELETE FROM episode_state WHERE user_id = ?",
        "DELETE FROM season_rating WHERE user_id = ?",
        "DELETE FROM shelf WHERE user_id = ?",
        "DELETE FROM library_entry WHERE user_id = ?",
    ] {
        sqlx::query(statement)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Emptied {
        titles,
        watches,
        shelves,
    })
}
