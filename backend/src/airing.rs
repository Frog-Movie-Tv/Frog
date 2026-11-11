//! The Airing page: every show on TMDB with an episode in a five-week
//! window, and every film released in it here, most popular first.
//!
//! Once a day the worker asks TMDB's `/discover/tv` for every show with an
//! episode in the window, reads each show's seasons there, and keeps the
//! episodes inside it; and `/discover/movie` for the films released in this
//! instance's region (`refresh_list`). Everything kept is TMDB's: names,
//! posters, networks, numbers, days and popularity. `crate::airtime` then adds
//! the moment each episode airs, every few hours. The page reads only these
//! tables, so a view never reaches another site.
//!
//! An episode belongs to the day its moment falls on where the viewer is, or
//! to TMDB's day when no moment is known (`day`).

use std::collections::{HashMap, HashSet};
use std::time::Duration as Pause;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::airtime;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// The window, in days from today: last week, and four weeks ahead.
pub const BEFORE_DAYS: i64 = 7;
pub const AFTER_DAYS: i64 = 28;

/// A day either side of the window is listed too, so the first and last days
/// are whole wherever the viewer is.
const MARGIN_DAYS: i64 = 1;

/// How often the list is built again, and how long a failed build waits.
const LIST_HOURS: i64 = 24;
const RETRY_HOURS: i64 = 1;

/// TMDB answers 500 pages of a list at most.
const MAX_PAGES: i64 = 500;

/// Seasons of one show read at most: a show starting a season while the last
/// one ends has two in the window; more is a show TMDB numbers oddly.
const MAX_SEASONS: usize = 3;

/// Pages of films read: past this, TMDB's list is films few would look for.
const FILM_PAGES: i64 = 25;

/// TMDB's release types listed: in cinemas, limited or wide, and digital.
const RELEASE_TYPES: [i64; 3] = [2, 3, 4];

/// Between two TMDB calls, to stay well under its limit.
const PACE: Pause = Pause::from_millis(60);

/// The first and last day of the window around `today`.
pub fn window(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    (
        today - Duration::days(BEFORE_DAYS),
        today + Duration::days(AFTER_DAYS),
    )
}

fn parse_day(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()
}

/// The seasons of a TMDB show that have episodes between `from` and `to`:
/// those running over the window by their first days, and those of the last
/// and next episodes. Specials are left out.
pub fn seasons_in_window(detail: &Value, from: NaiveDate, to: NaiveDate) -> Vec<i64> {
    let mut seasons: Vec<(i64, Option<NaiveDate>)> = detail["seasons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let n = s["season_number"].as_i64().filter(|n| *n > 0)?;
            Some((n, s["air_date"].as_str().and_then(parse_day)))
        })
        .collect();
    seasons.sort_by_key(|s| s.0);

    let mut out: Vec<i64> = Vec::new();
    for key in ["last_episode_to_air", "next_episode_to_air"] {
        let e = &detail[key];
        let (Some(n), Some(day)) = (
            e["season_number"].as_i64().filter(|n| *n > 0),
            e["air_date"].as_str().and_then(parse_day),
        ) else {
            continue;
        };
        if day >= from - Duration::days(60) && day <= to && !out.contains(&n) {
            out.push(n);
        }
    }
    for (i, (n, start)) in seasons.iter().enumerate() {
        let Some(start) = start else { continue };
        let next_start = seasons.get(i + 1).and_then(|s| s.1);
        if *start <= to && next_start.is_none_or(|next| next >= from) && !out.contains(n) {
            out.push(*n);
        }
    }
    out.sort_unstable();
    // The latest ones, when TMDB's seasons say something unlikely.
    let skip = out.len().saturating_sub(MAX_SEASONS);
    out.split_off(skip)
}

/// One episode as the list keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub season: i64,
    pub episode: i64,
    pub name: Option<String>,
    pub air_date: NaiveDate,
}

/// The episodes in a TMDB season answer between `from` and `to`.
pub fn episodes_in_window(season: &Value, from: NaiveDate, to: NaiveDate) -> Vec<Listed> {
    season["episodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let air_date = e["air_date"].as_str().and_then(parse_day)?;
            (air_date >= from && air_date <= to).then_some(())?;
            Some(Listed {
                season: e["season_number"].as_i64().filter(|n| *n > 0)?,
                episode: e["episode_number"].as_i64()?,
                name: e["name"]
                    .as_str()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_string),
                air_date,
            })
        })
        .collect()
}

/// A film's release here inside the window, and its TMDB release type, from
/// `/movie/{id}` with `release_dates`: the region's first cinema or digital
/// release, else the primary release day, counted as in cinemas.
pub fn film_release(
    detail: &Value,
    region: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Option<(NaiveDate, i64)> {
    let inside = |d: NaiveDate| d >= from && d <= to;
    let here = detail["release_dates"]["results"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["iso_3166_1"].as_str() == Some(region))
        .and_then(|r| {
            r["release_dates"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| {
                    let kind = d["type"].as_i64().filter(|t| RELEASE_TYPES.contains(t))?;
                    Some((d["release_date"].as_str().and_then(parse_day)?, kind))
                })
                .min()
        });
    match here {
        Some((day, kind)) => inside(day).then_some((day, kind)),
        None => detail["release_date"]
            .as_str()
            .and_then(parse_day)
            .filter(|d| inside(*d))
            .map(|d| (d, 3)),
    }
}

/// Build the list again when a day has passed since the last one, and no
/// build began in the last hour.
pub async fn refresh_if_due(state: &AppState) {
    if !state.tmdb.is_configured() {
        return;
    }
    let row: Option<(Option<String>, Option<String>)> =
        match sqlx::query_as("SELECT started_at, listed_at FROM airing_state WHERE id = 1")
            .fetch_optional(&state.pool)
            .await
        {
            Ok(row) => row,
            Err(e) => {
                tracing::warn!(error = %e, "could not read the Airing list's state");
                return;
            }
        };
    let (started, listed) = row.unwrap_or((None, None));
    let ago = |at: &Option<String>| {
        at.as_deref()
            .and_then(|a| DateTime::parse_from_rfc3339(a).ok())
            .map(|a| Utc::now() - a.with_timezone(&Utc))
    };
    if ago(&listed).is_some_and(|a| a < Duration::hours(LIST_HOURS)) {
        return;
    }
    if ago(&started).is_some_and(|a| a < Duration::hours(RETRY_HOURS)) {
        return;
    }
    let _ = sqlx::query(
        "INSERT INTO airing_state (id, started_at) VALUES (1, ?)
         ON CONFLICT (id) DO UPDATE SET started_at = excluded.started_at",
    )
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await;
    match refresh_list(state).await {
        Ok(shows) => tracing::info!(shows, "Airing list built"),
        Err(e) => tracing::warn!(error = %e, "Airing list not built"),
    }
}

/// Read every show TMDB says airs in the window, and keep its episodes there.
/// Returns how many shows are listed.
pub async fn refresh_list(state: &AppState) -> AppResult<usize> {
    let started = Utc::now().to_rfc3339();
    let (first, last) = window(Utc::now().date_naive());
    let (from, to) = (
        first - Duration::days(MARGIN_DAYS),
        last + Duration::days(MARGIN_DAYS),
    );

    let films = list_films(state, from, to, &started).await?;

    // Most popular first, so the top of every day is there soonest.
    let mut ids: Vec<i64> = Vec::new();
    let mut seen = HashSet::new();
    let mut page = 1;
    loop {
        let body = state
            .tmdb
            .fetch(
                "/discover/tv",
                &[
                    ("air_date.gte", from.to_string()),
                    ("air_date.lte", to.to_string()),
                    ("sort_by", "popularity.desc".into()),
                    ("include_adult", "false".into()),
                    ("include_null_first_air_dates", "false".into()),
                    ("page", page.to_string()),
                ],
            )
            .await?;
        for r in body["results"].as_array().into_iter().flatten() {
            if let Some(id) = r["id"].as_i64() {
                if seen.insert(id) {
                    ids.push(id);
                }
            }
        }
        let pages = body["total_pages"].as_i64().unwrap_or(1).min(MAX_PAGES);
        if page >= pages {
            break;
        }
        page += 1;
        tokio::time::sleep(PACE).await;
    }

    let mut listed = 0;
    for id in ids {
        tokio::time::sleep(PACE).await;
        match list_show(state, id, from, to, &started).await {
            Ok(true) => listed += 1,
            Ok(false) => {}
            // Gone from TMDB since the list was read.
            Err(AppError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }

    // What this build did not see has left the window.
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "DELETE FROM airing_episode
          WHERE tmdb_id IN (SELECT tmdb_id FROM airing_show WHERE listed_at < ?)",
    )
    .bind(&started)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM airing_show WHERE listed_at < ?")
        .bind(&started)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM airing_film WHERE listed_at < ?")
        .bind(&started)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM airing_episode WHERE air_date < ? OR air_date > ?")
        .bind(from.to_string())
        .bind(to.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO airing_state (id, listed_at, shows) VALUES (1, ?, ?)
         ON CONFLICT (id) DO UPDATE SET listed_at = excluded.listed_at, shows = excluded.shows",
    )
    .bind(Utc::now().to_rfc3339())
    .bind((listed + films) as i64)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(listed + films)
}

/// The films TMDB says are released here in the window, most popular first.
/// Returns how many are listed.
async fn list_films(
    state: &AppState,
    from: NaiveDate,
    to: NaiveDate,
    started: &str,
) -> AppResult<usize> {
    let region = state.tmdb.region().to_string();
    let types = RELEASE_TYPES.map(|t| t.to_string()).join("|");
    let mut ids: Vec<i64> = Vec::new();
    let mut page = 1;
    loop {
        let body = state
            .tmdb
            .fetch(
                "/discover/movie",
                &[
                    ("region", region.clone()),
                    ("release_date.gte", from.to_string()),
                    ("release_date.lte", to.to_string()),
                    ("with_release_type", types.clone()),
                    ("sort_by", "popularity.desc".into()),
                    ("include_adult", "false".into()),
                    ("page", page.to_string()),
                ],
            )
            .await?;
        ids.extend(
            body["results"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r["id"].as_i64()),
        );
        let pages = body["total_pages"].as_i64().unwrap_or(1).min(FILM_PAGES);
        if page >= pages {
            break;
        }
        page += 1;
        tokio::time::sleep(PACE).await;
    }

    let mut listed = 0;
    for id in ids {
        tokio::time::sleep(PACE).await;
        let detail = match state
            .tmdb
            .fetch(
                &format!("/movie/{id}"),
                &[("append_to_response", "release_dates".into())],
            )
            .await
        {
            Ok(detail) => detail,
            Err(AppError::NotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        let Some((day, kind)) = film_release(&detail, &region, from, to) else {
            continue;
        };
        let name = detail["title"].as_str().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let original = detail["original_title"].as_str().filter(|o| *o != name);
        sqlx::query(
            "INSERT INTO airing_film (tmdb_id, name, search_name, poster_path, genres, popularity,
                                      release_date, release_type, listed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (tmdb_id) DO UPDATE SET
                name = excluded.name,
                search_name = excluded.search_name,
                poster_path = excluded.poster_path,
                genres = excluded.genres,
                popularity = excluded.popularity,
                release_date = excluded.release_date,
                release_type = excluded.release_type,
                listed_at = excluded.listed_at",
        )
        .bind(id)
        .bind(name)
        .bind(crate::text::search_name(name, original))
        .bind(detail["poster_path"].as_str())
        .bind(genre_names(&detail))
        .bind(detail["popularity"].as_f64().unwrap_or(0.0))
        .bind(day.to_string())
        .bind(kind)
        .bind(started)
        .execute(&state.pool)
        .await?;
        listed += 1;
    }
    Ok(listed)
}

/// TMDB's genre names between bars, "|Animation|Drama|", or nothing.
fn genre_names(detail: &Value) -> String {
    let names: Vec<&str> = detail["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| g["name"].as_str())
        .collect();
    if names.is_empty() {
        String::new()
    } else {
        format!("|{}|", names.join("|"))
    }
}

/// One show: its detail and the seasons running in the window. False when
/// none of its episodes fall inside it after all.
async fn list_show(
    state: &AppState,
    tmdb_id: i64,
    from: NaiveDate,
    to: NaiveDate,
    started: &str,
) -> AppResult<bool> {
    let detail = state
        .tmdb
        .fetch(
            &format!("/tv/{tmdb_id}"),
            &[("append_to_response", "external_ids".into())],
        )
        .await?;
    let mut episodes = Vec::new();
    for season in seasons_in_window(&detail, from, to) {
        tokio::time::sleep(PACE).await;
        let body = match state
            .tmdb
            .fetch(&format!("/tv/{tmdb_id}/season/{season}"), &[])
            .await
        {
            Ok(body) => body,
            Err(AppError::NotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        episodes.extend(episodes_in_window(&body, from, to));
    }
    if episodes.is_empty() {
        return Ok(false);
    }
    let Some(show) = airtime::Show::from_tmdb(&detail) else {
        return Ok(false);
    };
    airtime::remember(&state.pool, &show).await?;
    if store(&state.pool, &detail, &show, &episodes, started).await? {
        airtime::mark_due(&state.pool, tmdb_id).await?;
    }
    Ok(true)
}

/// Write one show and its episodes. True when an episode is new or moved
/// day, so its time is worked out on the next pass.
async fn store(
    pool: &SqlitePool,
    detail: &Value,
    show: &airtime::Show,
    episodes: &[Listed],
    started: &str,
) -> AppResult<bool> {
    let genres = genre_names(detail);
    let network = detail["networks"][0]["name"].as_str();
    let original = (!show.original_name.is_empty()).then_some(show.original_name.as_str());

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO airing_show (tmdb_id, name, search_name, poster_path, network, genres,
                                  anime, popularity, listed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (tmdb_id) DO UPDATE SET
            name = excluded.name,
            search_name = excluded.search_name,
            poster_path = excluded.poster_path,
            network = excluded.network,
            genres = excluded.genres,
            anime = excluded.anime,
            popularity = excluded.popularity,
            listed_at = excluded.listed_at",
    )
    .bind(show.tmdb_id)
    .bind(&show.name)
    .bind(crate::text::search_name(&show.name, original))
    .bind(detail["poster_path"].as_str())
    .bind(network)
    .bind(&genres)
    .bind(show.anime)
    .bind(detail["popularity"].as_f64().unwrap_or(0.0))
    .bind(started)
    .execute(&mut *tx)
    .await?;

    let before: Vec<(i64, i64, String)> =
        sqlx::query_as("SELECT season, episode, air_date FROM airing_episode WHERE tmdb_id = ?")
            .bind(show.tmdb_id)
            .fetch_all(&mut *tx)
            .await?;
    let before: HashMap<(i64, i64), String> = before
        .into_iter()
        .map(|(s, e, day)| ((s, e), day))
        .collect();

    let mut changed = false;
    for e in episodes {
        let day = e.air_date.to_string();
        match before.get(&(e.season, e.episode)) {
            Some(was) if *was == day => {
                sqlx::query(
                    "UPDATE airing_episode SET name = ?
                      WHERE tmdb_id = ? AND season = ? AND episode = ?",
                )
                .bind(&e.name)
                .bind(show.tmdb_id)
                .bind(e.season)
                .bind(e.episode)
                .execute(&mut *tx)
                .await?;
            }
            _ => {
                // New, or moved to another day: its old moment no longer holds.
                changed = true;
                sqlx::query(
                    "INSERT INTO airing_episode (tmdb_id, season, episode, name, air_date)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT (tmdb_id, season, episode) DO UPDATE SET
                        name = excluded.name,
                        air_date = excluded.air_date,
                        air_at = NULL,
                        air_source = NULL",
                )
                .bind(show.tmdb_id)
                .bind(e.season)
                .bind(e.episode)
                .bind(&e.name)
                .bind(&day)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    for (season, episode) in before.keys() {
        if !episodes
            .iter()
            .any(|e| e.season == *season && e.episode == *episode)
        {
            sqlx::query(
                "DELETE FROM airing_episode WHERE tmdb_id = ? AND season = ? AND episode = ?",
            )
            .bind(show.tmdb_id)
            .bind(season)
            .bind(episode)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(changed)
}

// --- the page -----------------------------------------------------------------

/// Which kind of title the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Shows that are not anime.
    Shows,
    Anime,
    Films,
}

impl Kind {
    pub fn parse(raw: &str) -> Option<Kind> {
        match raw {
            "shows" => Some(Kind::Shows),
            "anime" => Some(Kind::Anime),
            "films" => Some(Kind::Films),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Kind::Shows => "shows",
            Kind::Anime => "anime",
            Kind::Films => "films",
        }
    }
}

/// What the page is narrowed to.
#[derive(Debug, Clone, Default)]
pub struct Filters {
    /// Only titles in the viewer's library.
    pub mine: bool,
    /// Only one kind of title.
    pub kind: Option<Kind>,
    /// A TMDB genre name.
    pub genre: Option<String>,
    /// Part of a name, folded (`text::fold`).
    pub search: Option<String>,
}

/// One show on one day: its first episode that day, and how many more. Or
/// one film released that day.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    /// `tv` or `movie`, as TMDB's ids are unique per kind.
    pub kind: String,
    pub tmdb_id: i64,
    /// Frog's id, when its catalog has the show.
    pub id: Option<i64>,
    pub name: String,
    pub poster_path: Option<String>,
    pub network: Option<String>,
    pub season: i64,
    pub episode: i64,
    pub episode_name: Option<String>,
    /// TMDB's day.
    pub air_date: String,
    /// The moment, UTC, when it is known.
    pub air_at: Option<String>,
    /// Other episodes of the show on the same day.
    pub more: i64,
    /// A film's TMDB release type: 2 or 3 in cinemas, 4 digital.
    pub release_type: Option<i64>,
    pub in_list: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Day {
    pub date: String,
    /// Shows on this day, filtered.
    pub total: i64,
    pub items: Vec<Item>,
}

/// The UTC moments a day starts and ends at in `zone`. A midnight that a clock
/// change skips starts the day at the first moment that does exist.
pub fn bounds(zone: Tz, day: NaiveDate) -> (String, String) {
    let start = |d: NaiveDate| {
        let midnight = d.and_hms_opt(0, 0, 0).unwrap_or_default();
        let at = zone
            .from_local_datetime(&midnight)
            .earliest()
            .or_else(|| {
                zone.from_local_datetime(&(midnight + Duration::hours(1)))
                    .earliest()
            })
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|| midnight.and_utc());
        airtime::stamp(at)
    };
    (start(day), start(day + Duration::days(1)))
}

/// The day an episode falls on in `zone`: its moment's, else TMDB's.
pub fn local_day(zone: Tz, air_date: &str, air_at: Option<&str>) -> Option<NaiveDate> {
    match air_at.and_then(|a| DateTime::parse_from_rfc3339(a).ok()) {
        Some(at) => Some(at.with_timezone(&zone).date_naive()),
        None => parse_day(air_date),
    }
}

/// Everything on one day, shows and films together, before filtering:
/// `?1` the day, `?2` and `?3` its first and last moments in UTC. A show's
/// first episode that day stands for it, with how many more there are.
const ON_THE_DAY: &str = "
    WITH shows AS (
      SELECT 'tv' AS kind, e.tmdb_id, s.name, s.poster_path, s.network, e.season, e.episode,
             e.name AS episode_name, e.air_date, e.air_at, s.popularity, s.anime, s.genres,
             s.search_name, NULL AS release_type,
             ROW_NUMBER() OVER (PARTITION BY e.tmdb_id
                                ORDER BY COALESCE(e.air_at, e.air_date), e.season, e.episode) AS n,
             COUNT(*) OVER (PARTITION BY e.tmdb_id) - 1 AS more
        FROM airing_episode e JOIN airing_show s ON s.tmdb_id = e.tmdb_id
       WHERE (e.air_at IS NULL AND e.air_date = ?1) OR (e.air_at >= ?2 AND e.air_at < ?3)
    ),
    every AS (
      SELECT kind, tmdb_id, name, poster_path, network, season, episode, episode_name,
             air_date, air_at, popularity, anime, genres, search_name, release_type, more
        FROM shows WHERE n = 1
      UNION ALL
      SELECT 'movie', f.tmdb_id, f.name, f.poster_path, NULL, 0, 0, NULL,
             f.release_date, NULL, f.popularity, 0, f.genres, f.search_name, f.release_type, 0
        FROM airing_film f WHERE f.release_date = ?1
    )";

/// The filters, from `?4` on. The library is read only for the viewer (`?5`).
const FILTERS: &str = "
    WHERE (?4 = 0 OR EXISTS (
          SELECT 1 FROM title_external x
            JOIN title t ON t.id = x.title_id
            JOIN library_entry l ON l.title_id = COALESCE(t.merged_into, t.id)
           WHERE x.provider = 'tmdb' AND x.external_id = every.kind || '/' || every.tmdb_id
             AND l.user_id = ?5))
      AND (?6 IS NULL
           OR (?6 = 'films' AND every.kind = 'movie')
           OR (?6 = 'anime' AND every.anime = 1)
           OR (?6 = 'shows' AND every.kind = 'tv' AND every.anime = 0))
      AND (?7 IS NULL OR instr(every.genres, '|' || ?7 || '|') > 0)
      AND (?8 IS NULL OR instr(every.search_name, ?8) > 0)";

/// (kind, tmdb_id, name, poster, network, season, episode, episode name,
/// air_date, air_at, more, release type)
type ItemRow = (
    String,
    i64,
    String,
    Option<String>,
    Option<String>,
    i64,
    i64,
    Option<String>,
    String,
    Option<String>,
    i64,
    Option<i64>,
);

/// One day of the page for one viewer: the most popular first, `limit` of
/// them from `offset`.
pub async fn day(
    pool: &SqlitePool,
    user_id: i64,
    zone: Tz,
    date: NaiveDate,
    filters: &Filters,
    offset: i64,
    limit: i64,
) -> AppResult<Day> {
    let (start, end) = bounds(zone, date);
    let date_text = date.to_string();
    let kind = filters.kind.map(Kind::as_str);

    let (total,): (i64,) = sqlx::query_as(&format!(
        "{ON_THE_DAY} SELECT COUNT(*) FROM every {FILTERS}"
    ))
    .bind(&date_text)
    .bind(&start)
    .bind(&end)
    .bind(filters.mine)
    .bind(user_id)
    .bind(kind)
    .bind(&filters.genre)
    .bind(&filters.search)
    .fetch_one(pool)
    .await?;

    let rows: Vec<ItemRow> = sqlx::query_as(&format!(
        "{ON_THE_DAY}
         SELECT kind, tmdb_id, name, poster_path, network, season, episode, episode_name,
                air_date, air_at, more, release_type
           FROM every {FILTERS}
          ORDER BY popularity DESC, name, tmdb_id
          LIMIT ?9 OFFSET ?10"
    ))
    .bind(&date_text)
    .bind(&start)
    .bind(&end)
    .bind(filters.mine)
    .bind(user_id)
    .bind(kind)
    .bind(&filters.genre)
    .bind(&filters.search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let (id, in_list) = in_catalog(pool, user_id, &row.0, row.1).await?;
        items.push(Item {
            kind: row.0,
            tmdb_id: row.1,
            id,
            name: row.2,
            poster_path: row.3,
            network: row.4,
            season: row.5,
            episode: row.6,
            episode_name: row.7,
            air_date: row.8,
            air_at: row.9,
            more: row.10,
            release_type: row.11,
            in_list,
        });
    }
    Ok(Day {
        date: date_text,
        total,
        items,
    })
}

/// Frog's title for a TMDB entry, and whether it is in the viewer's library.
async fn in_catalog(
    pool: &SqlitePool,
    user_id: i64,
    kind: &str,
    tmdb_id: i64,
) -> AppResult<(Option<i64>, bool)> {
    let row: Option<(i64, bool)> = sqlx::query_as(
        "SELECT COALESCE(t.merged_into, t.id),
                EXISTS (SELECT 1 FROM library_entry l
                         WHERE l.user_id = ?1 AND l.title_id = COALESCE(t.merged_into, t.id))
           FROM title_external x JOIN title t ON t.id = x.title_id
          WHERE x.provider = 'tmdb' AND x.external_id = ?2",
    )
    .bind(user_id)
    .bind(format!("{kind}/{tmdb_id}"))
    .fetch_optional(pool)
    .await?;
    Ok(row.map_or((None, false), |(id, mine)| (Some(id), mine)))
}

/// Every genre on the page, by name.
pub async fn genres(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT genres FROM airing_show UNION SELECT DISTINCT genres FROM airing_film",
    )
    .fetch_all(pool)
    .await?;
    let mut all: Vec<String> = rows
        .iter()
        .flat_map(|(g,)| g.split('|'))
        .filter(|g| !g.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    all.sort();
    Ok(all)
}

/// When the list was last built, and how many shows it holds.
pub async fn status(pool: &SqlitePool) -> AppResult<(Option<String>, i64)> {
    let row: Option<(Option<String>, i64)> =
        sqlx::query_as("SELECT listed_at, shows FROM airing_state WHERE id = 1")
            .fetch_optional(pool)
            .await?;
    let (listed, shows) = row.unwrap_or((None, 0));
    // A first build still running already shows what it has.
    let (kept,): (i64,) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM airing_show) + (SELECT COUNT(*) FROM airing_film)",
    )
    .fetch_one(pool)
    .await?;
    Ok((listed, shows.max(kept)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_day_in_brussels_is_its_own_midnights_in_utc() {
        let zone: Tz = "Europe/Brussels".parse().unwrap();
        assert_eq!(
            bounds(zone, d("2026-11-16")),
            ("2026-11-15T23:00:00Z".into(), "2026-11-16T23:00:00Z".into())
        );
        // The day the clocks go back is 25 hours long.
        assert_eq!(
            bounds(zone, d("2026-10-25")),
            ("2026-10-24T22:00:00Z".into(), "2026-10-25T23:00:00Z".into())
        );
    }

    #[test]
    fn an_evening_in_new_york_is_the_next_morning_in_brussels() {
        // The Gilded Age: TMDB says Sunday 15 November; 21:00 Eastern.
        let brussels: Tz = "Europe/Brussels".parse().unwrap();
        let new_york: Tz = "America/New_York".parse().unwrap();
        let at = Some("2026-11-16T02:00:00Z");
        assert_eq!(local_day(brussels, "2026-11-15", at), Some(d("2026-11-16")));
        assert_eq!(local_day(new_york, "2026-11-15", at), Some(d("2026-11-15")));
        // No moment: TMDB's day, wherever the viewer is.
        assert_eq!(
            local_day(brussels, "2026-11-15", None),
            Some(d("2026-11-15"))
        );
    }

    #[test]
    fn a_late_night_slot_in_japan_is_the_afternoon_before_in_brussels() {
        // Ranma ½: 00:55 on Sunday in Tokyo; TMDB writes Sunday.
        let brussels: Tz = "Europe/Brussels".parse().unwrap();
        assert_eq!(
            local_day(brussels, "2026-11-15", Some("2026-11-14T15:55:00Z")),
            Some(d("2026-11-14"))
        );
    }

    #[test]
    fn the_seasons_read_are_those_running_in_the_window() {
        let detail = json!({
            "seasons": [
                { "season_number": 0, "air_date": "2016-11-27" },
                { "season_number": 1, "air_date": "2017-10-03" },
                { "season_number": 2, "air_date": "2026-10-03" }
            ],
            "last_episode_to_air": { "season_number": 2, "air_date": "2026-11-07" },
            "next_episode_to_air": { "season_number": 2, "air_date": "2026-11-14" }
        });
        assert_eq!(
            seasons_in_window(&detail, d("2026-11-03"), d("2026-12-10")),
            [2]
        );
        // One season of many years, still running.
        let one = json!({ "seasons": [{ "season_number": 1, "air_date": "2024-10-06" }] });
        assert_eq!(
            seasons_in_window(&one, d("2026-11-03"), d("2026-12-10")),
            [1]
        );
        // A new season starting while the window is open, after an old one.
        let next = json!({ "seasons": [
            { "season_number": 3, "air_date": "2025-06-22" },
            { "season_number": 4, "air_date": "2026-11-01" }
        ]});
        assert_eq!(
            seasons_in_window(&next, d("2026-11-03"), d("2026-12-10")),
            [4]
        );
    }

    #[test]
    fn only_episodes_inside_the_window_are_kept() {
        let season = json!({ "episodes": [
            { "season_number": 4, "episode_number": 2, "air_date": "2026-11-08", "name": "Two" },
            { "season_number": 4, "episode_number": 3, "air_date": "2026-11-15", "name": " " },
            { "season_number": 4, "episode_number": 9, "air_date": "2027-01-10", "name": "Late" },
            { "season_number": 4, "episode_number": 10, "air_date": null }
        ]});
        let kept = episodes_in_window(&season, d("2026-11-03"), d("2026-12-10"));
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].name, None);
        assert_eq!(kept[1].air_date, d("2026-11-15"));
    }

    #[test]
    fn a_film_is_listed_on_its_release_here() {
        let detail = json!({
            "title": "Wicked: For Good", "release_date": "2026-11-19",
            "release_dates": { "results": [
                { "iso_3166_1": "US", "release_dates": [{ "type": 3, "release_date": "2026-11-21T00:00:00.000Z" }] },
                { "iso_3166_1": "BE", "release_dates": [
                    { "type": 1, "release_date": "2026-11-10T00:00:00.000Z" },
                    { "type": 3, "release_date": "2026-11-19T00:00:00.000Z" },
                    { "type": 4, "release_date": "2027-01-10T00:00:00.000Z" }
                ]}
            ]}
        });
        let (from, to) = (d("2026-11-03"), d("2026-12-10"));
        assert_eq!(
            film_release(&detail, "BE", from, to),
            Some((d("2026-11-19"), 3))
        );
        // No release listed here: the primary day, inside the window.
        assert_eq!(
            film_release(&detail, "NL", from, to),
            Some((d("2026-11-19"), 3))
        );
        // Out here after the window: not listed.
        let late = json!({ "release_date": "2026-11-19", "release_dates": { "results": [
            { "iso_3166_1": "BE", "release_dates": [{ "type": 4, "release_date": "2027-02-01" }] }
        ]}});
        assert_eq!(film_release(&late, "BE", from, to), None);
    }

    #[test]
    fn the_window_is_last_week_and_four_ahead() {
        assert_eq!(window(d("2026-11-11")), (d("2026-11-04"), d("2026-12-09")));
    }
}
