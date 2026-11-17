//! Every statement on `event`, `event_title`, `event_hidden` and
//! `event_seen`, and how far one account is with a title on a list. `event`
//! and `event_title` are shared, like a title; hiding and the premiere
//! played are each account's own and always name it.

use crate::clock::Stamp;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;

use super::{show_standing, Standing};
use crate::awards::eligible::parse;
use crate::error::AppResult;

/// What a film or an episode stands in for when TMDB has no runtime.
pub const FILM_MINUTES: i64 = 120;
pub const EPISODE_MINUTES: i64 = 45;

#[derive(Debug, Clone, FromRow)]
pub struct EventRow {
    pub id: i64,
    pub slug: String,
    pub title_id: Option<i64>,
    pub release_day: Option<String>,
    pub released_checked_at: Option<String>,
    pub state: String,
    pub pinned_at: Option<String>,
}

impl EventRow {
    pub fn on(&self) -> bool {
        self.state == "on"
    }
    pub fn pinned(&self) -> Option<DateTime<Utc>> {
        self.pinned_at.as_deref().and_then(parse)
    }
    pub fn day(&self) -> Option<NaiveDate> {
        self.release_day
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    }
}

const EVENT_COLUMNS: &str =
    "id, slug, title_id, release_day, released_checked_at, state, pinned_at";

pub async fn all(pool: &SqlitePool) -> AppResult<Vec<EventRow>> {
    Ok(
        sqlx::query_as(&format!("SELECT {EVENT_COLUMNS} FROM event ORDER BY id"))
            .fetch_all(pool)
            .await?,
    )
}

pub async fn by_slug(pool: &SqlitePool, slug: &str) -> AppResult<Option<EventRow>> {
    Ok(
        sqlx::query_as(&format!("SELECT {EVENT_COLUMNS} FROM event WHERE slug = ?"))
            .bind(slug)
            .fetch_optional(pool)
            .await?,
    )
}

/// The row for a pack, made when it is new. The administrator's switch and
/// pinned moment stay as they were.
pub async fn upsert(pool: &SqlitePool, slug: &str, title_id: Option<i64>) -> AppResult<i64> {
    let now = Utc::now().stamp();
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO event (slug, title_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT (slug) DO UPDATE SET title_id = COALESCE(excluded.title_id, event.title_id)
         RETURNING id",
    )
    .bind(slug)
    .bind(title_id)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn set_release_day(pool: &SqlitePool, id: i64, day: NaiveDate) -> AppResult<()> {
    let now = Utc::now().stamp();
    sqlx::query(
        "UPDATE event SET release_day = ?, released_checked_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(day.format("%Y-%m-%d").to_string())
    .bind(&now)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(pool: &SqlitePool, id: i64, on: bool) -> AppResult<()> {
    sqlx::query("UPDATE event SET state = ?, updated_at = ? WHERE id = ?")
        .bind(if on { "on" } else { "off" })
        .bind(Utc::now().stamp())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_pinned(pool: &SqlitePool, id: i64, at: Option<DateTime<Utc>>) -> AppResult<()> {
    sqlx::query("UPDATE event SET pinned_at = ?, updated_at = ? WHERE id = ?")
        .bind(at.map(|a| a.stamp()))
        .bind(Utc::now().stamp())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// One title on an event's list.
#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct ListRow {
    pub position: i64,
    pub title_id: i64,
    pub season_from: Option<i64>,
    pub season_to: Option<i64>,
    pub group_name: String,
    pub essential: bool,
    pub note: Option<String>,
}

/// The list as the pack has it now: written whole, so a title the pack
/// dropped goes.
pub async fn set_titles(pool: &SqlitePool, event_id: i64, rows: &[ListRow]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM event_title WHERE event_id = ?")
        .bind(event_id)
        .execute(&mut *tx)
        .await?;
    for r in rows {
        sqlx::query(
            "INSERT INTO event_title (event_id, position, title_id, season_from, season_to,
                                      group_name, essential, note)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event_id)
        .bind(r.position)
        .bind(r.title_id)
        .bind(r.season_from)
        .bind(r.season_to)
        .bind(&r.group_name)
        .bind(r.essential)
        .bind(&r.note)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn titles(pool: &SqlitePool, event_id: i64) -> AppResult<Vec<ListRow>> {
    Ok(sqlx::query_as(
        "SELECT position, title_id, season_from, season_to, group_name, essential, note
           FROM event_title WHERE event_id = ? ORDER BY position",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?)
}

pub async fn hidden_ids(pool: &SqlitePool, user_id: i64) -> AppResult<HashSet<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT event_id FROM event_hidden WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

pub async fn set_hidden(
    pool: &SqlitePool,
    user_id: i64,
    event_id: i64,
    hidden: bool,
) -> AppResult<()> {
    if hidden {
        sqlx::query("INSERT OR IGNORE INTO event_hidden (user_id, event_id, at) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(event_id)
            .bind(Utc::now().stamp())
            .execute(pool)
            .await?;
    } else {
        sqlx::query("DELETE FROM event_hidden WHERE user_id = ? AND event_id = ?")
            .bind(user_id)
            .bind(event_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn seen_ids(pool: &SqlitePool, user_id: i64) -> AppResult<HashSet<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT event_id FROM event_seen WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Says whether this was the first time, so a second device stays still.
pub async fn mark_seen(pool: &SqlitePool, user_id: i64, event_id: i64) -> AppResult<bool> {
    let done =
        sqlx::query("INSERT OR IGNORE INTO event_seen (user_id, event_id, at) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(event_id)
            .bind(Utc::now().stamp())
            .execute(pool)
            .await?
            .rows_affected();
    Ok(done > 0)
}

/// (season, episode, runtime, first recorded)
type EpisodeRow = (i64, i64, Option<i64>, Option<String>);

/// How far one account is with one title on a list. A film is seen with any
/// watch; a show once every released regular episode of its listed seasons
/// is. "Recorded" is when Frog first knew: `logged_at`, else the watch's
/// own time for watches older than that column.
pub async fn standing(
    pool: &SqlitePool,
    user_id: i64,
    row: &ListRow,
    kind: &str,
    runtime: Option<i64>,
) -> AppResult<Standing> {
    if kind == "movie" {
        let (first,): (Option<String>,) = sqlx::query_as(
            "SELECT MIN(COALESCE(logged_at, watched_at)) FROM watch_event
              WHERE user_id = ? AND title_id = ?",
        )
        .bind(user_id)
        .bind(row.title_id)
        .fetch_one(pool)
        .await?;
        let seen_at = first.as_deref().and_then(parse);
        let seen = first.is_some();
        return Ok(Standing {
            seen,
            watched: seen as i64,
            total: 1,
            minutes_left: if seen {
                0
            } else {
                runtime.filter(|r| *r > 0).unwrap_or(FILM_MINUTES)
            },
            next: None,
            seen_at,
        });
    }
    let rows: Vec<EpisodeRow> = sqlx::query_as(concat!(
        "SELECT e.season_number, e.episode_number, e.runtime,
                (SELECT MIN(COALESCE(w.logged_at, w.watched_at)) FROM watch_event w
                  WHERE w.user_id = ?1 AND w.episode_id = e.id)
           FROM episode e
          WHERE e.title_id = ?2 AND e.season_number > 0
            AND (?3 IS NULL OR e.season_number >= ?3)
            AND (?4 IS NULL OR e.season_number <= ?4)
            AND ",
        crate::released!(),
        " ORDER BY e.season_number, e.episode_number"
    ))
    .bind(user_id)
    .bind(row.title_id)
    .bind(row.season_from)
    .bind(row.season_to)
    .fetch_all(pool)
    .await?;
    let episodes: Vec<(i64, i64, i64, Option<DateTime<Utc>>)> = rows
        .into_iter()
        .map(|(s, e, rt, first)| {
            // A watch whose time cannot be read still counts as watched.
            let at = first.map(|f| parse(&f).unwrap_or(DateTime::<Utc>::MIN_UTC));
            (s, e, rt.unwrap_or(0), at)
        })
        .collect();
    let fallback = runtime.filter(|r| *r > 0).unwrap_or(EPISODE_MINUTES);
    Ok(show_standing(&episodes, fallback))
}

/// Whether this account has the title in its library.
pub async fn in_library(pool: &SqlitePool, user_id: i64) -> AppResult<HashSet<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT title_id FROM library_entry WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Who has a title on their watchlist, of those the viewer may see.
pub async fn watchlisted(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<bool> {
    let (on,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM library_entry
                         WHERE user_id = ? AND title_id = ? AND status = 'planned')",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    Ok(on)
}

/// The first watch of the event's own title, or of an episode of its season.
pub async fn watches_of_target(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    season: Option<i64>,
) -> AppResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT w.id FROM watch_event w
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ? AND w.title_id = ?
            AND (?3 IS NULL OR e.season_number = ?3)
          ORDER BY w.watched_at, w.id",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(season)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}
