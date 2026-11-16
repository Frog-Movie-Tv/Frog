//! One account's library: entries, progress, "up next", and moving an
//! account's data from one title to another. The titles themselves, shared
//! by everyone, are `crate::catalog`.

use crate::clock::Stamp;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};

use crate::catalog::{parse_accent, parse_genres, TitleRow, TITLE_COLUMNS};
use crate::error::{AppError, AppResult};
use crate::palette::Palette;
use crate::text;

mod merge;
mod people;
mod records;

pub use merge::*;
pub use people::*;
pub use records::*;

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
    /// Watched again this many times: a show's count, or a film's repeat watches.
    pub rewatches: i64,
    /// Who it was watched with, most recent first: anyone named on one of its
    /// watches, or the people a show is being watched with now. Only for the
    /// owner; `SharedItem` leaves it out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub watched_with: Vec<Mate>,
}

/// Someone a title was watched with, as a card names them.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Mate {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
}

/// Everyone each title was watched with, for one account, or for one of its
/// titles. Accounts switched off are left out, as everywhere people are named.
async fn mates(
    pool: &SqlitePool,
    user_id: i64,
    title_id: Option<i64>,
) -> AppResult<std::collections::HashMap<i64, Vec<Mate>>> {
    // (title, account, username, display name, latest)
    type MateRow = (i64, i64, String, Option<String>, String);
    let rows: Vec<MateRow> = sqlx::query_as(
        "SELECT x.title_id, u.id, u.username, u.display_name, MAX(x.at) AS latest
           FROM (SELECT w.title_id, ww.with_user_id AS who, w.watched_at AS at
                   FROM watch_with ww
                   JOIN watch_event w ON w.id = ww.watch_event_id AND w.user_id = ?1
                  WHERE ww.user_id = ?1 AND (?2 IS NULL OR w.title_id = ?2)
                 UNION ALL
                 SELECT title_id, with_user_id, created_at FROM watching_with
                  WHERE user_id = ?1 AND (?2 IS NULL OR title_id = ?2)) x
           JOIN app_user u ON u.id = x.who AND u.disabled = 0
          GROUP BY x.title_id, u.id
          ORDER BY latest DESC, u.username",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    let mut out: std::collections::HashMap<i64, Vec<Mate>> = std::collections::HashMap::new();
    for (title, id, username, display_name, _) in rows {
        out.entry(title).or_default().push(Mate {
            id,
            username,
            display_name,
        });
    }
    Ok(out)
}

/// What another account may see of a library entry. Named field by field on
/// purpose: notes are private, and so is anything added to `LibraryEntry`
/// until someone decides otherwise here.
#[derive(Debug, Clone, Serialize)]
pub struct SharedEntry {
    pub status: String,
    pub rating: Option<f64>,
    pub favourite: bool,
}

/// A title of someone else's library, as their profile shows it.
#[derive(Debug, Clone, Serialize)]
pub struct SharedItem {
    #[serde(flatten)]
    pub title: TitleRow,
    pub genres: Vec<String>,
    pub accent: Palette,
    pub entry: Option<SharedEntry>,
    pub watched_episodes: i64,
    pub total_episodes: i64,
    pub last_watched_at: Option<String>,
    pub rewatches: i64,
}

impl From<LibraryItem> for SharedItem {
    fn from(item: LibraryItem) -> Self {
        Self {
            title: item.title,
            genres: item.genres,
            accent: item.accent,
            entry: item.entry.map(|e| SharedEntry {
                status: e.status,
                rating: e.rating,
                favourite: e.favourite,
            }),
            watched_episodes: item.watched_episodes,
            total_episodes: item.total_episodes,
            last_watched_at: item.last_watched_at,
            rewatches: item.rewatches,
        }
    }
}

/// A rating as it is kept: a whole number from 0 to 100. Every score format
/// shows one exactly (8.7 is 87, half stars are tens), so whatever a person,
/// an import or AniList sends is held to that, and nothing is kept that no
/// format could show as it is.
pub fn whole_rating(rating: f64) -> f64 {
    rating.clamp(0.0, 100.0).round()
}

/// A field that was sent, even as null, against one that was left out:
/// `Some(None)` clears it, `None` leaves it as it is. Serde on its own reads
/// a null `Option<Option<T>>` as left out, so Clear never cleared anything.
fn sent<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryPatch {
    pub status: Option<String>,
    /// 0..100 internally; the UI converts to whichever score format is set.
    #[serde(default, deserialize_with = "sent")]
    pub rating: Option<Option<f64>>,
    pub favourite: Option<bool>,
    #[serde(default, deserialize_with = "sent")]
    pub notes: Option<Option<String>>,
    pub notify: Option<bool>,
}

/// A title of a library, as much as a bulk action needs of it.
#[derive(Debug, Clone, FromRow)]
pub struct TrackedTitle {
    pub id: i64,
    pub kind: String,
    pub release_date: Option<String>,
}

/// How many ids go in one `IN (…)`. SQLite takes a limited number of variables.
const IDS_AT_ONCE: usize = 500;

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

    let now = Utc::now().stamp();
    let existing = get_entry(pool, user_id, title_id).await?;

    let status = patch
        .status
        .clone()
        .or_else(|| existing.as_ref().map(|e| e.status.clone()))
        .unwrap_or_else(|| "planned".into());

    let rating = match patch.rating {
        Some(v) => v.map(whole_rating),
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
    remove_entries(pool, user_id, &[title_id]).await
}

/// Which of these titles are in this account's library. An id it does not
/// hold, or another account's, is simply not among them.
pub async fn tracked_titles(
    pool: &SqlitePool,
    user_id: i64,
    title_ids: &[i64],
) -> AppResult<Vec<TrackedTitle>> {
    let mut out = Vec::with_capacity(title_ids.len());
    for ids in title_ids.chunks(IDS_AT_ONCE) {
        let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT t.id, t.kind, t.release_date
               FROM library_entry l JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ",
        );
        q.push_bind(user_id).push(" AND l.title_id IN (");
        let mut list = q.separated(", ");
        for id in ids {
            list.push_bind(*id);
        }
        q.push(")");
        out.extend(q.build_query_as::<TrackedTitle>().fetch_all(pool).await?);
    }
    Ok(out)
}

/// A film's first watch, unless it has one: finishing it again adds nothing.
/// `how` is `tick` for one film finished, `bulk` for many marked at once.
pub async fn add_first_film_watch(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    watched_at: &str,
    undated: bool,
    how: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated, logged_at, logged_how)
         SELECT ?1, ?2, NULL, ?3, 0, 'manual', ?4, ?5, ?6
          WHERE NOT EXISTS (SELECT 1 FROM watch_event WHERE user_id = ?1 AND title_id = ?2)",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(watched_at)
    .bind(undated)
    .bind(Utc::now().stamp())
    .bind(how)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, FromRow)]
pub struct Progress {
    pub watched: i64,
    pub total: i64,
}

/// One account's time with a title, as its page tells it.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct History {
    /// The day it was started: the first dated watch, or the day the entry
    /// says (an import or AniList may know it), whichever is earlier.
    pub started: Option<String>,
    /// The day it was finished, once it is completed: the last dated watch,
    /// or the day the entry says.
    pub finished: Option<String>,
    /// How many times it was watched again after the first.
    pub rewatches: i64,
}

/// The day of a stored time, `YYYY-MM-DD`.
fn day_of(at: &Option<String>) -> Option<String> {
    at.as_deref()
        .filter(|a| a.len() >= 10)
        .map(|a| a[..10].to_string())
}

/// Put the pieces of a history together: the decision, apart from the reads.
/// `passes` is how many times every released episode (or the film) was seen.
pub fn history_of(
    entry: &LibraryEntry,
    first_watch: &Option<String>,
    last_watch: &Option<String>,
    passes: i64,
) -> History {
    let started = match (day_of(first_watch), day_of(&entry.started_at)) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let finished = (entry.status == "completed")
        .then(|| day_of(last_watch).or_else(|| day_of(&entry.finished_at)))
        .flatten();
    History {
        started,
        finished,
        rewatches: entry.rewatch_count.max(passes - 1).max(0),
    }
}

/// (first dated watch, last dated watch)
type SpanRow = (Option<String>, Option<String>);

/// A title's history for one account, or nothing when it is not in the library.
pub async fn history(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    kind: &str,
) -> AppResult<Option<History>> {
    let Some(entry) = get_entry(pool, user_id, title_id).await? else {
        return Ok(None);
    };
    let (first, last): SpanRow = sqlx::query_as(
        "SELECT MIN(watched_at), MAX(watched_at) FROM watch_event
          WHERE user_id = ? AND title_id = ? AND undated = 0",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    let passes: (Option<i64>,) = if kind == "movie" {
        sqlx::query_as(
            "SELECT COUNT(*) FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_one(pool)
        .await?
    } else {
        // Seen in full as often as its least-seen released episode.
        sqlx::query_as(concat!(
            "SELECT MIN((SELECT COUNT(*) FROM watch_event w
                          WHERE w.user_id = ? AND w.episode_id = e.id))
               FROM episode e
              WHERE e.title_id = ? AND e.season_number > 0 AND ",
            crate::released!()
        ))
        .bind(user_id)
        .bind(title_id)
        .fetch_one(pool)
        .await?
    };
    Ok(Some(history_of(
        &entry,
        &first,
        &last,
        passes.0.unwrap_or(0),
    )))
}

/// Start and finish days another place knows (AniList, an import), and how
/// often it was watched again. They are kept as plain days, which is what
/// tells them from the moment Frog stamps when a status changes: a known day
/// always replaces a stamp, and between known days the earliest start and the
/// latest finish win, so a show AniList keeps as several entries starts with
/// its first and finishes with its last. Nothing given changes nothing.
/// A plain yyyy-mm-dd day from whatever a tracker sent, or nothing.
fn day(d: Option<&str>) -> Option<String> {
    d.filter(|d| d.len() >= 10 && chrono::NaiveDate::parse_from_str(&d[..10], "%Y-%m-%d").is_ok())
        .map(|d| d[..10].to_string())
}

/// The days AniList has for a title, which win over Frog's: AniList is where
/// the list is kept. A day AniList does not have leaves Frog's; the rewatch
/// count only goes up, so one logged in Frog is not lost to a sync.
pub async fn replace_known_dates(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    started: Option<&str>,
    finished: Option<&str>,
    rewatches: i64,
) -> AppResult<()> {
    let (started, finished) = (day(started), day(finished));
    if started.is_none() && finished.is_none() && rewatches <= 0 {
        return Ok(());
    }
    sqlx::query(
        "UPDATE library_entry
            SET started_at = COALESCE(?1, started_at),
                finished_at = COALESCE(?2, finished_at),
                rewatch_count = MAX(rewatch_count, ?3)
          WHERE user_id = ?4 AND title_id = ?5",
    )
    .bind(started)
    .bind(finished)
    .bind(rewatches.max(0))
    .bind(user_id)
    .bind(title_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_known_dates(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    started: Option<&str>,
    finished: Option<&str>,
    rewatches: i64,
) -> AppResult<()> {
    let (started, finished) = (day(started), day(finished));
    if started.is_none() && finished.is_none() && rewatches <= 0 {
        return Ok(());
    }
    sqlx::query(
        "UPDATE library_entry
            SET started_at = CASE
                    WHEN ?1 IS NULL THEN started_at
                    WHEN started_at IS NULL OR length(started_at) <> 10 OR ?1 < started_at THEN ?1
                    ELSE started_at END,
                finished_at = CASE
                    WHEN ?2 IS NULL THEN finished_at
                    WHEN finished_at IS NULL OR length(finished_at) <> 10 OR ?2 > finished_at THEN ?2
                    ELSE finished_at END,
                rewatch_count = MAX(rewatch_count, ?3)
          WHERE user_id = ?4 AND title_id = ?5",
    )
    .bind(started)
    .bind(finished)
    .bind(rewatches.max(0))
    .bind(user_id)
    .bind(title_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Watched it again. A film gets another watch on that day; a show, whose
/// rewatch is not one episode, is counted on its entry, with the day as when
/// it was last finished.
pub async fn add_rewatch(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    kind: &str,
    watched_at: &str,
    undated: bool,
) -> AppResult<()> {
    if kind == "movie" {
        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                      source, undated)
             VALUES (?, ?, NULL, ?, 1, 'manual', ?)",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(watched_at)
        .bind(undated)
        .execute(pool)
        .await?;
    }
    sqlx::query(
        "UPDATE library_entry
            SET rewatch_count = rewatch_count + ?,
                finished_at = CASE WHEN ? THEN finished_at ELSE ? END,
                updated_at = ?
          WHERE user_id = ? AND title_id = ?",
    )
    // A film's rewatches are its watches, counted where they are.
    .bind(if kind == "movie" { 0 } else { 1 })
    .bind(undated)
    .bind(watched_at)
    .bind(Utc::now().stamp())
    .bind(user_id)
    .bind(title_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Someone else on this Frog with a title in their library, as a title page
/// shows them: what they made of it, never their notes.
#[derive(Debug, Clone, Serialize)]
pub struct OtherWatcher {
    #[serde(flatten)]
    pub who: Shown,
    pub status: String,
    pub rating: Option<f64>,
    pub favourite: bool,
}

/// Everyone the viewer may see who has this title in their library.
pub async fn others_with(
    pool: &SqlitePool,
    viewer: i64,
    title_id: i64,
) -> AppResult<Vec<OtherWatcher>> {
    let mut out = Vec::new();
    for person in shown_to(pool, viewer).await? {
        let row: Option<(String, Option<f64>, bool)> = sqlx::query_as(
            "SELECT status, rating, favourite FROM library_entry
              WHERE user_id = ? AND title_id = ?",
        )
        .bind(person.id)
        .bind(title_id)
        .fetch_optional(pool)
        .await?;
        if let Some((status, rating, favourite)) = row {
            out.push(OtherWatcher {
                who: person.shown,
                status,
                rating,
                favourite,
            });
        }
    }
    // Who finished it first, then who is on it, then who means to.
    let order = |s: &str| {
        ["completed", "watching", "paused", "planned", "dropped"]
            .iter()
            .position(|x| *x == s)
            .unwrap_or(9)
    };
    out.sort_by_key(|o| order(&o.status));
    Ok(out)
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
    /// The moment, when AniList or TVmaze knows the time.
    pub air_at: Option<String>,
    pub aired: bool,
}

/// (season, episode, name, still_path, air_date, aired, air_at)
type NextRow = (
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
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
        " THEN 1 ELSE 0 END, e.air_at
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
        |(season_number, episode_number, name, still_path, air_date, aired, air_at)| NextEpisode {
            season_number,
            episode_number,
            name,
            still_path,
            air_date,
            air_at,
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
    // watch holds the day it was recorded, which is not when it was seen. A
    // manga was last read when a chapter was.
    let (last,): (Option<String>,) = sqlx::query_as(
        "SELECT MAX(at) FROM (
            SELECT MAX(watched_at) AS at FROM watch_event
             WHERE user_id = ?1 AND title_id = ?2 AND undated = 0
            UNION ALL
            SELECT MAX(read_at) FROM chapter_read WHERE user_id = ?1 AND title_id = ?2)",
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
    let (film_again,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watch_event
          WHERE user_id = ? AND title_id = ? AND episode_id IS NULL AND is_rewatch = 1",
    )
    .bind(user_id)
    .bind(title.id)
    .fetch_one(pool)
    .await?;
    let rewatches = entry.as_ref().map_or(0, |e| e.rewatch_count) + film_again;
    let watched_with = mates(pool, user_id, Some(title.id))
        .await?
        .remove(&title.id)
        .unwrap_or_default();
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
        rewatches,
        watched_with,
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

/// Whether a title's progress is a count rather than its ticked episodes: a
/// title from AniList with no episode list, which every manga is.
pub async fn is_counted(pool: &SqlitePool, title_id: i64) -> AppResult<bool> {
    let (counted,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM title_external x
                         WHERE x.title_id = ?1 AND x.provider = 'anilist' AND x.is_primary = 1)
            AND NOT EXISTS (SELECT 1 FROM episode e
                             WHERE e.title_id = ?1 AND e.season_number > 0)",
    )
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    Ok(counted)
}

/// The count kept for a title followed as one (`is_counted`), when this
/// account has one.
pub async fn counted_progress(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<i64>> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT progress FROM library_entry WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
}

/// Set the count of episodes watched of a title with no episode list, or of
/// a manga's chapters read.
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

/// `build_item` for a whole list, in five queries instead of three per title.
///
/// A library of six hundred titles was eighteen hundred queries and over a
/// second before the page could draw.
/// What the library page narrows by.
#[derive(Debug, Default)]
pub struct LibraryFilter {
    pub status: Option<String>,
    pub kind: Option<String>,
    pub genre: Option<String>,
    /// A name, typed by a person.
    pub term: Option<String>,
    pub favourite: bool,
    /// Only what was watched more than once.
    pub rewatched: bool,
}

impl LibraryFilter {
    /// Everything but the status, which `library_counts` must not narrow by.
    fn push_without_status(&self, q: &mut QueryBuilder<Sqlite>) {
        if let Some(kind) = &self.kind {
            q.push(" AND t.kind = ").push_bind(kind.clone());
        }
        if let Some(genre) = &self.genre {
            q.push(" AND t.genres_json LIKE ")
                .push_bind(format!("%\"{genre}\"%"));
        }
        if let Some(term) = &self.term {
            // Folded on both sides, so neither case nor accents decide a match,
            // and typed literally: a % or _ in a title is a character, not a
            // wildcard. A row written by an older binary has no search_name
            // until the next start fills it; until then its plain name matches.
            let folded = text::fold(term.trim())
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            q.push(
                " AND COALESCE(t.search_name, lower(t.name || char(10) || COALESCE(t.original_name, '')))
                      LIKE ",
            )
            .push_bind(format!("%{folded}%"))
            .push(" ESCAPE '\\'");
        }
        if self.favourite {
            q.push(" AND l.favourite = 1");
        }
        if self.rewatched {
            q.push(
                " AND (l.rewatch_count > 0 OR EXISTS (
                        SELECT 1 FROM watch_event w
                         WHERE w.user_id = l.user_id AND w.title_id = l.title_id
                           AND w.episode_id IS NULL AND w.is_rewatch = 1))",
            );
        }
    }
}

/// Which slice of a list, in which order.
pub struct Page<'a> {
    pub sort: Option<&'a str>,
    pub order: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// Map a sort key to SQL. An allowlist, never interpolation of user input.
///
/// `user_id` is written into the sort subquery rather than bound, because a
/// placeholder inside ORDER BY would have to be bound between the filters and
/// the paging ones and silently shift if either changed. It comes from the
/// session as an `i64`, so there is nothing in it to escape.
pub fn order_clause(sort: Option<&str>, order: Option<&str>, user_id: i64) -> String {
    let dir = match order {
        Some("asc") => "ASC",
        _ => "DESC",
    };
    let column: String = match sort.unwrap_or("added") {
        "name" => "t.name COLLATE NOCASE".into(),
        "rating" => "l.rating".into(),
        "release" => "COALESCE(t.release_date, '')".into(),
        "updated" => "l.updated_at".into(),
        "last_watched" => format!(
            "COALESCE((SELECT MAX(w.watched_at) FROM watch_event w
                WHERE w.title_id = t.id AND w.user_id = {user_id} AND w.undated = 0),
              (SELECT MAX(r.read_at) FROM chapter_read r
                WHERE r.title_id = t.id AND r.user_id = {user_id}))"
        ),
        _ => "l.added_at".into(),
    };
    // NULLs last in either direction: an unrated title should never head the list.
    format!("ORDER BY ({column}) IS NULL, {column} {dir}, t.name COLLATE NOCASE ASC")
}

/// The titles of one account's library that pass a filter.
pub async fn library_titles(
    pool: &SqlitePool,
    user_id: i64,
    filter: &LibraryFilter,
    page: &Page<'_>,
) -> AppResult<Vec<TitleRow>> {
    let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(format!(
        "SELECT {TITLE_COLUMNS}
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = "
    ));
    q.push_bind(user_id);
    if let Some(status) = &filter.status {
        q.push(" AND l.status = ").push_bind(status.clone());
    }
    filter.push_without_status(&mut q);
    q.push(" ");
    q.push(order_clause(page.sort, page.order, user_id));
    q.push(" LIMIT ").push_bind(page.limit);
    q.push(" OFFSET ").push_bind(page.offset);
    Ok(q.build_query_as().fetch_all(pool).await?)
}

/// How many titles each status would show with the rest of the filter as it
/// is, so the number on a status is what choosing it gives. The status itself
/// does not narrow them: choosing one leaves the others there to choose.
pub async fn library_counts(
    pool: &SqlitePool,
    user_id: i64,
    filter: &LibraryFilter,
) -> AppResult<Vec<(String, i64)>> {
    let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT l.status, COUNT(*)
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ",
    );
    q.push_bind(user_id);
    filter.push_without_status(&mut q);
    q.push(" GROUP BY l.status");
    Ok(q.build_query_as().fetch_all(pool).await?)
}

/// Every genre in one account's library, so a filter never hides its own options.
pub async fn library_genres(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT g.value
           FROM library_entry l
           JOIN title t ON t.id = l.title_id, json_each(t.genres_json) g
          WHERE l.user_id = ?
          ORDER BY g.value COLLATE NOCASE",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(g,)| g).collect())
}

pub async fn build_items(
    pool: &SqlitePool,
    user_id: i64,
    titles: Vec<TitleRow>,
) -> AppResult<Vec<LibraryItem>> {
    let facts = ItemFacts::load(pool, user_id).await?;
    Ok(titles.into_iter().map(|t| facts.item(t)).collect())
}

/// What one account's items are built from, read once for the whole library.
/// Load it once and call `item` for each title when a page draws several
/// lists, as Home does. The numbers are the same as `build_item`'s: specials
/// do not count, and the episode list is shared while the watches are this
/// account's.
pub struct ItemFacts {
    entries: std::collections::HashMap<i64, LibraryEntry>,
    watched: std::collections::HashMap<i64, i64>,
    totals: std::collections::HashMap<i64, i64>,
    last: std::collections::HashMap<i64, Option<String>>,
    counted: std::collections::HashMap<i64, i64>,
    /// A film's repeat watches; a show's rewatches are counted on its entry.
    film_again: std::collections::HashMap<i64, i64>,
    with: std::collections::HashMap<i64, Vec<Mate>>,
}

impl ItemFacts {
    pub async fn load(pool: &SqlitePool, user_id: i64) -> AppResult<Self> {
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

        // A title stays on a hand-picked shelf after it leaves the library.
        let totals: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(concat!(
            "SELECT e.title_id, COUNT(*)
               FROM episode e
              WHERE e.season_number > 0
                AND e.title_id IN (SELECT title_id FROM library_entry WHERE user_id = ?1
                                   UNION
                                   SELECT i.title_id FROM shelf_item i
                                     JOIN shelf s ON s.id = i.shelf_id
                                    WHERE s.user_id = ?1)
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
            "SELECT title_id, MAX(at) FROM (
                SELECT title_id, watched_at AS at FROM watch_event
                 WHERE user_id = ?1 AND undated = 0
                UNION ALL
                SELECT title_id, read_at FROM chapter_read WHERE user_id = ?1)
              GROUP BY title_id",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        let counted = local_progress(pool, user_id).await?;
        let film_again: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
            "SELECT title_id, COUNT(*) FROM watch_event
              WHERE user_id = ? AND episode_id IS NULL AND is_rewatch = 1
              GROUP BY title_id",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();

        let with = mates(pool, user_id, None).await?;

        Ok(ItemFacts {
            entries,
            watched,
            totals,
            last,
            counted,
            film_again,
            with,
        })
    }

    pub fn item(&self, title: TitleRow) -> LibraryItem {
        let total = self.totals.get(&title.id).copied().unwrap_or(0);
        let seen = self
            .counted
            .get(&title.id)
            .or_else(|| self.watched.get(&title.id));
        let entry = self.entries.get(&title.id).cloned();
        let rewatches = entry.as_ref().map_or(0, |e| e.rewatch_count)
            + self.film_again.get(&title.id).copied().unwrap_or(0);
        LibraryItem {
            genres: parse_genres(&title.genres_json),
            accent: parse_accent(title.accent_json.as_deref()),
            entry,
            rewatches,
            watched_episodes: seen.copied().unwrap_or(0),
            total_episodes: if total > 0 {
                total
            } else {
                title.episode_count.unwrap_or(0)
            },
            last_watched_at: self.last.get(&title.id).cloned().flatten(),
            watched_with: self.with.get(&title.id).cloned().unwrap_or_default(),
            title,
        }
    }
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

#[cfg(test)]
mod order_tests {
    use super::{history_of, order_clause, whole_rating, LibraryEntry};

    #[test]
    fn a_rating_is_kept_whole_and_on_the_scale() {
        assert_eq!(whole_rating(87.0), 87.0);
        assert_eq!(whole_rating(87.4), 87.0);
        assert_eq!(whole_rating(87.5), 88.0, "8.75 out of 10 reads as 8.8");
        assert_eq!(whole_rating(120.0), 100.0);
        assert_eq!(whole_rating(-3.0), 0.0);
    }

    #[test]
    fn sort_keys_are_allowlisted() {
        assert!(order_clause(Some("name"), Some("asc"), 1).contains("t.name"));
        assert!(order_clause(Some("rating"), None, 1).contains("l.rating"));
        // Anything unknown falls back to added_at rather than reaching SQL.
        let injected = order_clause(Some("name; DROP TABLE title"), None, 1);
        assert!(injected.contains("l.added_at"));
        assert!(!injected.contains("DROP"));
    }

    #[test]
    fn sorting_by_last_watched_only_counts_my_watches() {
        let clause = order_clause(Some("last_watched"), None, 7);
        assert!(clause.contains("w.user_id = 7"));
    }

    fn entry(status: &str, started: Option<&str>, finished: Option<&str>) -> LibraryEntry {
        LibraryEntry {
            status: status.into(),
            rating: None,
            favourite: false,
            notes: None,
            started_at: started.map(str::to_string),
            finished_at: finished.map(str::to_string),
            rewatch_count: 0,
            added_at: String::new(),
            updated_at: String::new(),
            notify: true,
        }
    }

    #[test]
    fn a_history_goes_by_the_watches_and_the_dates_an_import_gave() {
        let some = |s: &str| Some(s.to_string());
        // Imported today, watched in 2019: the watches say when.
        let e = entry(
            "completed",
            Some("2026-11-02T20:00:00Z"),
            Some("2026-11-02T20:00:00Z"),
        );
        let h = history_of(
            &e,
            &some("2019-03-01T20:00:00Z"),
            &some("2019-04-10T20:00:00Z"),
            1,
        );
        assert_eq!(h.started.as_deref(), Some("2019-03-01"));
        assert_eq!(h.finished.as_deref(), Some("2019-04-10"));
        assert_eq!(h.rewatches, 0);

        // AniList knew the start, and nothing was dated.
        let e = entry("completed", Some("2018-01-05"), Some("2018-02-01"));
        let h = history_of(&e, &None, &None, 1);
        assert_eq!(h.started.as_deref(), Some("2018-01-05"));
        assert_eq!(h.finished.as_deref(), Some("2018-02-01"));

        // Not finished yet: no end day. Seen twice through: one rewatch.
        let e = entry("watching", Some("2025-01-01"), None);
        let h = history_of(&e, &None, &some("2025-02-01T20:00:00Z"), 0);
        assert_eq!(h.finished, None);
        assert_eq!(
            history_of(&entry("completed", None, None), &None, &None, 3).rewatches,
            2
        );
    }
}

/// (season, episode, my rating, my notes, number of watches,
///  latest watch time, whether that watch is undated, air moment)
pub type EpisodeStateRow = (
    i64,
    i64,
    Option<f64>,
    Option<String>,
    i64,
    Option<String>,
    Option<bool>,
    Option<String>,
);

/// One season's episodes as one account has them: ratings, notes, watches.
pub async fn season_states(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    season: i64,
) -> AppResult<Vec<EpisodeStateRow>> {
    Ok(sqlx::query_as(
        "SELECT e.season_number,
                e.episode_number,
                CAST(NULL AS REAL),
                s.notes,
                (SELECT COUNT(*) FROM watch_event w
                  WHERE w.user_id = ?1 AND w.episode_id = e.id),
                (SELECT w.watched_at FROM watch_event w
                  WHERE w.user_id = ?1 AND w.episode_id = e.id
                  ORDER BY w.watched_at DESC LIMIT 1),
                (SELECT w.undated FROM watch_event w
                  WHERE w.user_id = ?1 AND w.episode_id = e.id
                  ORDER BY w.watched_at DESC LIMIT 1),
                e.air_at
           FROM episode e
           LEFT JOIN episode_state s ON s.episode_id = e.id AND s.user_id = ?1
          WHERE e.title_id = ?2 AND e.season_number = ?3",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(season)
    .fetch_all(pool)
    .await?)
}

/// The library's header figures.
pub struct LibraryFigures {
    pub by_status: Vec<(String, i64)>,
    pub by_kind: Vec<(String, i64)>,
    /// Watches of titles still in the library, as on the stats page.
    pub films_watched: i64,
    pub episodes_watched: i64,
    pub minutes: Option<f64>,
    pub mean_rating: Option<f64>,
}

pub async fn library_figures(pool: &SqlitePool, user_id: i64) -> AppResult<LibraryFigures> {
    let by_status: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM library_entry WHERE user_id = ? GROUP BY status",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let by_kind: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.kind, COUNT(*) FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?
         GROUP BY t.kind",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let (films_watched, episodes_watched): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(w.episode_id IS NULL), 0), COALESCE(SUM(w.episode_id IS NOT NULL), 0)
           FROM watch_event w
           JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
          WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    // Runtime is per-episode for shows, so this is an estimate. SQLite types
    // SUM() by its operands, so CAST pins it to a REAL.
    let (minutes,): (Option<f64>,) = sqlx::query_as(
        "SELECT CAST(SUM(COALESCE(e.runtime, t.runtime, 0)) AS REAL)
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let (mean_rating,): (Option<f64>,) = sqlx::query_as(
        "SELECT AVG(rating) FROM library_entry WHERE user_id = ? AND rating IS NOT NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(LibraryFigures {
        by_status,
        by_kind,
        films_watched,
        episodes_watched,
        minutes,
        mean_rating,
    })
}

/// Each library title's status and primary provider id:
/// (title id, status, provider, provider's id).
pub async fn library_sources(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<Vec<(i64, String, Option<String>, Option<String>)>> {
    Ok(sqlx::query_as(
        "SELECT l.title_id, l.status, x.provider, x.external_id
           FROM library_entry l
           LEFT JOIN title_external x ON x.title_id = l.title_id AND x.is_primary = 1
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

/// Every provider id of every library title, with its status:
/// (provider, provider's id, title id, status).
pub async fn library_ids(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<Vec<(String, String, i64, String)>> {
    Ok(sqlx::query_as(
        "SELECT x.provider, x.external_id, l.title_id, l.status
           FROM library_entry l
           JOIN title_external x ON x.title_id = l.title_id
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

/// (id, kind, primary provider, its id, name, poster, backdrop, release date,
///  vote, genres)
pub type KnownRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<f64>,
    String,
);

/// Titles this Frog knows whose folded name, or a word of it, starts with
/// `pattern` or `alternative`; the account's own first.
pub async fn known_by_name(
    pool: &SqlitePool,
    user_id: i64,
    pattern: &str,
    alternative: &str,
) -> AppResult<Vec<KnownRow>> {
    Ok(sqlx::query_as(
        "SELECT t.id, t.kind, x.provider, x.external_id, t.name, t.poster_path,
                t.backdrop_path, t.release_date, t.tmdb_vote, t.genres_json
           FROM title t
           LEFT JOIN title_external x ON x.title_id = t.id AND x.is_primary = 1
          WHERE t.merged_into IS NULL
            AND (t.search_name LIKE ?1 || '%' OR t.search_name LIKE '% ' || ?1 || '%'
                 OR t.search_name LIKE '%' || char(10) || ?1 || '%'
                 OR t.search_name LIKE ?3 || '%' OR t.search_name LIKE '% ' || ?3 || '%')
          ORDER BY EXISTS (SELECT 1 FROM library_entry l
                            WHERE l.title_id = t.id AND l.user_id = ?2) DESC,
                   t.tmdb_vote DESC
          LIMIT 8",
    )
    .bind(pattern)
    .bind(user_id)
    .bind(alternative)
    .fetch_all(pool)
    .await?)
}

/// One watch of an episode as an untick cleared it.
#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct KeptWatch {
    pub watched_at: String,
    pub undated: bool,
    pub is_rewatch: bool,
    pub source: String,
    #[serde(default)]
    pub logged_at: Option<String>,
    #[serde(default)]
    pub logged_how: Option<String>,
}

/// An episode of a title by its numbers.
pub async fn episode_id(
    pool: &SqlitePool,
    title_id: i64,
    season: i64,
    episode: i64,
) -> AppResult<i64> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM episode WHERE title_id = ? AND season_number = ? AND episode_number = ?",
    )
    .bind(title_id)
    .bind(season)
    .bind(episode)
    .fetch_optional(pool)
    .await?;
    row.map(|r| r.0)
        .ok_or_else(|| AppError::NotFound(format!("no S{season}E{episode} for this show")))
}

/// Put back a watch an untick cleared, unless one at that moment is there.
pub async fn restore_watch(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    episode_id: i64,
    w: &KeptWatch,
    source: &str,
) -> AppResult<()> {
    let (logged_at, logged_how) =
        crate::awards::eligible::carried(w.logged_at.as_deref(), w.logged_how.as_deref());
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated, logged_at, logged_how)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
          WHERE NOT EXISTS (SELECT 1 FROM watch_event
                             WHERE user_id = ?1 AND episode_id = ?3 AND watched_at = ?4)",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(episode_id)
    .bind(&w.watched_at)
    .bind(w.is_rewatch)
    .bind(source)
    .bind(w.undated)
    .bind(logged_at)
    .bind(logged_how)
    .execute(pool)
    .await?;
    Ok(())
}

/// A watch ticked by hand; a rewatch when this account has seen it before.
pub async fn add_episode_watch(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    episode_id: i64,
    when: &str,
    undated: bool,
) -> AppResult<()> {
    add_watch(
        pool,
        user_id,
        title_id,
        Some(episode_id),
        when,
        undated,
        "manual",
    )
    .await?;
    Ok(())
}

/// One watch of an episode, or of a film when `episode_id` is None, from
/// `source`; a rewatch when this account has seen it before. Gives the
/// watch's id, so whoever added it can take exactly that one back.
pub async fn add_watch(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    episode_id: Option<i64>,
    when: &str,
    undated: bool,
    source: &str,
) -> AppResult<i64> {
    let (seen_before,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watch_event
          WHERE user_id = ? AND title_id = ? AND episode_id IS ?",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(episode_id)
    .fetch_one(pool)
    .await?;
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated)
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(episode_id)
    .bind(when)
    .bind(i64::from(seen_before > 0))
    .bind(source)
    .bind(i64::from(undated))
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Take back one watch by its id. False when the account has no such watch,
/// because it was unticked in the meantime.
pub async fn remove_watch(pool: &SqlitePool, user_id: i64, watch_id: i64) -> AppResult<bool> {
    let done = sqlx::query("DELETE FROM watch_event WHERE id = ? AND user_id = ?")
        .bind(watch_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Whether the account has any watch of a title, episodes and film alike.
pub async fn has_watches(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<bool> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM watch_event WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_one(pool)
            .await?;
    Ok(n > 0)
}

/// Clear every watch of an episode, handing them back so undo can put back
/// the rewatches as well as the last.
pub async fn clear_episode_watches(
    pool: &SqlitePool,
    user_id: i64,
    episode_id: i64,
) -> AppResult<Vec<KeptWatch>> {
    let cleared = sqlx::query_as(
        "SELECT watched_at, undated, is_rewatch, source, logged_at, logged_how FROM watch_event
          WHERE user_id = ? AND episode_id = ? ORDER BY watched_at",
    )
    .bind(user_id)
    .bind(episode_id)
    .fetch_all(pool)
    .await?;
    sqlx::query("DELETE FROM watch_event WHERE user_id = ? AND episode_id = ?")
        .bind(user_id)
        .bind(episode_id)
        .execute(pool)
        .await?;
    Ok(cleared)
}

/// Clear one season's watches, or the whole show's but its specials.
pub async fn clear_season_watches(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    season: Option<i64>,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM watch_event
          WHERE user_id = ?3
            AND episode_id IN (
                SELECT id FROM episode
                 WHERE title_id = ?1
                   AND (?2 IS NOT NULL OR season_number > 0)
                   AND (?2 IS NULL OR season_number = ?2))",
    )
    .bind(title_id)
    .bind(season)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_episode_notes(
    pool: &SqlitePool,
    user_id: i64,
    episode_id: i64,
    notes: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO episode_state (user_id, episode_id, notes, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, episode_id) DO UPDATE SET
            notes = excluded.notes,
            updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(episode_id)
    .bind(notes)
    .bind(Utc::now().stamp())
    .execute(pool)
    .await?;
    Ok(())
}

/// Counts by name as one JSON object: `{ "watching": 3, ... }`.
pub fn to_map(rows: Vec<(String, i64)>) -> serde_json::Value {
    serde_json::Value::Object(
        rows.into_iter()
            .map(|(k, v)| (k, serde_json::json!(v)))
            .collect(),
    )
}
