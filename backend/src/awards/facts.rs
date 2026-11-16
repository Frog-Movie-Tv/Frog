//! What the badge rules read about one account: a handful of queries for the
//! whole account, as `store::ItemFacts` does, then plain values.

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;

use super::eligible::{self, Logged, EPISODE_MINUTES, FILM_MINUTES};
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct Watch {
    pub id: i64,
    pub title_id: i64,
    pub episode_id: Option<i64>,
    pub season: Option<i64>,
    pub watched_at: DateTime<Utc>,
    pub logged_at: Option<DateTime<Utc>>,
    pub how: Option<String>,
    pub undated: bool,
    pub is_rewatch: bool,
    pub source: String,
    pub minutes: f64,
    pub with_someone: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub title_id: i64,
    pub kind: String,
    pub status: String,
    /// The title's rating, or the average of its seasons'.
    pub rating: Option<f64>,
    pub favourite: bool,
    pub has_notes: bool,
    pub added_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub rewatch_count: i64,
    pub progress: Option<i64>,
    pub release: Option<NaiveDate>,
    pub genres: Vec<String>,
    /// TMDB's score, 0–10.
    pub tmdb_vote: Option<f64>,
    pub tmdb_status: Option<String>,
    pub episode_count: Option<i64>,
    pub anime: bool,
    /// TMDB's origin countries, and the language it was made in ("" when
    /// not known yet).
    pub countries: Vec<String>,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub id: i64,
    pub title_id: i64,
    pub season: i64,
    /// The day it airs: from its moment in the account's zone when known.
    pub air_day: Option<NaiveDate>,
    pub air_at: Option<DateTime<Utc>>,
    pub released: bool,
}

/// One person, director or studio on a title in the library.
#[derive(Debug, Clone)]
pub struct Credit {
    pub title_id: i64,
    pub role: String,
    pub person: i64,
}

#[derive(Debug, Clone)]
pub struct Facts {
    pub tz: Tz,
    pub now: DateTime<Utc>,
    pub watches: Vec<Watch>,
    pub entries: Vec<Entry>,
    pub episodes: Vec<Episode>,
    /// Days chapters were read on, one per chapter.
    pub reads: Vec<NaiveDate>,
    pub credits: Vec<Credit>,
    pub shelves: i64,
    pub joined: Option<DateTime<Utc>>,
    /// Taste matches with the accounts that may be compared, in percent.
    pub tastes: Vec<u8>,
    /// Titles this account was first here to add, with the day the second
    /// account that may be compared added it after.
    pub pioneered: Vec<(i64, NaiveDate)>,
}

impl Facts {
    pub fn today(&self) -> NaiveDate {
        eligible::day(self.now, self.tz)
    }

    pub fn day(&self, at: DateTime<Utc>) -> NaiveDate {
        eligible::day(at, self.tz)
    }

    /// The watches as the eligibility rules take them.
    pub fn logged(&self) -> Vec<Logged> {
        self.watches
            .iter()
            .map(|w| Logged {
                id: w.id,
                title_id: w.title_id,
                watched_at: w.watched_at,
                logged_at: w.logged_at,
                how: w.how.clone(),
                undated: w.undated,
                minutes: w.minutes,
            })
            .collect()
    }
}

#[derive(FromRow)]
struct WatchRow {
    id: i64,
    title_id: i64,
    episode_id: Option<i64>,
    season: Option<i64>,
    watched_at: String,
    logged_at: Option<String>,
    logged_how: Option<String>,
    undated: bool,
    is_rewatch: bool,
    source: String,
    runtime: Option<i64>,
    with_someone: bool,
}

#[derive(FromRow)]
struct EntryRow {
    title_id: i64,
    kind: String,
    status: String,
    rating: Option<f64>,
    favourite: bool,
    has_notes: bool,
    added_at: Option<String>,
    finished_at: Option<String>,
    rewatch_count: i64,
    progress: Option<i64>,
    release_date: Option<String>,
    genres_json: String,
    tmdb_vote: Option<f64>,
    tmdb_status: Option<String>,
    episode_count: Option<i64>,
    original_name: Option<String>,
    linked: bool,
    countries_json: Option<String>,
    original_language: Option<String>,
}

#[derive(FromRow)]
struct EpisodeRow {
    id: i64,
    title_id: i64,
    season_number: i64,
    air_date: Option<String>,
    air_at: Option<String>,
    released: bool,
}

/// A date or a moment, as Frog writes either, to its day.
fn date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

/// Anime: what AniList knows, or an animated title named in Japanese kana.
/// Frog keeps no country of origin, so a Japanese film TMDB lists only as
/// Animation is told by its original name.
pub fn is_anime(kind: &str, genres: &[String], original_name: Option<&str>, linked: bool) -> bool {
    if kind == "manga" {
        return false;
    }
    let kana = original_name
        .unwrap_or_default()
        .chars()
        .any(|c| ('\u{3040}'..='\u{30ff}').contains(&c));
    linked || (genres.iter().any(|g| g == "Animation") && kana)
}

pub async fn read(pool: &SqlitePool, user_id: i64, tz: Tz, now: DateTime<Utc>) -> AppResult<Facts> {
    let watches: Vec<WatchRow> = sqlx::query_as(
        "SELECT w.id, w.title_id, w.episode_id, e.season_number AS season, w.watched_at,
                w.logged_at, w.logged_how, w.undated, w.is_rewatch, w.source,
                COALESCE(e.runtime, t.runtime) AS runtime,
                EXISTS (SELECT 1 FROM watch_with x
                         WHERE x.watch_event_id = w.id AND x.user_id = w.user_id)
                  OR w.logged_how = 'together' AS with_someone
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let watches = watches
        .into_iter()
        .filter_map(|r| {
            let film = r.episode_id.is_none();
            Some(Watch {
                id: r.id,
                title_id: r.title_id,
                episode_id: r.episode_id,
                season: r.season,
                watched_at: eligible::parse(&r.watched_at)?,
                logged_at: r.logged_at.as_deref().and_then(eligible::parse),
                how: r.logged_how,
                undated: r.undated,
                is_rewatch: r.is_rewatch,
                source: r.source,
                minutes: r
                    .runtime
                    .filter(|m| *m > 0)
                    .map_or(if film { FILM_MINUTES } else { EPISODE_MINUTES }, |m| {
                        m as f64
                    }),
                with_someone: r.with_someone,
            })
        })
        .collect();

    let entries: Vec<EntryRow> = sqlx::query_as(
        "SELECT l.title_id, t.kind, l.status,
                COALESCE(l.rating, (SELECT AVG(s.rating) FROM season_rating s
                                     WHERE s.user_id = l.user_id
                                       AND s.title_id = l.title_id)) AS rating,
                l.favourite, COALESCE(TRIM(l.notes) <> '', 0) AS has_notes, l.added_at,
                l.finished_at, l.rewatch_count, l.progress, t.release_date, t.genres_json,
                t.tmdb_vote, t.tmdb_status, t.episode_count, t.original_name,
                t.countries_json, t.original_language,
                EXISTS (SELECT 1 FROM title_external x
                         WHERE x.title_id = t.id AND x.provider = 'anilist')
                  OR (t.genres_json LIKE '%\"Animation\"%'
                      -- By number, so anime_map's index answers: matching the
                      -- address as text read the whole map once per title.
                      AND EXISTS (SELECT 1 FROM title_external x
                                    JOIN anime_map m
                                      ON m.tmdb_id = CAST(substr(x.external_id,
                                                                 length(t.kind) + 2)
                                                          AS INTEGER)
                                   WHERE x.title_id = t.id AND x.provider = 'tmdb'
                                     AND x.external_id = t.kind || '/' || m.tmdb_id)) AS linked
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let entries = entries
        .into_iter()
        .map(|r| {
            let genres: Vec<String> = serde_json::from_str(&r.genres_json).unwrap_or_default();
            Entry {
                anime: is_anime(&r.kind, &genres, r.original_name.as_deref(), r.linked),
                title_id: r.title_id,
                kind: r.kind,
                status: r.status,
                rating: r.rating,
                favourite: r.favourite,
                has_notes: r.has_notes,
                added_at: r.added_at.as_deref().and_then(eligible::parse),
                finished_at: r.finished_at.as_deref().and_then(eligible::parse),
                rewatch_count: r.rewatch_count,
                progress: r.progress,
                release: r.release_date.as_deref().and_then(date),
                genres,
                tmdb_vote: r.tmdb_vote,
                tmdb_status: r.tmdb_status,
                episode_count: r.episode_count,
                countries: r
                    .countries_json
                    .as_deref()
                    .and_then(|c| serde_json::from_str(c).ok())
                    .unwrap_or_default(),
                language: r.original_language.unwrap_or_default(),
            }
        })
        .collect();

    let episodes: Vec<EpisodeRow> = sqlx::query_as(concat!(
        "SELECT e.id, e.title_id, e.season_number, e.air_date, e.air_at, ",
        crate::released!(),
        " AS released
           FROM episode e
           JOIN library_entry l ON l.title_id = e.title_id AND l.user_id = ?
          WHERE e.season_number > 0"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let episodes = episodes
        .into_iter()
        .map(|r| {
            let air_at = r.air_at.as_deref().and_then(eligible::parse);
            Episode {
                id: r.id,
                title_id: r.title_id,
                season: r.season_number,
                air_day: air_at
                    .map(|at| eligible::day(at, tz))
                    .or_else(|| r.air_date.as_deref().and_then(date)),
                air_at,
                released: r.released,
            }
        })
        .collect();

    let reads: Vec<(String,)> =
        sqlx::query_as("SELECT read_at FROM chapter_read WHERE user_id = ? ORDER BY read_at")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    let reads = reads
        .iter()
        .filter_map(|(at,)| {
            eligible::parse(at)
                .map(|t| eligible::day(t, tz))
                .or_else(|| date(at))
        })
        .collect();

    // Leading cast only: someone far down the credits did not draw you to it.
    let credits: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT p.title_id, p.role, p.tmdb_id
           FROM title_person p
           JOIN library_entry l ON l.title_id = p.title_id AND l.user_id = ?
          WHERE p.role IN ('director', 'studio') OR (p.role = 'cast' AND p.ord < 15)",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let credits = credits
        .into_iter()
        .map(|(title_id, role, person)| Credit {
            title_id,
            role,
            person,
        })
        .collect();

    let (shelves,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM shelf WHERE user_id = ? AND watchlist = 0")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    let joined: Option<(String,)> = sqlx::query_as("SELECT created_at FROM app_user WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    let others = comparable(pool, user_id).await?;
    let mut tastes = Vec::new();
    for other in &others {
        if let Some(m) = crate::taste::between(pool, user_id, *other).await? {
            tastes.push(m.percent);
        }
    }
    let pioneered = pioneered(pool, user_id, &others, tz).await?;

    Ok(Facts {
        tz,
        now,
        watches,
        entries,
        episodes,
        reads,
        credits,
        shelves,
        joined: joined.and_then(|(at,)| eligible::parse(&at)),
        tastes,
        pioneered,
    })
}

/// The accounts a badge may compare this one with: their profile is on, and
/// they have not switched Recommendations off (`store::shown_to`, as Your
/// people and taste matches use).
async fn comparable(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<i64>> {
    let mut out = Vec::new();
    for account in crate::store::shown_to(pool, user_id).await? {
        if crate::inbox::wants(pool, account.id).await?.recommendations {
            out.push(account.id);
        }
    }
    Ok(out)
}

/// Titles this account added before anyone else here, which at least two
/// comparable accounts added later. Only the count reaches the badge, never
/// who or which of theirs.
async fn pioneered(
    pool: &SqlitePool,
    user_id: i64,
    comparable: &[i64],
    tz: Tz,
) -> AppResult<Vec<(i64, NaiveDate)>> {
    let rows: Vec<(i64, String, i64, String)> = sqlx::query_as(
        "SELECT l.title_id, l.added_at, o.user_id, o.added_at
           FROM library_entry l
           JOIN library_entry o ON o.title_id = l.title_id AND o.user_id <> l.user_id
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let shown: HashSet<i64> = comparable.iter().copied().collect();
    let mut titles: std::collections::BTreeMap<i64, (String, Vec<(i64, String)>)> =
        Default::default();
    for (title, mine, other, theirs) in rows {
        titles
            .entry(title)
            .or_insert_with(|| (mine, Vec::new()))
            .1
            .push((other, theirs));
    }
    let mut out = Vec::new();
    for (title, (mine, others)) in titles {
        // Anyone at all who had it first, seen or not, means this one was not.
        if others.iter().any(|(_, theirs)| *theirs <= mine) {
            continue;
        }
        let mut later: Vec<&String> = others
            .iter()
            .filter(|(o, _)| shown.contains(o))
            .map(|(_, at)| at)
            .collect();
        later.sort();
        if let Some(second) = later.get(1) {
            let day = eligible::parse(second)
                .map(|t| eligible::day(t, tz))
                .or_else(|| date(second));
            if let Some(day) = day {
                out.push((title, day));
            }
        }
    }
    Ok(out)
}
