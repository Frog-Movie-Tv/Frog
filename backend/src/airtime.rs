//! The moment an episode airs, for episodes TMDB lists.
//!
//! TMDB says which shows and episodes exist and on which day. This adds only
//! the moment: AniList's airing schedule for anime (a TMDB show in Animation
//! from Japan), TVmaze's `airstamp` for everything else, and nothing when
//! neither knows, so a page says "time TBA" rather than guessing. No title,
//! name, picture or episode is ever taken from either.
//!
//! The matching is pure (`match_anilist`, `match_tvmaze`) and `resolve` reads
//! the providers. How a TMDB show is found on each is kept for good in
//! `airtime_show`. A result is stored where it is shown, as UTC:
//! `episode.air_at` for titles in Frog's catalog and `airing_episode.air_at`
//! for the Airing page. `refresh_due` works every show out again every few
//! hours; TMDB's `air_date` is never written here.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Asia::Tokyo;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::anilist::Api;
use crate::catalog::{self, ExternalId};
use crate::error::AppResult;
use crate::state::AppState;
use crate::text::fold;
use crate::tvmaze;

/// How long a show's worked-out times stand before they are worked out again.
pub const TIMES_HOURS: i64 = 6;

/// A show TVmaze did not have is asked about again after this long.
const LOOKUP_RETRY_DAYS: i64 = 7;

/// An AniList search by name stands this long before it runs again, for a
/// show whose newest season the anime map does not cover yet.
const SEARCH_RETRY_DAYS: i64 = 30;

/// Matched by number alone, AniList's day may be this far from TMDB's.
const NUMBER_REACH_DAYS: i64 = 7;

/// Shows worked out in one pass of the worker.
pub const BATCH: i64 = 40;

/// Frog's own episodes that get a time: what the calendar can show.
const TRACKED_BEFORE_DAYS: i64 = 45;
const TRACKED_AFTER_DAYS: i64 = 120;

/// Where a time came from, as `air_source` keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    AniList,
    TvMaze,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::AniList => "anilist",
            Source::TvMaze => "tvmaze",
        }
    }
}

/// One of TMDB's episodes: its numbers and TMDB's day.
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub season: i64,
    pub episode: i64,
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub at: DateTime<Utc>,
    pub source: Source,
}

/// What TMDB says of a show that finding it elsewhere needs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Show {
    pub tmdb_id: i64,
    pub name: String,
    pub original_name: String,
    pub first_year: Option<i32>,
    pub anime: bool,
    /// TMDB's external ids, only for finding the show on TVmaze.
    pub tvdb_id: Option<i64>,
    pub imdb_id: Option<String>,
}

impl Show {
    /// From a TMDB `/tv/{id}` answer with `external_ids` appended.
    pub fn from_tmdb(detail: &Value) -> Option<Show> {
        let tmdb_id = detail["id"].as_i64()?;
        let genres: Vec<i64> = detail["genres"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| g["id"].as_i64())
            .collect();
        let countries: Vec<&str> = detail["origin_country"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let text = |key: &str| detail[key].as_str().unwrap_or("").trim().to_string();
        Some(Show {
            tmdb_id,
            name: text("name"),
            original_name: text("original_name"),
            first_year: detail["first_air_date"]
                .as_str()
                .and_then(|d| d.get(..4))
                .and_then(|y| y.parse().ok()),
            anime: is_anime(&genres, &countries),
            tvdb_id: detail["external_ids"]["tvdb_id"]
                .as_i64()
                .filter(|id| *id > 0),
            imdb_id: detail["external_ids"]["imdb_id"]
                .as_str()
                .filter(|id| id.starts_with("tt"))
                .map(str::to_string),
        })
    }
}

/// TMDB's Animation genre, made in Japan.
pub fn is_anime(genre_ids: &[i64], countries: &[&str]) -> bool {
    genre_ids.contains(&16) && countries.iter().any(|c| c.eq_ignore_ascii_case("JP"))
}

/// `air_at` as the database keeps it: UTC, to the second, with a Z, so it
/// sorts and compares as text.
pub fn stamp(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// --- matching -----------------------------------------------------------------

/// One airing on AniList, with where the anime map places its media on TMDB.
#[derive(Debug, Clone, PartialEq)]
pub struct Airing {
    pub media_id: i64,
    pub episode: i64,
    pub at: DateTime<Utc>,
    /// TMDB's season for this media, and how many TMDB episodes come before
    /// its first, when the anime map says.
    pub season: Option<i64>,
    pub offset: Option<i64>,
}

fn japan_day(at: DateTime<Utc>) -> NaiveDate {
    at.with_timezone(&Tokyo).date_naive()
}

/// AniList's numbering, moved onto TMDB's by the anime map.
fn numbered(a: &Airing, w: &Wanted) -> bool {
    a.season == Some(w.season) && a.episode + a.offset.unwrap_or(0) == w.episode
}

/// AniList splits a show into one media per season or cour and numbers each
/// from 1, so an airing is found by its day in Japan: TMDB's day, else a day
/// either side for a late-night slot. Several on that day are told apart by
/// the anime map's numbering. With nothing those days, the number alone
/// decides, but only when one airing answers to it.
pub fn match_anilist(airings: &[Airing], wanted: &[Wanted]) -> Vec<Option<DateTime<Utc>>> {
    wanted.iter().map(|w| anilist_one(airings, w)).collect()
}

fn anilist_one(airings: &[Airing], w: &Wanted) -> Option<DateTime<Utc>> {
    if let Some(day) = w.date {
        for reach in [0, 1] {
            let near: Vec<&Airing> = airings
                .iter()
                .filter(|a| (japan_day(a.at) - day).num_days().abs() == reach)
                .collect();
            if !near.is_empty() {
                return pick(&near, w);
            }
        }
    }
    let by_number: Vec<&Airing> = airings.iter().filter(|a| numbered(a, w)).collect();
    match by_number.as_slice() {
        [a] if w
            .date
            .is_none_or(|d| (japan_day(a.at) - d).num_days().abs() <= NUMBER_REACH_DAYS) =>
        {
            Some(a.at)
        }
        _ => None,
    }
}

/// One moment out of the airings on a day, or none when they disagree.
fn pick(near: &[&Airing], w: &Wanted) -> Option<DateTime<Utc>> {
    if near.iter().all(|a| a.at == near[0].at) {
        return Some(near[0].at);
    }
    let by_number: Vec<_> = near.iter().filter(|a| numbered(a, w)).collect();
    if let [a] = by_number.as_slice() {
        return Some(a.at);
    }
    let this_season: Vec<_> = near.iter().filter(|a| a.season == Some(w.season)).collect();
    if let [a] = this_season.as_slice() {
        return Some(a.at);
    }
    None
}

/// TVmaze numbers most shows as TMDB does: season and number, when TVmaze's
/// day agrees with TMDB's within a day. Otherwise the day decides: the one
/// episode TVmaze has that day, or the same place among several. An episode
/// TVmaze lists without a time of day has none, whatever its `airstamp`.
pub fn match_tvmaze(eps: &[tvmaze::Episode], wanted: &[Wanted]) -> Vec<Option<DateTime<Utc>>> {
    wanted.iter().map(|w| tvmaze_one(eps, wanted, w)).collect()
}

fn tvmaze_one(eps: &[tvmaze::Episode], wanted: &[Wanted], w: &Wanted) -> Option<DateTime<Utc>> {
    let same: Vec<&tvmaze::Episode> = eps
        .iter()
        .filter(|e| e.season == w.season && e.number == Some(w.episode))
        .collect();
    if let [e] = same.as_slice() {
        let agrees = match (w.date, e.airdate) {
            (Some(ours), Some(theirs)) => (ours - theirs).num_days().abs() <= 1,
            _ => true,
        };
        if agrees {
            return e.at;
        }
    }

    let day = w.date?;
    let mut theirs: Vec<&tvmaze::Episode> = eps
        .iter()
        .filter(|e| e.airdate == Some(day) && e.number.is_some())
        .collect();
    theirs.sort_by_key(|e| (e.season, e.number));
    let mut ours: Vec<&Wanted> = wanted.iter().filter(|o| o.date == Some(day)).collect();
    ours.sort_by_key(|o| (o.season, o.episode));
    match theirs.as_slice() {
        [] => None,
        _ if theirs.len() == ours.len() => {
            let place = ours
                .iter()
                .position(|o| o.season == w.season && o.episode == w.episode)?;
            theirs[place].at
        }
        _ if theirs.iter().all(|e| e.at == theirs[0].at) => theirs[0].at,
        _ => None,
    }
}

// --- the providers ------------------------------------------------------------

/// The moments of these episodes of one show, in the same order: AniList for
/// anime that AniList has, TVmaze for the rest. An error is a provider out of
/// reach, and leaves whatever was stored before.
pub async fn resolve(
    state: &AppState,
    show: &Show,
    wanted: &[Wanted],
) -> AppResult<Vec<Option<Found>>> {
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let tag = |found: Vec<Option<DateTime<Utc>>>, source| {
        found
            .into_iter()
            .map(|at| at.map(|at| Found { at, source }))
            .collect()
    };
    if show.anime {
        let media = anilist_media(state, show, wanted).await?;
        if !media.is_empty() {
            let airings = anilist_airings(state, &media, wanted).await?;
            return Ok(tag(match_anilist(&airings, wanted), Source::AniList));
        }
    }
    let Some(id) = tvmaze_show(state, show).await? else {
        return Ok(vec![None; wanted.len()]);
    };
    let eps = tvmaze::episodes(state, id).await?;
    Ok(tag(match_tvmaze(&eps, wanted), Source::TvMaze))
}

/// One episode's moment, as `resolve` finds it. Pages read stored times; this
/// is for asking about one episode directly.
pub async fn air_timestamp(
    state: &AppState,
    tmdb_id: i64,
    season: i64,
    episode: i64,
    tmdb_air_date: Option<NaiveDate>,
) -> AppResult<Option<DateTime<Utc>>> {
    let show = show_facts(state, tmdb_id).await?;
    let wanted = [Wanted {
        season,
        episode,
        date: tmdb_air_date,
    }];
    Ok(resolve(state, &show, &wanted)
        .await?
        .pop()
        .flatten()
        .map(|f| f.at))
}

/// An AniList media and where the anime map places it on TMDB.
#[derive(Debug, Clone, PartialEq)]
struct Media {
    id: i64,
    season: Option<i64>,
    offset: Option<i64>,
}

/// The show's AniList media: from the anime map, and, when a season it is
/// asked about has none there, from a search by name.
async fn anilist_media(state: &AppState, show: &Show, wanted: &[Wanted]) -> AppResult<Vec<Media>> {
    let mapped: Vec<(i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT anilist_id, tmdb_season, tmdb_offset FROM anime_map
          WHERE tmdb_kind = 'tv' AND tmdb_id = ?",
    )
    .bind(show.tmdb_id)
    .fetch_all(&state.pool)
    .await?;
    let mut media: Vec<Media> = mapped
        .iter()
        .map(|&(id, season, offset)| Media { id, season, offset })
        .collect();
    let gap = wanted
        .iter()
        .any(|w| w.season > 0 && !mapped.iter().any(|m| m.1 == Some(w.season)));
    if gap {
        for id in searched_media(state, show).await? {
            if !media.iter().any(|m| m.id == id) {
                media.push(Media {
                    id,
                    season: None,
                    offset: None,
                });
            }
        }
    }
    Ok(media)
}

/// AniList media found by the show's name, kept in `airtime_show`.
async fn searched_media(state: &AppState, show: &Show) -> AppResult<Vec<i64>> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT anilist_ids, anilist_checked_at FROM airtime_show WHERE tmdb_id = ?",
    )
    .bind(show.tmdb_id)
    .fetch_optional(&state.pool)
    .await?;
    let (kept, checked) = row.unwrap_or((None, None));
    if !older_than(checked.as_deref(), Duration::days(SEARCH_RETRY_DAYS)) {
        return Ok(kept
            .and_then(|k| serde_json::from_str(&k).ok())
            .unwrap_or_default());
    }
    let found = search_anilist(state, show).await?;
    sqlx::query(
        "INSERT INTO airtime_show (tmdb_id, anilist_ids, anilist_checked_at) VALUES (?, ?, ?)
         ON CONFLICT (tmdb_id) DO UPDATE SET
            anilist_ids = excluded.anilist_ids,
            anilist_checked_at = excluded.anilist_checked_at",
    )
    .bind(show.tmdb_id)
    .bind(serde_json::to_string(&found)?)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(found)
}

const SEARCH_QUERY: &str = "query ($search: String) {
  Page(perPage: 10) {
    media(search: $search, type: ANIME, sort: SEARCH_MATCH) {
      id format startDate { year } synonyms title { romaji english native }
    }
  }
}";

/// Series on AniList that carry the show's name (or begin with it, as a
/// "2nd Season" does), from its first year on, and that the anime map does
/// not give to another TMDB show.
async fn search_anilist(state: &AppState, show: &Show) -> AppResult<Vec<i64>> {
    let names: Vec<String> = [&show.name, &show.original_name]
        .into_iter()
        .map(|n| squash(n))
        .filter(|n| !n.is_empty())
        .collect();
    let api = Api::public(state);
    for term in [&show.name, &show.original_name] {
        if term.is_empty() {
            continue;
        }
        let data = api
            .graphql("", SEARCH_QUERY, json!({ "search": term }))
            .await?;
        let ids = search_hits(&data, &names, show.first_year);
        if ids.is_empty() {
            continue;
        }
        let sql = format!(
            "SELECT anilist_id FROM anime_map
              WHERE tmdb_id IS NOT NULL AND tmdb_id != ? AND anilist_id IN ({})",
            ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
        );
        let mut query = sqlx::query_as::<_, (i64,)>(&sql).bind(show.tmdb_id);
        for id in &ids {
            query = query.bind(id);
        }
        let elsewhere = query.fetch_all(&state.pool).await?;
        return Ok(ids
            .into_iter()
            .filter(|id| !elsewhere.iter().any(|e| e.0 == *id))
            .collect());
    }
    Ok(Vec::new())
}

/// A name folded to its letters and digits, one space between words, so a
/// curly apostrophe or a colon does not keep two spellings of a name apart.
fn squash(name: &str) -> String {
    fold(name)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The ids in a search answer that name this show.
fn search_hits(data: &Value, names: &[String], first_year: Option<i32>) -> Vec<i64> {
    data["Page"]["media"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| matches!(m["format"].as_str(), Some("TV" | "TV_SHORT" | "ONA")))
        .filter(|m| match (m["startDate"]["year"].as_i64(), first_year) {
            (Some(year), Some(first)) => year >= i64::from(first) - 1,
            _ => true,
        })
        .filter(|m| {
            let titles = ["romaji", "english", "native"]
                .iter()
                .filter_map(|k| m["title"][*k].as_str())
                .chain(
                    m["synonyms"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                );
            titles.map(squash).any(|t| {
                names
                    .iter()
                    .any(|n| t == *n || t.starts_with(&format!("{n} ")))
            })
        })
        .filter_map(|m| m["id"].as_i64())
        .collect()
}

const AIRINGS_QUERY: &str = "query ($ids: [Int], $from: Int, $page: Int) {
  Page(page: $page, perPage: 50) {
    pageInfo { hasNextPage }
    airingSchedules(mediaId_in: $ids, airingAt_greater: $from, sort: TIME) {
      mediaId episode airingAt
    }
  }
}";

/// Pages of airings read at most, from two days before the earliest episode.
const AIRING_PAGES: i64 = 6;

async fn anilist_airings(
    state: &AppState,
    media: &[Media],
    wanted: &[Wanted],
) -> AppResult<Vec<Airing>> {
    let earliest = wanted
        .iter()
        .filter_map(|w| w.date)
        .min()
        .unwrap_or_else(|| Utc::now().date_naive());
    let from = (earliest - Duration::days(2))
        .and_hms_opt(0, 0, 0)
        .map_or(0, |d| d.and_utc().timestamp());
    let ids: Vec<i64> = media.iter().map(|m| m.id).collect();
    let api = Api::public(state);
    let mut out = Vec::new();
    for page in 1..=AIRING_PAGES {
        let data = api
            .graphql(
                "",
                AIRINGS_QUERY,
                json!({ "ids": ids, "from": from, "page": page }),
            )
            .await?;
        for a in data["Page"]["airingSchedules"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let (Some(media_id), Some(episode), Some(at)) = (
                a["mediaId"].as_i64(),
                a["episode"].as_i64(),
                a["airingAt"]
                    .as_i64()
                    .and_then(|t| DateTime::from_timestamp(t, 0)),
            ) else {
                continue;
            };
            let placed = media.iter().find(|m| m.id == media_id);
            out.push(Airing {
                media_id,
                episode,
                at,
                season: placed.and_then(|m| m.season),
                offset: placed.and_then(|m| m.offset),
            });
        }
        if data["Page"]["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break;
        }
    }
    Ok(out)
}

/// The show's TVmaze id: kept, else looked up by TMDB's external ids.
async fn tvmaze_show(state: &AppState, show: &Show) -> AppResult<Option<i64>> {
    let row: Option<(Option<i64>, Option<String>)> =
        sqlx::query_as("SELECT tvmaze_id, tvmaze_checked_at FROM airtime_show WHERE tmdb_id = ?")
            .bind(show.tmdb_id)
            .fetch_optional(&state.pool)
            .await?;
    let (kept, checked) = row.unwrap_or((None, None));
    if kept.is_some() {
        return Ok(kept);
    }
    if !older_than(checked.as_deref(), Duration::days(LOOKUP_RETRY_DAYS)) {
        return Ok(None);
    }
    let found = tvmaze::find(state, show.tvdb_id, show.imdb_id.as_deref()).await?;
    sqlx::query(
        "INSERT INTO airtime_show (tmdb_id, tvmaze_id, tvmaze_checked_at) VALUES (?, ?, ?)
         ON CONFLICT (tmdb_id) DO UPDATE SET
            tvmaze_id = excluded.tvmaze_id,
            tvmaze_checked_at = excluded.tvmaze_checked_at",
    )
    .bind(show.tmdb_id)
    .bind(found)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(found)
}

/// Whether an RFC 3339 moment is missing or further back than `age`.
fn older_than(at: Option<&str>, age: Duration) -> bool {
    at.and_then(|a| DateTime::parse_from_rfc3339(a).ok())
        .is_none_or(|a| Utc::now() - a.with_timezone(&Utc) > age)
}

// --- what is kept -------------------------------------------------------------

/// Keep what TMDB says of a show, for finding it elsewhere. How it was found
/// stays as it was.
pub async fn remember(pool: &SqlitePool, show: &Show) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO airtime_show (tmdb_id, name, original_name, first_year, anime, tvdb_id, imdb_id)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (tmdb_id) DO UPDATE SET
            name = excluded.name,
            original_name = excluded.original_name,
            first_year = excluded.first_year,
            anime = excluded.anime,
            tvdb_id = excluded.tvdb_id,
            imdb_id = excluded.imdb_id",
    )
    .bind(show.tmdb_id)
    .bind(&show.name)
    .bind(&show.original_name)
    .bind(show.first_year)
    .bind(show.anime)
    .bind(show.tvdb_id)
    .bind(&show.imdb_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Work this show's times out on the next pass, not in a few hours.
pub async fn mark_due(pool: &SqlitePool, tmdb_id: i64) -> AppResult<()> {
    sqlx::query("UPDATE airtime_show SET times_at = NULL WHERE tmdb_id = ?")
        .bind(tmdb_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// (name, original_name, first_year, anime, tvdb_id, imdb_id)
type ShowRow = (
    Option<String>,
    Option<String>,
    Option<i32>,
    bool,
    Option<i64>,
    Option<String>,
);

/// The show as last kept, or as TMDB has it now for one Frog tracks.
async fn show_facts(state: &AppState, tmdb_id: i64) -> AppResult<Show> {
    if let Some((detail, _)) = state.tmdb.kept("tv", tmdb_id).await {
        if let Some(show) = Show::from_tmdb(&detail) {
            remember(&state.pool, &show).await?;
            return Ok(show);
        }
    }
    let row: Option<ShowRow> = sqlx::query_as(
        "SELECT name, original_name, first_year, anime, tvdb_id, imdb_id
           FROM airtime_show WHERE tmdb_id = ?",
    )
    .bind(tmdb_id)
    .fetch_optional(&state.pool)
    .await?;
    if let Some((Some(name), original, first_year, anime, tvdb_id, imdb_id)) = row {
        return Ok(Show {
            tmdb_id,
            name,
            original_name: original.unwrap_or_default(),
            first_year,
            anime,
            tvdb_id,
            imdb_id,
        });
    }
    let detail = state.tmdb.tv(tmdb_id).await?;
    let show = Show::from_tmdb(&detail).unwrap_or(Show {
        tmdb_id,
        ..Show::default()
    });
    remember(&state.pool, &show).await?;
    Ok(show)
}

/// Frog's title for a TMDB show, when TMDB is where its episodes come from.
async fn tracked_title(pool: &SqlitePool, tmdb_id: i64) -> AppResult<Option<i64>> {
    let id = ExternalId::tmdb("tv", tmdb_id);
    let Some(title_id) = catalog::find(pool, &id).await? else {
        return Ok(None);
    };
    Ok((catalog::primary(pool, title_id).await? == Some(id)).then_some(title_id))
}

fn day(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()
}

/// The episodes of one show that want a time: those on the Airing page, and
/// Frog's own near today, with the dated seasons' undated episodes after them.
async fn wanted_for(
    pool: &SqlitePool,
    tmdb_id: i64,
    title_id: Option<i64>,
) -> AppResult<Vec<Wanted>> {
    let mut all: BTreeMap<(i64, i64), Option<NaiveDate>> = BTreeMap::new();
    let listed: Vec<(i64, i64, String)> =
        sqlx::query_as("SELECT season, episode, air_date FROM airing_episode WHERE tmdb_id = ?")
            .bind(tmdb_id)
            .fetch_all(pool)
            .await?;
    for (season, episode, date) in listed {
        all.insert((season, episode), day(&date));
    }
    if let Some(title_id) = title_id {
        let ours: Vec<(i64, i64, Option<String>)> = sqlx::query_as(
            "SELECT season_number, episode_number, air_date FROM episode
              WHERE title_id = ?1 AND season_number > 0
                AND COALESCE(air_source, '') != 'restore'
                AND (air_date BETWEEN date('now', ?2) AND date('now', ?3)
                     OR (air_date IS NULL AND season_number >= (
                           SELECT MAX(season_number) FROM episode
                            WHERE title_id = ?1 AND air_date IS NOT NULL)))",
        )
        .bind(title_id)
        .bind(format!("-{TRACKED_BEFORE_DAYS} day"))
        .bind(format!("+{TRACKED_AFTER_DAYS} day"))
        .fetch_all(pool)
        .await?;
        for (season, episode, date) in ours {
            let date = date.as_deref().and_then(day);
            let slot = all.entry((season, episode)).or_insert(date);
            if slot.is_none() {
                *slot = date;
            }
        }
    }
    Ok(all
        .into_iter()
        .map(|((season, episode), date)| Wanted {
            season,
            episode,
            date,
        })
        .collect())
}

/// Write one show's moments where they are shown. An episode with none
/// found loses any it had: it may have moved.
async fn write(
    pool: &SqlitePool,
    tmdb_id: i64,
    title_id: Option<i64>,
    wanted: &[Wanted],
    found: &[Option<Found>],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for (w, f) in wanted.iter().zip(found) {
        let at = f.as_ref().map(|f| stamp(f.at));
        let source = f.as_ref().map(|f| f.source.as_str());
        sqlx::query(
            "UPDATE airing_episode SET air_at = ?, air_source = ?
              WHERE tmdb_id = ? AND season = ? AND episode = ?",
        )
        .bind(&at)
        .bind(source)
        .bind(tmdb_id)
        .bind(w.season)
        .bind(w.episode)
        .execute(&mut *tx)
        .await?;
        if let Some(title_id) = title_id {
            sqlx::query(
                "UPDATE episode SET air_at = ?, air_source = ?
                  WHERE title_id = ? AND season_number = ? AND episode_number = ?
                    AND COALESCE(air_source, '') != 'restore'",
            )
            .bind(&at)
            .bind(source)
            .bind(title_id)
            .bind(w.season)
            .bind(w.episode)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "INSERT INTO airtime_show (tmdb_id, times_at) VALUES (?, ?)
         ON CONFLICT (tmdb_id) DO UPDATE SET times_at = excluded.times_at",
    )
    .bind(tmdb_id)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Work out the times of one show and store them. Returns how many of its
/// episodes have one.
pub async fn refresh_show(state: &AppState, tmdb_id: i64) -> AppResult<usize> {
    let title_id = tracked_title(&state.pool, tmdb_id).await?;
    let wanted = wanted_for(&state.pool, tmdb_id, title_id).await?;
    let found = if wanted.is_empty() {
        Vec::new()
    } else {
        let show = show_facts(state, tmdb_id).await?;
        resolve(state, &show, &wanted).await?
    };
    write(&state.pool, tmdb_id, title_id, &wanted, &found).await?;
    Ok(found.iter().flatten().count())
}

/// Shows whose times are due: everything on the Airing page, and every show
/// in someone's library with episodes near today, at once when someone added
/// it since its times were last worked out. Frog's own first, then by
/// popularity.
pub async fn refresh_due(state: &AppState) -> usize {
    let stale = (Utc::now() - Duration::hours(TIMES_HOURS)).to_rfc3339();
    let due: Vec<(i64,)> = match sqlx::query_as(
        "SELECT d.tmdb_id FROM (
             SELECT s.tmdb_id AS tmdb_id, s.popularity AS rank, NULL AS since FROM airing_show s
             UNION ALL
             SELECT CAST(substr(x.external_id, 4) AS INTEGER), 1e12,
                    (SELECT MAX(l.added_at) FROM library_entry l WHERE l.title_id = t.id)
               FROM title t
               JOIN title_external x ON x.title_id = t.id AND x.provider = 'tmdb'
                                    AND x.is_primary = 1
              WHERE t.kind = 'tv' AND t.merged_into IS NULL
                AND EXISTS (SELECT 1 FROM library_entry l WHERE l.title_id = t.id)
                AND EXISTS (SELECT 1 FROM episode e WHERE e.title_id = t.id
                             AND e.air_date BETWEEN date('now', ?1) AND date('now', ?2))
           ) d
           LEFT JOIN airtime_show a ON a.tmdb_id = d.tmdb_id
          GROUP BY d.tmdb_id
         HAVING MAX(a.times_at) IS NULL OR MAX(a.times_at) < ?3
             OR MAX(a.times_at) < MAX(d.since)
          ORDER BY MAX(d.since) IS NULL, MAX(a.times_at) IS NOT NULL, MAX(d.rank) DESC
          LIMIT ?4",
    )
    .bind(format!("-{TRACKED_BEFORE_DAYS} day"))
    .bind(format!("+{TRACKED_AFTER_DAYS} day"))
    .bind(&stale)
    .bind(BATCH)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "could not list shows for air times");
            return 0;
        }
    };
    let mut done = 0;
    for (tmdb_id,) in due {
        match refresh_show(state, tmdb_id).await {
            Ok(_) => done += 1,
            Err(crate::error::AppError::NotFound(_)) => {
                // TMDB no longer has it: nothing to time, and not asked again soon.
                let _ = write(&state.pool, tmdb_id, None, &[], &[]).await;
            }
            Err(e) => {
                // A provider out of reach: the rest would fail the same way.
                tracing::warn!(tmdb_id, error = %e, "air times not read");
                break;
            }
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn want(season: i64, episode: i64, date: &str) -> Wanted {
        Wanted {
            season,
            episode,
            date: Some(d(date)),
        }
    }

    fn airing(media_id: i64, episode: i64, when: &str, season: i64, offset: Option<i64>) -> Airing {
        Airing {
            media_id,
            episode,
            at: at(when),
            season: Some(season),
            offset,
        }
    }

    #[test]
    fn a_later_season_is_found_by_its_day_in_japan() {
        // Black Clover: TMDB's season 2 is AniList's "2nd Season", numbered from 1.
        let airings = [
            airing(195604, 6, "2026-11-07T14:00:00Z", 2, None),
            airing(195604, 7, "2026-11-14T14:00:00Z", 2, None),
        ];
        let found = match_anilist(&airings, &[want(2, 7, "2026-11-14")]);
        assert_eq!(found, [Some(at("2026-11-14T14:00:00Z"))]);
    }

    #[test]
    fn a_late_night_slot_lands_on_the_next_day_in_japan() {
        // Ranma ½ (2024): TMDB has one season of 36; AniList's third cour
        // starts at TMDB's 25th. 00:55 on Sunday in Japan is Saturday in UTC.
        let airings = [
            airing(209872, 6, "2026-11-07T15:55:00Z", 1, Some(24)),
            airing(209872, 7, "2026-11-14T15:55:00Z", 1, Some(24)),
        ];
        // TMDB writes the Japanese day...
        assert_eq!(
            match_anilist(&airings, &[want(1, 31, "2026-11-15")]),
            [Some(at("2026-11-14T15:55:00Z"))]
        );
        // ...or the evening before, which a day's reach still finds.
        assert_eq!(
            match_anilist(&airings, &[want(1, 31, "2026-11-14")]),
            [Some(at("2026-11-14T15:55:00Z"))]
        );
    }

    #[test]
    fn two_airings_on_one_day_are_told_apart_by_the_anime_map() {
        let airings = [
            airing(1, 7, "2026-11-14T14:00:00Z", 1, Some(12)),
            airing(2, 19, "2026-11-14T05:00:00Z", 2, None),
        ];
        assert_eq!(
            match_anilist(&airings, &[want(1, 19, "2026-11-14")]),
            [Some(at("2026-11-14T14:00:00Z"))]
        );
        // Unmapped media on the same day: no way to tell, so no time.
        let unknown = [
            Airing {
                season: None,
                ..airings[0].clone()
            },
            Airing {
                season: None,
                ..airings[1].clone()
            },
        ];
        assert_eq!(
            match_anilist(&unknown, &[want(1, 19, "2026-11-14")]),
            [None]
        );
    }

    #[test]
    fn the_number_decides_only_when_nothing_is_near_and_one_answers() {
        let airings = [airing(1, 3, "2026-11-21T14:00:00Z", 1, None)];
        // TMDB has no day yet.
        let undated = Wanted {
            season: 1,
            episode: 3,
            date: None,
        };
        assert_eq!(
            match_anilist(&airings, &[undated]),
            [Some(at("2026-11-21T14:00:00Z"))]
        );
        // TMDB's day is a month away: a different episode, so no time.
        assert_eq!(match_anilist(&airings, &[want(1, 3, "2026-12-21")]), [None]);
        // Two media both numbered 3 in this season: ambiguous.
        let two = [
            airings[0].clone(),
            airing(2, 3, "2026-11-22T14:00:00Z", 1, None),
        ];
        let undated = Wanted {
            season: 1,
            episode: 3,
            date: None,
        };
        assert_eq!(match_anilist(&two, &[undated]), [None]);
    }

    fn tm(season: i64, number: i64, airdate: &str, stamp: Option<&str>) -> tvmaze::Episode {
        tvmaze::Episode {
            season,
            number: Some(number),
            airdate: Some(d(airdate)),
            at: stamp.map(at),
        }
    }

    #[test]
    fn tvmaze_matches_by_number_when_the_day_agrees() {
        // The Gilded Age: Sunday 21:00 Eastern is Monday 02:00 UTC.
        let eps = [
            tm(4, 2, "2026-11-08", Some("2026-11-09T02:00:00Z")),
            tm(4, 3, "2026-11-15", Some("2026-11-16T02:00:00Z")),
        ];
        assert_eq!(
            match_tvmaze(&eps, &[want(4, 3, "2026-11-15")]),
            [Some(at("2026-11-16T02:00:00Z"))]
        );
    }

    #[test]
    fn tvmaze_without_a_time_of_day_gives_none() {
        // parse_episodes leaves `at` empty for TVmaze's 12:00 stand-in.
        let eps = [tm(1, 1, "2026-11-20", None)];
        assert_eq!(match_tvmaze(&eps, &[want(1, 1, "2026-11-20")]), [None]);
    }

    #[test]
    fn tvmaze_numbered_otherwise_falls_back_on_the_day() {
        // A talk show TVmaze numbers by year and TMDB by run.
        let eps = [
            tm(2026, 210, "2026-11-16", Some("2026-11-17T04:35:00Z")),
            tm(2026, 211, "2026-11-17", Some("2026-11-18T04:35:00Z")),
        ];
        let wanted = [want(12, 40, "2026-11-16"), want(12, 41, "2026-11-17")];
        assert_eq!(
            match_tvmaze(&eps, &wanted),
            [
                Some(at("2026-11-17T04:35:00Z")),
                Some(at("2026-11-18T04:35:00Z"))
            ]
        );
        // The same numbers weeks apart are not the same episode.
        let far = [tm(12, 40, "2026-12-28", Some("2026-12-29T04:35:00Z"))];
        assert_eq!(match_tvmaze(&far, &[want(12, 40, "2026-11-16")]), [None]);
    }

    #[test]
    fn two_episodes_on_one_day_pair_in_order() {
        let eps = [
            tm(2, 2, "2026-11-12", Some("2026-11-13T02:30:00Z")),
            tm(2, 1, "2026-11-12", Some("2026-11-13T02:00:00Z")),
        ];
        // TMDB numbers them as a later part of the season.
        let wanted = [want(1, 11, "2026-11-12"), want(1, 12, "2026-11-12")];
        assert_eq!(
            match_tvmaze(&eps, &wanted),
            [
                Some(at("2026-11-13T02:00:00Z")),
                Some(at("2026-11-13T02:30:00Z"))
            ]
        );
    }

    #[test]
    fn anime_is_animation_from_japan() {
        assert!(is_anime(&[16, 10759], &["JP"]));
        assert!(!is_anime(&[16], &["US"]));
        assert!(!is_anime(&[18], &["JP"]));
    }

    #[test]
    fn a_show_reads_from_tmdb() {
        let detail = json!({
            "id": 73223, "name": "Black Clover", "original_name": "ブラッククローバー",
            "first_air_date": "2017-10-03", "origin_country": ["JP"],
            "genres": [{ "id": 16, "name": "Animation" }],
            "external_ids": { "tvdb_id": 331753, "imdb_id": "tt7441658" }
        });
        let show = Show::from_tmdb(&detail).unwrap();
        assert!(show.anime);
        assert_eq!(show.first_year, Some(2017));
        assert_eq!(show.tvdb_id, Some(331753));
        assert_eq!(show.imdb_id.as_deref(), Some("tt7441658"));
    }

    #[test]
    fn a_search_keeps_series_that_carry_the_name() {
        let data = json!({ "Page": { "media": [
            { "id": 195604, "format": "TV", "startDate": { "year": 2026 },
              "title": { "romaji": "Black Clover 2nd Season", "english": "Black Clover Season 2" } },
            { "id": 131680, "format": "MOVIE", "startDate": { "year": 2023 },
              "title": { "romaji": "Black Clover: Mahou Tei no Ken" } },
            { "id": 110318, "format": "ONA", "startDate": { "year": 2019 },
              "title": { "romaji": "Mugyutto! Black Clover" } },
            { "id": 1, "format": "TV", "startDate": { "year": 2001 },
              "title": { "romaji": "Black Clover" } }
        ]}});
        let names = vec![squash("Black Clover")];
        assert_eq!(search_hits(&data, &names, Some(2017)), [195604]);
    }

    #[test]
    fn names_compare_without_their_punctuation() {
        assert_eq!(
            squash("The Guy She Was Interested In Wasn't a Guy at All"),
            squash("The Guy She Was Interested In Wasn’t a Guy At All")
        );
    }

    #[test]
    fn stamps_sort_as_text() {
        assert_eq!(stamp(at("2026-11-16T02:00:00Z")), "2026-11-16T02:00:00Z");
    }
}
