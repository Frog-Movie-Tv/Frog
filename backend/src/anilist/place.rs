//! Where an AniList entry belongs in TMDB's terms, and how sure that is.
//!
//! Every way of placing an entry produces evidence with a confidence, and
//! only a confident placement is applied; the rest are kept with the reason
//! and the closest candidates, for the person to settle. In order:
//!
//! 1. Fribb's anime-lists: the TMDB entry, season and offset by id.
//! 2. Kometa's Anime-IDs: TVDB's season and offset, turned into TMDB's.
//! 3. Air dates: AniList's start date against the episodes of the show the
//!    entry hangs off, specials included. Precise, and indifferent to names.
//! 4. The prequel chain: count the episodes of the seasons before it.
//! 5. Name and year on TMDB, from every title AniList has, cleaned of the
//!    decorations TMDB does not use.
//!
//! The id lists are checked against the air dates too, so a list that has
//! drifted from TMDB is caught rather than trusted.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{NaiveDate, Utc};
use serde_json::{json, Value};

use super::{media_name, remember_media, Api, Remote, ANIMATION, MEDIA_QUERY};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::models;

/// Bumped whenever placing changes, so automatic placements made by an
/// older version are looked at again (and repaired when certain).
pub const RESOLVER: i64 = 2;

/// Below this, a placement is a suggestion for the person, not a fact.
pub const CONFIDENT: i64 = 70;

/// (tvdb_id, tvdb_season, tvdb_epoffset, imdb_id,
///  tmdb_kind, tmdb_id, tmdb_season, tmdb_offset)
type MapRow = (
    Option<i64>,
    Option<i64>,
    i64,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

/// Where an AniList entry lives in Frog. Season 0 is TMDB's specials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Movie(i64),
    Show {
        tmdb_id: i64,
        season: i64,
        offset: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Matched,
    /// Candidates exist, none of them certain.
    NotConfident,
    /// Nothing on TMDB looks like it.
    NoCandidate,
    /// Not out yet, and TMDB has nowhere for it.
    Upcoming,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Matched => "matched",
            Outcome::NotConfident => "not_confident",
            Outcome::NoCandidate => "no_candidate",
            Outcome::Upcoming => "upcoming",
        }
    }
}

/// What placing one entry came to.
#[derive(Debug, Clone)]
pub struct Placement {
    pub target: Option<Target>,
    pub outcome: Outcome,
    pub method: Option<&'static str>,
    pub confidence: i64,
    pub note: Option<String>,
    /// The show it belongs to, when known even though it is not placed (a
    /// season TMDB has not listed yet).
    pub show: Option<i64>,
    pub candidates: Vec<Value>,
    /// Every piece of evidence weighed, for diagnostics.
    pub evidence: Vec<String>,
}

/// One piece of evidence for a placement.
#[derive(Debug, Clone)]
struct Found {
    target: Target,
    method: &'static str,
    confidence: i64,
    note: Option<String>,
}

// --- titles --------------------------------------------------------------------

/// Romaji's long vowels and the accents of European titles, as plain letters.
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ý' | 'ÿ' => 'y',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

/// Letters and digits only, lower case, accents folded: "Frieren: Beyond
/// Journey's End" and "FRIEREN - Beyond Journey’s End" are the same name.
pub fn plain(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .map(fold)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// A title without what AniList adds and TMDB does not: "(ONA)", "(2018)",
/// "Season 2", "2nd Season", "Part 2", "Cour 3", "The Final Season".
pub fn clean(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let words: Vec<String> = out
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| c == ':' || c == '-' || c == ',')
                .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect();
    let is_number = |w: &str| w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty();
    let is_ordinal = |w: &str| {
        let w = w.to_lowercase();
        ["st", "nd", "rd", "th"]
            .iter()
            .any(|s| w.strip_suffix(s).is_some_and(is_number))
            || matches!(
                w.as_str(),
                "second" | "third" | "fourth" | "fifth" | "final"
            )
    };
    let mut kept: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i].to_lowercase();
        let next = words.get(i + 1).map(|n| n.to_lowercase());
        let skip = match (w.as_str(), next.as_deref()) {
            ("season" | "part" | "cour", Some(n)) if is_number(n) || is_ordinal(n) => 2,
            (_, Some("season" | "cour" | "part")) if is_ordinal(&w) => 2,
            ("the", Some("final")) => {
                if words
                    .get(i + 2)
                    .is_some_and(|w| w.eq_ignore_ascii_case("season"))
                {
                    3
                } else {
                    0
                }
            }
            ("season" | "part", None) => 1,
            // "Sword Art Online the Movie: Integral Domain" is the film
            // "Sword Art Online: Integral Domain".
            ("the", Some("movie")) => 2,
            ("movie" | "gekijouban" | "gekijoban", _) => 1,
            _ => 0,
        };
        if skip > 0 {
            i += skip;
            continue;
        }
        kept.push(words[i].clone());
        i += 1;
    }
    kept.join(" ")
}

/// For films: drop "the Movie" and brackets, but keep any part number.
fn clean_keep_numbers(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    // Lower case is all `plain` keeps anyway.
    out.to_lowercase()
        .replace("the movie", "")
        .replace("gekijouban", "")
}

/// The part before a subtitle: "One Piece: Heart of Gold" is "One Piece".
fn base(name: &str) -> Option<String> {
    let cut = name.find(':').or_else(|| name.find(" - "))?;
    let head = name[..cut].trim();
    (plain(head).chars().count() >= 3).then(|| head.to_string())
}

/// How alike two plain names are, 0.0 to 1.0, by edit distance.
fn similarity(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(prev + usize::from(ca != cb));
            prev = here;
        }
    }
    1.0 - row[b.len()] as f64 / a.len().max(b.len()) as f64
}

/// Every name the entry goes by, as AniList gives them and cleaned, for
/// comparing against TMDB's names.
fn names(remote: &Remote) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |n: String| {
        let p = plain(&n);
        if !p.is_empty() && !out.contains(&p) {
            out.push(p);
        }
    };
    for t in remote.titles.iter().chain(&remote.synonyms) {
        add(t.clone());
        add(numbered(t));
        add(numbered(&clean_keep_numbers(t)));
        // Without its season or part a film would claim the first film of a
        // series; "Infinity Castle Part 2" is not "Infinity Castle".
        if !is_film(remote) {
            add(clean(t));
        }
    }
    out
}

/// A title with "Part 2", "Season 2" or "2nd Season" as just "2", the way TMDB
/// tends to number sequels.
fn numbered(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i].to_lowercase();
        let next = words.get(i + 1).map(|n| n.to_lowercase());
        let number = |w: &str| {
            let digits: String = w.chars().take_while(|c| c.is_ascii_digit()).collect();
            (!digits.is_empty()).then_some(digits)
        };
        match (w.as_str(), next.as_deref()) {
            ("season" | "part" | "cour", Some(n)) if number(n).is_some() => {
                out.push(number(n).unwrap_or_default());
                i += 2;
            }
            (n, Some("season" | "part" | "cour")) if number(n).is_some() => {
                out.push(number(n).unwrap_or_default());
                i += 2;
            }
            _ => {
                out.push(words[i].to_string());
                i += 1;
            }
        }
    }
    out.join(" ")
}

/// What to type into TMDB's search, best first and few: every query costs a
/// request.
fn queries(remote: &Remote) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |q: String| {
        let q = q.trim().to_string();
        if q.chars().count() >= 2 && !out.iter().any(|o| plain(o) == plain(&q)) {
            out.push(q);
        }
    };
    for t in &remote.titles {
        add(clean(t));
    }
    for t in &remote.titles {
        add(t.clone());
    }
    // Synonyms in Latin script only: TMDB's search does well with English and
    // romaji, and the native title is already above.
    for s in remote.synonyms.iter().filter(|s| s.is_ascii()).take(2) {
        add(clean(s));
    }
    out.truncate(5);
    out
}

fn is_film(remote: &Remote) -> bool {
    remote.format.as_deref() == Some("MOVIE")
}

fn is_special(remote: &Remote) -> bool {
    matches!(remote.format.as_deref(), Some("SPECIAL" | "OVA" | "MUSIC"))
}

/// The formats that make up a series' main run, as opposed to films, OVAs
/// and specials hanging off it.
pub fn main_run(format: Option<&str>) -> bool {
    matches!(format, Some("TV" | "TV_SHORT" | "ONA"))
}

fn unreleased(remote: &Remote) -> bool {
    remote.media_status.as_deref() == Some("NOT_YET_RELEASED")
}

fn days_apart(a: &str, b: &str) -> Option<i64> {
    let a = NaiveDate::parse_from_str(a.get(..10)?, "%Y-%m-%d").ok()?;
    let b = NaiveDate::parse_from_str(b.get(..10)?, "%Y-%m-%d").ok()?;
    Some((a - b).num_days().abs())
}

// --- TMDB's side ---------------------------------------------------------------

async fn map_row(state: &AppState, media_id: i64) -> AppResult<Option<MapRow>> {
    Ok(sqlx::query_as(
        "SELECT tvdb_id, tvdb_season, tvdb_epoffset, imdb_id,
                tmdb_kind, tmdb_id, tmdb_season, tmdb_offset
           FROM anime_map WHERE anilist_id = ?",
    )
    .bind(media_id)
    .fetch_optional(&state.pool)
    .await?)
}

/// (season, episode count) of a TMDB show, specials included.
async fn tmdb_seasons(state: &AppState, tmdb_id: i64) -> AppResult<Vec<(i64, i64)>> {
    let detail = state.tmdb.tv(tmdb_id).await?;
    let mut seasons: Vec<(i64, i64)> = models::seasons(&detail)
        .into_iter()
        .map(|s| (s.season_number, s.episode_count))
        .collect();
    seasons.sort_unstable();
    Ok(seasons)
}

/// (episode number, air date) of one TMDB season. A season TMDB does not
/// have is simply empty.
async fn season_dates(
    state: &AppState,
    tmdb_id: i64,
    season: i64,
) -> AppResult<Vec<(i64, Option<String>)>> {
    match state.tmdb.season(tmdb_id, season).await {
        Ok(payload) => Ok(models::episodes_of_season(&payload)
            .into_iter()
            .map(|e| (e.episode_number, e.air_date))
            .collect()),
        Err(AppError::NotFound(_)) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// When the episode (or film) a placement starts at came out, per TMDB.
async fn starts_on(state: &AppState, target: Target) -> AppResult<Option<String>> {
    match target {
        Target::Movie(id) => {
            let detail = state.tmdb.movie(id).await?;
            Ok(models::str_at(&detail, "release_date").filter(|d| !d.is_empty()))
        }
        Target::Show {
            tmdb_id,
            season,
            offset,
        } => Ok(season_dates(state, tmdb_id, season)
            .await?
            .into_iter()
            .find(|(ep, _)| *ep == offset + 1)
            .and_then(|(_, date)| date)),
    }
}

/// How much more (or less) to believe a placement once its date is known.
/// Films get more room: TMDB often dates a film by its festival premiere.
async fn checked(state: &AppState, remote: &Remote, mut found: Found) -> AppResult<Found> {
    let Some(start) = remote.start.as_deref() else {
        return Ok(found);
    };
    let Some(theirs) = starts_on(state, found.target).await? else {
        return Ok(found);
    };
    let film = matches!(found.target, Target::Movie(_));
    match days_apart(start, &theirs) {
        Some(d) if d <= 3 => found.confidence = (found.confidence + 10).min(97),
        // A week out is a neighbouring episode, not this one.
        Some(d) if !film && d <= 45 => found.confidence -= 15,
        // TMDB often dates a film by its festival premiere.
        Some(d) if film && d <= 120 => {}
        Some(_) => {
            found.confidence = found.confidence.min(45);
            found.note = Some(format!(
                "TMDB dates it {theirs}, AniList {start}: they do not agree."
            ));
        }
        None => {}
    }
    Ok(found)
}

/// The TMDB title another database's id points at. `Ok(None)` is TMDB saying
/// it knows no such id; TMDB being unreachable is an error, so a bad moment
/// never turns a whole list into "needs a place".
async fn tmdb_by_external(
    state: &AppState,
    id: &str,
    source: &str,
) -> AppResult<Option<(String, i64)>> {
    let found = match state.tmdb.find(id, source).await {
        Ok(v) => v,
        Err(AppError::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    let first = |key: &str| found[key].as_array().and_then(|a| a.first()).cloned();
    if let Some(m) = first("movie_results") {
        return Ok(m["id"].as_i64().map(|id| ("movie".into(), id)));
    }
    if let Some(t) = first("tv_results") {
        return Ok(t["id"].as_i64().map(|id| ("tv".into(), id)));
    }
    // An episode's own id still says which show it belongs to.
    if let Some(e) = first("tv_episode_results") {
        return Ok(e["show_id"].as_i64().map(|id| ("tv".into(), id)));
    }
    Ok(None)
}

// --- the id lists ----------------------------------------------------------------

/// Find the TMDB season and offset that absolute episode `abs` (0-based) falls
/// in, given TMDB's (season, episode count) list. Specials are not counted.
pub fn locate(abs: i64, seasons: &[(i64, i64)]) -> Option<(i64, i64)> {
    let mut remaining = abs;
    for &(number, count) in seasons.iter().filter(|(n, _)| *n >= 1) {
        if remaining < count {
            return Some((number, remaining));
        }
        remaining -= count;
    }
    None
}

/// Place a TVDB (season, offset) on TMDB's numbering.
///
/// When TMDB has the same season and it is long enough, the numbering is the
/// same. Otherwise `earlier` is how many episodes TVDB's earlier seasons hold,
/// which gives the absolute position to find in TMDB's seasons. Season -1 is
/// TVDB's absolute order, where the offset already is the position.
pub fn place_tv(
    tvdb_season: i64,
    offset: i64,
    episodes: Option<i64>,
    tmdb_seasons: &[(i64, i64)],
    earlier: Option<i64>,
) -> Option<(i64, i64)> {
    if tvdb_season == -1 {
        return locate(offset, tmdb_seasons);
    }
    if tvdb_season < 1 {
        return None;
    }
    let needed = offset + episodes.unwrap_or(1).max(1);
    if tmdb_seasons
        .iter()
        .any(|&(n, count)| n == tvdb_season && count >= needed)
    {
        return Some((tvdb_season, offset));
    }
    locate(earlier? + offset, tmdb_seasons)
}

/// Fribb's anime-lists: the TMDB entry, and for a show TMDB's own season and
/// offset. A spot two AniList entries claim, which is what an unlisted second
/// cour looks like, is not believed.
async fn by_tmdb_list(state: &AppState, remote: &Remote, row: &MapRow) -> AppResult<Option<Found>> {
    let (kind, Some(tmdb_id), season, offset) = (row.4.as_deref(), row.5, row.6, row.7) else {
        return Ok(None);
    };
    let found = match kind {
        Some("movie") => {
            // A twelve-episode series is not a film, whatever a list says.
            let single = is_film(remote) || remote.episodes.is_none_or(|n| n <= 1);
            if !single {
                return Ok(None);
            }
            Found {
                target: Target::Movie(tmdb_id),
                method: "TMDB id list",
                confidence: 88,
                note: None,
            }
        }
        Some("tv") => {
            let Some(season) = season.filter(|s| *s >= 0) else {
                return Ok(None);
            };
            // Specials are only placed where the list says exactly which one.
            if season == 0 && offset.is_none() {
                return Ok(None);
            }
            let (shared,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM anime_map
                  WHERE tmdb_kind = 'tv' AND tmdb_id = ? AND tmdb_season = ? AND tmdb_offset IS ?",
            )
            .bind(tmdb_id)
            .bind(season)
            .bind(offset)
            .fetch_one(&state.pool)
            .await?;
            if shared > 1 {
                // Contested: never applied, so TMDB need not be asked.
                return Ok(Some(Found {
                    target: Target::Show {
                        tmdb_id,
                        season,
                        offset: offset.unwrap_or(0),
                    },
                    method: "TMDB id list",
                    confidence: 50,
                    note: Some(
                        "Another AniList entry is listed at the same place on TMDB.".to_string(),
                    ),
                }));
            }
            let seasons = tmdb_seasons(state, tmdb_id).await?;
            let Some(count) = seasons.iter().find(|s| s.0 == season).map(|s| s.1) else {
                return Ok(None);
            };
            let offset = offset.unwrap_or(0);
            // A season TMDB has not filled in yet still exists; one too short
            // to hold this entry means the list and TMDB disagree.
            if count > 0 && offset >= count {
                return Ok(None);
            }
            Found {
                target: Target::Show {
                    tmdb_id,
                    season,
                    offset,
                },
                method: "TMDB id list",
                confidence: if shared > 1 { 50 } else { 85 },
                note: (shared > 1).then(|| {
                    "Another AniList entry is listed at the same place on TMDB.".to_string()
                }),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(checked(state, remote, found).await?))
}

/// Kometa's Anime-IDs: TVDB's season and offset, turned into TMDB's numbering
/// through the lengths of the seasons before it.
async fn by_tvdb_list(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
    row: &MapRow,
) -> AppResult<Option<Found>> {
    let (tvdb_id, tvdb_season, offset, imdb_id) = (row.0, row.1, row.2, row.3.as_deref());

    if let Some(imdb) = imdb_id {
        if is_film(remote) || tvdb_id.is_none() {
            if let Some((kind, id)) = tmdb_by_external(state, imdb, "imdb_id").await? {
                if kind == "movie" {
                    let found = Found {
                        target: Target::Movie(id),
                        method: "IMDb id",
                        confidence: 85,
                        note: None,
                    };
                    return Ok(Some(checked(state, remote, found).await?));
                }
            }
        }
    }
    if is_film(remote) {
        // A film filed under a show's specials: the film itself is better
        // found by name, and failing that by its air date.
        return Ok(None);
    }

    let (Some(tvdb_id), Some(tvdb_season)) = (tvdb_id, tvdb_season) else {
        return Ok(None);
    };
    // When Fribb's list already named the show, TVDB need not be asked.
    let tmdb_id = match (row.4.as_deref(), row.5) {
        (Some("tv"), Some(id)) => id,
        _ => {
            let Some((kind, id)) = tmdb_by_external(state, &tvdb_id.to_string(), "tvdb_id").await?
            else {
                return Ok(None);
            };
            if kind != "tv" {
                return Ok(None);
            }
            id
        }
    };
    let seasons = tmdb_seasons(state, tmdb_id).await?;
    let earlier = if tvdb_season > 1 {
        earlier_episodes(state, api, token, tvdb_id, tvdb_season).await?
    } else {
        Some(0)
    };
    let Some((season, offset)) = place_tv(tvdb_season, offset, remote.episodes, &seasons, earlier)
    else {
        return Ok(None);
    };
    let found = Found {
        target: Target::Show {
            tmdb_id,
            season,
            offset,
        },
        method: "TVDB id list",
        confidence: 80,
        note: None,
    };
    Ok(Some(checked(state, remote, found).await?))
}

/// Episode counts for AniList ids, from the shared cache or AniList itself.
async fn media_episodes(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    ids: &[i64],
) -> AppResult<HashMap<i64, Option<i64>>> {
    let mut out = HashMap::new();
    let mut missing = Vec::new();
    for &id in ids {
        let row: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT episodes FROM anilist_media WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?;
        match row {
            Some((n,)) => {
                out.insert(id, n);
            }
            None => missing.push(id),
        }
    }
    for chunk in missing.chunks(50) {
        let data = api
            .graphql(token, MEDIA_QUERY, json!({ "ids": chunk }))
            .await?;
        for m in data["Page"]["media"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let Some(id) = m["id"].as_i64() else { continue };
            let n = m["episodes"].as_i64().filter(|n| *n > 0);
            remember_media(&state.pool, id, n, m["format"].as_str(), &media_name(&m)).await?;
            out.insert(id, n);
        }
    }
    Ok(out)
}

/// How many episodes TVDB seasons 1 .. `season - 1` of this show hold, from
/// the AniList entries that make them up. `None` if any of them is unknown.
async fn earlier_episodes(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    tvdb_id: i64,
    season: i64,
) -> AppResult<Option<i64>> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT anilist_id, tvdb_season, tvdb_epoffset FROM anime_map
          WHERE tvdb_id = ? AND tvdb_season BETWEEN 1 AND ?",
    )
    .bind(tvdb_id)
    .bind(season - 1)
    .fetch_all(&state.pool)
    .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
    let episodes = media_episodes(state, api, token, &ids).await?;

    let mut length: BTreeMap<i64, i64> = BTreeMap::new();
    for (id, s, offset) in rows {
        let Some(Some(n)) = episodes.get(&id) else {
            return Ok(None);
        };
        let end = offset + n;
        let slot = length.entry(s).or_insert(0);
        *slot = (*slot).max(end);
    }
    if (1..season).any(|s| !length.contains_key(&s)) {
        return Ok(None);
    }
    Ok(Some(length.values().sum()))
}

// --- how entries hang together on AniList ---------------------------------------

const CHAIN_QUERY: &str = "query ($id: Int) {
  Media(id: $id, type: ANIME) {
    id format status episodes seasonYear startDate { year month day }
    title { english romaji native }
    relations { edges { relationType node { id type format } } }
  }
}";

/// One entry as AniList links it to others.
#[derive(Debug, Clone)]
pub(super) struct Link {
    pub prequel: Option<i64>,
    pub episodes: Option<i64>,
    pub format: Option<String>,
    pub year: Option<i64>,
    pub titles: Vec<String>,
    pub start: Option<String>,
    pub media_status: Option<String>,
    /// Main-run entries this one hangs off or follows, closest first.
    pub related: Vec<i64>,
}

/// (prequel, episodes, format, year, titles, fetched_at, start, status, related)
type ChainRow = (
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<i64>,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
);

/// A full yyyy-mm-dd from AniList's fuzzy date, or nothing.
pub(super) fn full_date(d: &Value) -> Option<String> {
    let (y, m, day) = (
        d["year"].as_i64()?,
        d["month"].as_i64()?,
        d["day"].as_i64()?,
    );
    NaiveDate::from_ymd_opt(y as i32, m as u32, day as u32)
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// Every title AniList gives a media, in order of usefulness.
pub(super) fn titles_of(media: &Value) -> Vec<String> {
    let mut titles: Vec<String> = Vec::new();
    for key in ["english", "romaji", "native"] {
        if let Some(t) = media["title"][key].as_str().map(str::trim) {
            if !t.is_empty() && !titles.iter().any(|known| known == t) {
                titles.push(t.to_string());
            }
        }
    }
    titles
}

/// What links `id` to the rest of its series, from the shared cache or
/// AniList. An entry whose length was still unknown is asked about again
/// after a day.
pub(super) async fn chain_link(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    id: i64,
) -> AppResult<Option<Link>> {
    let row: Option<ChainRow> = sqlx::query_as(
        "SELECT prequel, episodes, format, year, titles, fetched_at, start_date, media_status,
                related
           FROM anilist_chain WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    if let Some((prequel, episodes, format, year, titles, at, start, media_status, related)) = row {
        let recent = chrono::DateTime::parse_from_rfc3339(&at)
            .is_ok_and(|at| Utc::now() - at.with_timezone(&Utc) < chrono::Duration::days(1));
        if episodes.is_some() || recent {
            return Ok(Some(Link {
                prequel,
                episodes,
                format,
                year,
                titles: serde_json::from_str(&titles).unwrap_or_default(),
                start,
                media_status,
                related: serde_json::from_str(&related).unwrap_or_default(),
            }));
        }
    }

    let data = api.graphql(token, CHAIN_QUERY, json!({ "id": id })).await?;
    let media = &data["Media"];
    if media.is_null() {
        return Ok(None);
    }
    let edges: Vec<(String, i64, Option<String>)> = media["relations"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["node"]["type"].as_str() == Some("ANIME"))
        .filter_map(|e| {
            Some((
                e["relationType"].as_str()?.to_string(),
                e["node"]["id"].as_i64()?,
                e["node"]["format"].as_str().map(str::to_string),
            ))
        })
        .collect();
    // Exactly one main-run prequel; a film or OVA before it does not count,
    // and two would be a guess.
    let prequels: Vec<i64> = edges
        .iter()
        .filter(|(rel, _, format)| rel == "PREQUEL" && main_run(format.as_deref()))
        .map(|e| e.1)
        .collect();
    // What a special or film belongs to: its parent first, then the series it
    // follows or sits beside; main-run entries before specials, which can
    // still say which show they are part of.
    let mut related: Vec<i64> = Vec::new();
    for main in [true, false] {
        for rel in [
            "PARENT",
            "PREQUEL",
            "SIDE_STORY",
            "SEQUEL",
            "ALTERNATIVE",
            "SPIN_OFF",
        ] {
            for (r, other, format) in &edges {
                let film = format.as_deref() == Some("MOVIE");
                if r == rel
                    && main_run(format.as_deref()) == main
                    && !film
                    && !related.contains(other)
                {
                    related.push(*other);
                }
            }
        }
    }
    let link = Link {
        prequel: match prequels[..] {
            [one] => Some(one),
            _ => None,
        },
        episodes: media["episodes"].as_i64().filter(|n| *n > 0),
        format: media["format"].as_str().map(str::to_string),
        year: media["startDate"]["year"]
            .as_i64()
            .or_else(|| media["seasonYear"].as_i64()),
        titles: titles_of(media),
        start: full_date(&media["startDate"]),
        media_status: media["status"].as_str().map(str::to_string),
        related,
    };
    sqlx::query(
        "INSERT INTO anilist_chain (id, prequel, episodes, format, year, titles, fetched_at,
                                    start_date, media_status, related)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET
            prequel = excluded.prequel, episodes = excluded.episodes, format = excluded.format,
            year = excluded.year, titles = excluded.titles, fetched_at = excluded.fetched_at,
            start_date = excluded.start_date, media_status = excluded.media_status,
            related = excluded.related",
    )
    .bind(id)
    .bind(link.prequel)
    .bind(link.episodes)
    .bind(link.format.as_deref())
    .bind(link.year)
    .bind(serde_json::to_string(&link.titles)?)
    .bind(Utc::now().to_rfc3339())
    .bind(link.start.as_deref())
    .bind(link.media_status.as_deref())
    .bind(serde_json::to_string(&link.related)?)
    .execute(&state.pool)
    .await?;
    Ok(Some(link))
}

/// A related entry as a `Remote`, so it can be placed the same way.
fn as_remote(id: i64, link: &Link, like: &Remote) -> Remote {
    Remote {
        media_id: id,
        name: link.titles.first().cloned().unwrap_or_default(),
        titles: link.titles.clone(),
        synonyms: Vec::new(),
        format: link.format.clone(),
        episodes: link.episodes,
        year: link.year,
        start: link.start.clone(),
        media_status: link.media_status.clone(),
        country: like.country.clone(),
        next_airing: None,
        view: like.view.clone(),
    }
}

/// A later season nobody lists: walk AniList's prequels back to the first
/// season, place that, and count this one's start on from it through TMDB's
/// seasons. Season 3 after a 25-episode and a 12-episode season starts at
/// episode 38 of the series, wherever TMDB's seasons fall.
///
/// Gives the show it found too, so a season TMDB has not listed yet can still
/// say which show it belongs to.
async fn after_prequels(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
) -> AppResult<(Option<Found>, Option<i64>)> {
    if !main_run(remote.format.as_deref()) && remote.format.is_some() {
        return Ok((None, None));
    }
    let mut before = 0;
    let mut id = remote.media_id;
    let mut root: Option<(i64, Link)> = None;
    // Long-running series have a dozen seasons at most; a loop in the
    // relations must not run forever.
    for _ in 0..24 {
        let Some(link) = chain_link(state, api, token, id).await? else {
            return Ok((None, None));
        };
        let Some(prequel) = link.prequel else {
            root = Some((id, link));
            break;
        };
        let Some(earlier) = chain_link(state, api, token, prequel).await? else {
            return Ok((None, None));
        };
        let Some(length) = earlier.episodes else {
            return Ok((None, None));
        };
        before += length;
        id = prequel;
    }
    let Some((root_id, link)) = root else {
        return Ok((None, None));
    };
    if root_id == remote.media_id {
        // Already the first season.
        return Ok((None, None));
    }
    let first = as_remote(root_id, &link, remote);
    let placed = Box::pin(known_place(state, api, token, &first)).await?;
    let Some(Found {
        target:
            Target::Show {
                tmdb_id,
                season,
                offset,
            },
        confidence,
        ..
    }) = placed
    else {
        return Ok((None, None));
    };
    if confidence < CONFIDENT {
        return Ok((None, None));
    }
    let seasons = tmdb_seasons(state, tmdb_id).await?;
    let start: i64 = seasons
        .iter()
        .filter(|(n, _)| *n >= 1 && *n < season)
        .map(|(_, count)| count)
        .sum::<i64>()
        + offset;
    let found = locate(start + before, &seasons).map(|(season, offset)| Found {
        target: Target::Show {
            tmdb_id,
            season,
            offset,
        },
        method: "the season before it",
        confidence: 78,
        note: None,
    });
    let found = match found {
        Some(f) => Some(checked(state, remote, f).await?),
        None => None,
    };
    Ok((found, Some(tmdb_id)))
}

// --- air dates -----------------------------------------------------------------

/// The TMDB shows an entry could belong to: from the id lists, from the
/// entries it hangs off on AniList, and from its own name without subtitle.
async fn related_shows(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
    row: Option<&MapRow>,
    searched: &mut Searches,
) -> AppResult<Vec<i64>> {
    let mut shows: Vec<i64> = Vec::new();
    let push = |id: i64, shows: &mut Vec<i64>| {
        if !shows.contains(&id) {
            shows.push(id);
        }
    };
    if let Some(row) = row {
        if let (Some("tv"), Some(id)) = (row.4.as_deref(), row.5) {
            push(id, &mut shows);
        }
    }
    if let Some(link) = chain_link(state, api, token, remote.media_id).await? {
        for other in link.related.iter().take(4) {
            if let Some(r) = map_row(state, *other).await? {
                if let (Some("tv"), Some(id)) = (r.4.as_deref(), r.5) {
                    push(id, &mut shows);
                    continue;
                }
                if let Some(tvdb) = r.0 {
                    if let Some((kind, id)) =
                        tmdb_by_external(state, &tvdb.to_string(), "tvdb_id").await?
                    {
                        if kind == "tv" {
                            push(id, &mut shows);
                        }
                    }
                }
            }
        }
    }
    // "One Piece: Heart of Gold" and "ONE PIECE FAN LETTER" belong to the
    // show called One Piece: a show whose whole name starts the entry's.
    let mut bases: Vec<String> = remote.titles.iter().filter_map(|t| base(t)).collect();
    bases.extend(remote.titles.iter().map(|t| clean(t)));
    let full: Vec<String> = remote.titles.iter().map(|t| plain(&clean(t))).collect();
    for b in bases.iter().take(3) {
        for r in searched.results(state, b).await? {
            if r["media_type"].as_str() != Some("tv") || !animated(&r) {
                continue;
            }
            let named = ["name", "original_name"]
                .iter()
                .filter_map(|k| r[*k].as_str())
                .map(|n| plain(&clean(n)))
                .any(|n| n.chars().count() >= 5 && full.iter().any(|f| f.starts_with(&n)));
            if let (true, Some(id)) = (named, r["id"].as_i64()) {
                push(id, &mut shows);
            }
        }
    }
    Ok(shows)
}

/// The episode AniList's start date points at, among the shows the entry
/// could belong to. Specials look in TMDB's specials first, series in the
/// regular seasons first.
async fn by_air_date(
    state: &AppState,
    remote: &Remote,
    shows: &[i64],
    continues: bool,
) -> AppResult<Option<Found>> {
    let Some(start) = remote.start.as_deref() else {
        return Ok(None);
    };
    let specials_first = is_special(remote) || is_film(remote);
    // (show, season, episode, days apart)
    let mut hits: Vec<(i64, i64, i64, i64)> = Vec::new();
    for &show in shows.iter().take(4) {
        for (season, _) in tmdb_seasons(state, show).await? {
            for (episode, date) in season_dates(state, show, season).await? {
                if let Some(d) = date.as_deref().and_then(|d| days_apart(start, d)) {
                    if d <= 1 {
                        hits.push((show, season, episode, d));
                    }
                }
            }
        }
    }
    // A special is only ever one of TMDB's specials: a regular episode that
    // aired the same day is a coincidence, not a match. A series is looked
    // for in the regular seasons, and in the specials only when it is not
    // there (an ONA TMDB keeps as specials).
    let preferred: Vec<&(i64, i64, i64, i64)> = hits
        .iter()
        .filter(|h| (h.1 == 0) == specials_first)
        .collect();
    let pool: Vec<&(i64, i64, i64, i64)> = if preferred.is_empty() && !specials_first {
        hits.iter().collect()
    } else {
        preferred
    };
    // A double premiere airs two episodes the same day: the first is where
    // it starts. Two different places, though, is a guess.
    let mut places: Vec<(i64, i64)> = pool.iter().map(|h| (h.0, h.1)).collect();
    places.sort_unstable();
    places.dedup();
    let exact = pool.iter().any(|h| h.3 == 0);
    let Some(&&(show, season, _, _)) = pool.iter().min_by_key(|h| (h.3, h.1, h.2)) else {
        return Ok(None);
    };
    let first = pool
        .iter()
        .filter(|h| h.0 == show && h.1 == season)
        .map(|h| h.2)
        .min()
        .unwrap_or(1);
    let mut confidence = match (places.len(), exact) {
        (1, true) => 90,
        (1, false) => 84,
        _ => 55,
    };
    // Starting in the middle of a TMDB season is what a second cour does;
    // anything that does not follow a season of its own landing there has
    // only shared an air date with somebody else's episode.
    if season > 0 && first > 1 && !continues {
        confidence = confidence.min(55);
    }
    Ok(Some(Found {
        target: Target::Show {
            tmdb_id: show,
            season,
            offset: first - 1,
        },
        method: "air date",
        confidence,
        note: (places.len() > 1).then(|| "Several episodes on TMDB aired that day.".to_string()),
    }))
}

// --- names ---------------------------------------------------------------------

/// TMDB searches made while placing one entry, each made once.
#[derive(Default)]
struct Searches {
    done: HashMap<String, Vec<Value>>,
}

impl Searches {
    async fn results(&mut self, state: &AppState, query: &str) -> AppResult<Vec<Value>> {
        let key = plain(query);
        if let Some(found) = self.done.get(&key) {
            return Ok(found.clone());
        }
        let found = state.tmdb.search_multi(query, 1).await?;
        let list: Vec<Value> = found["results"].as_array().cloned().unwrap_or_default();
        self.done.insert(key, list.clone());
        Ok(list)
    }
}

fn animated(r: &Value) -> bool {
    r["genre_ids"]
        .as_array()
        .is_some_and(|g| g.iter().any(|id| id.as_i64() == Some(ANIMATION)))
}

fn year_of(r: &Value) -> Option<i64> {
    r["release_date"]
        .as_str()
        .or_else(|| r["first_air_date"].as_str())
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse().ok())
}

/// One TMDB result weighed against the entry: the score, and what is wrong
/// with it, in words.
struct Scored {
    result: Value,
    score: i64,
    exact: bool,
    problems: Vec<&'static str>,
}

fn score(remote: &Remote, wanted: &[String], r: &Value) -> Option<Scored> {
    let kind = r["media_type"].as_str()?;
    if kind != "movie" && kind != "tv" {
        return None;
    }
    let film_like = kind == "movie" || is_film(remote);
    let theirs: Vec<String> = ["title", "name", "original_title", "original_name"]
        .iter()
        .filter_map(|k| r[*k].as_str())
        .flat_map(|n| {
            [
                plain(n),
                plain(&numbered(n)),
                if film_like {
                    plain(&numbered(&clean_keep_numbers(n)))
                } else {
                    plain(&clean(n))
                },
            ]
        })
        .filter(|n| !n.is_empty())
        .collect();
    let mut problems = Vec::new();
    let exact = theirs.iter().any(|t| wanted.contains(t));
    let close = theirs
        .iter()
        .flat_map(|t| wanted.iter().map(move |w| similarity(t, w)))
        .fold(0.0, f64::max);
    let mut score = if exact {
        60
    } else if close >= 0.88 {
        40
    } else {
        problems.push("the name is different");
        0
    };
    let film = is_film(remote);
    let series = remote.format.is_some() && !film;
    if film && kind == "tv" {
        score -= 40;
        problems.push("it is a series, this is a film");
    } else if series && kind == "movie" && !is_special(remote) && remote.episodes != Some(1) {
        score -= 40;
        problems.push("it is a film, this is a series");
    }
    if animated(r) {
        score += 15;
    } else {
        score -= 25;
        problems.push("it is not animated");
    }
    // Made where AniList says it was: a Japanese entry is a Japanese title.
    let language = r["original_language"].as_str();
    match remote.country.as_deref() {
        Some("JP") if language == Some("ja") => score += 10,
        Some("KR") if language == Some("ko") => score += 10,
        Some("CN" | "TW") if matches!(language, Some("zh" | "cn")) => score += 10,
        Some(_) if language.is_some() => score -= 10,
        _ => {}
    }
    match (remote.year, year_of(r)) {
        (Some(a), Some(b)) if a == b => score += 20,
        (Some(a), Some(b)) if (a - b).abs() == 1 => score += 10,
        (Some(_), Some(_)) => {
            score -= 30;
            problems.push("the year is different");
        }
        // Not out yet: TMDB often has no date for it either.
        (None, None) if unreleased(remote) => score += 10,
        _ => {}
    }
    Some(Scored {
        result: r.clone(),
        score,
        exact,
        problems,
    })
}

/// Every TMDB result found under the entry's names, best first.
async fn scored_results(
    state: &AppState,
    remote: &Remote,
    searched: &mut Searches,
) -> AppResult<Vec<Scored>> {
    let wanted = names(remote);
    let mut seen: HashSet<(String, i64)> = HashSet::new();
    let mut out: Vec<Scored> = Vec::new();
    for q in queries(remote) {
        for r in searched.results(state, &q).await? {
            let (Some(kind), Some(id)) = (r["media_type"].as_str(), r["id"].as_i64()) else {
                continue;
            };
            if !seen.insert((kind.to_string(), id)) {
                continue;
            }
            if let Some(s) = score(remote, &wanted, &r) {
                out.push(s);
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.score));
    Ok(out)
}

/// One TMDB title clearly ahead of the rest under the entry's own name. A
/// show matched this way is its first season, since a later season starts
/// later than the show does; an entry with a season before it is placed by
/// that instead.
async fn by_name(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
    scored: &[Scored],
) -> AppResult<Option<Found>> {
    let Some(best) = scored.first() else {
        return Ok(None);
    };
    // Clearly ahead: no runner-up at all, or one well behind.
    let margin = scored.get(1).map_or(i64::MAX, |s| best.score - s.score);
    let bar = if unreleased(remote) { 80 } else { 90 };
    if !best.exact || best.score < bar || margin < 15 {
        return Ok(None);
    }
    let Some(id) = best.result["id"].as_i64() else {
        return Ok(None);
    };
    let target = if best.result["media_type"].as_str() == Some("movie") {
        Target::Movie(id)
    } else {
        if is_special(remote) {
            return Ok(None);
        }
        // A sequel season is not season 1 of the show it continues, unless
        // TMDB keeps it as a show of its own that started that same year.
        if let Some(link) = chain_link(state, api, token, remote.media_id).await? {
            let own_show = year_of(&best.result).is_some() && year_of(&best.result) == remote.year;
            if link.prequel.is_some() && !own_show {
                return Ok(None);
            }
        }
        let seasons = tmdb_seasons(state, id).await?;
        if !seasons.iter().any(|s| s.0 == 1) && !unreleased(remote) {
            return Ok(None);
        }
        Target::Show {
            tmdb_id: id,
            season: 1,
            offset: 0,
        }
    };
    let found = Found {
        target,
        method: "name and year",
        confidence: (best.score - 15).clamp(CONFIDENT, 92),
        note: None,
    };
    Ok(Some(checked(state, remote, found).await?))
}

/// The closest TMDB titles, for the person to choose from, with what is
/// wrong with each.
fn candidates(scored: &[Scored]) -> Vec<Value> {
    // Even a poor match is worth offering when choosing by hand; what is
    // wrong with it is listed beside it.
    scored
        .iter()
        .take(6)
        .map(|s| {
            let r = &s.result;
            let date = r["release_date"]
                .as_str()
                .or_else(|| r["first_air_date"].as_str())
                .unwrap_or("");
            json!({
                "kind": r["media_type"],
                "tmdb_id": r["id"],
                "name": r["title"].as_str().or_else(|| r["name"].as_str()).unwrap_or(""),
                "year": date.get(..4),
                "poster_path": r["poster_path"],
                "score": s.score,
                "problems": s.problems,
            })
        })
        .collect()
}

// --- putting it together ---------------------------------------------------------

/// Placing an entry from the id lists alone, and failing that its own name:
/// what a related entry needs to be placed by, without walking further.
async fn known_place(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
) -> AppResult<Option<Found>> {
    let row = map_row(state, remote.media_id).await?;
    if let Some(row) = &row {
        if let Some(f) = by_tmdb_list(state, remote, row).await? {
            if f.confidence >= CONFIDENT {
                return Ok(Some(f));
            }
        }
        if let Some(f) = by_tvdb_list(state, api, token, remote, row).await? {
            if f.confidence >= CONFIDENT {
                return Ok(Some(f));
            }
        }
    }
    let mut searched = Searches::default();
    let scored = scored_results(state, remote, &mut searched).await?;
    by_name(state, api, token, remote, &scored).await
}

/// Where this entry goes, with how sure that is and why.
pub async fn place(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
) -> AppResult<Placement> {
    let row = map_row(state, remote.media_id).await?;
    let mut found: Vec<Found> = Vec::new();
    let mut searched = Searches::default();
    // Once one piece of evidence is confident, the rest is not looked for:
    // every step costs requests, and the id lists are already checked
    // against the air dates when AniList has one.
    let settled = |found: &[Found]| found.iter().any(|f| f.confidence >= CONFIDENT);

    // 1. The id lists. A film a list files under a show's specials waits:
    //    the film itself is where anyone looks for it.
    let film_as_special =
        is_film(remote) && row.as_ref().is_some_and(|r| r.4.as_deref() == Some("tv"));
    if let Some(row) = &row {
        if !film_as_special {
            if let Some(f) = by_tmdb_list(state, remote, row).await? {
                found.push(f);
            }
        }
        if let Some(f) = by_tvdb_list(state, api, token, remote, row).await? {
            found.push(f);
        }
    }

    // 2. A film, or anything not out yet, by its own name.
    let mut scored: Option<Vec<Scored>> = None;
    if (is_film(remote) || unreleased(remote)) && !settled(&found) {
        let list = scored_results(state, remote, &mut searched).await?;
        if let Some(f) = by_name(state, api, token, remote, &list).await? {
            found.push(f);
        }
        scored = Some(list);
    }

    // 3. A later season: count on from the first.
    let mut show: Option<i64> = None;
    if !settled(&found) && !is_film(remote) {
        let (f, root_show) = after_prequels(state, api, token, remote).await?;
        show = root_show;
        if let Some(f) = f {
            found.push(f);
        }
    }

    // 4. A film no TMDB film matched, filed as a special after all.
    if film_as_special && !settled(&found) {
        if let Some(row) = &row {
            let as_special = Remote {
                format: Some("SPECIAL".into()),
                ..remote.clone()
            };
            if let Some(f) = by_tmdb_list(state, &as_special, row).await? {
                found.push(f);
            }
        }
    }

    // 5. The air date, under every show it could belong to.
    if !settled(&found) && remote.start.is_some() {
        let shows = related_shows(state, api, token, remote, row.as_ref(), &mut searched).await?;
        if show.is_none() {
            show = shows.first().copied();
        }
        let continues = chain_link(state, api, token, remote.media_id)
            .await?
            .is_some_and(|l| l.prequel.is_some());
        if let Some(f) = by_air_date(state, remote, &shows, continues).await? {
            found.push(f);
        }
    }

    // 6. A series by its own name.
    if !settled(&found) && !is_film(remote) && !unreleased(remote) {
        let list = match scored.take() {
            Some(list) => list,
            None => scored_results(state, remote, &mut searched).await?,
        };
        if let Some(f) = by_name(state, api, token, remote, &list).await? {
            found.push(f);
        }
        scored = Some(list);
    }

    // Evidence that agrees is worth more than any one piece of it.
    found.sort_by_key(|f| std::cmp::Reverse(f.confidence));
    let evidence: Vec<String> = found
        .iter()
        .map(|f| format!("{} {:?} {}", f.method, f.target, f.confidence))
        .collect();
    if let Some(best) = found.first().cloned() {
        let agreeing = found
            .iter()
            .filter(|f| f.target == best.target && f.method != best.method)
            .count() as i64;
        let confidence = (best.confidence + 5 * agreeing).min(99);
        let disagreeing = found.iter().any(|f| {
            f.target != best.target
                && f.confidence >= CONFIDENT
                && f.confidence + 10 > best.confidence
        });
        if confidence >= CONFIDENT && !disagreeing {
            return Ok(Placement {
                target: Some(best.target),
                outcome: Outcome::Matched,
                method: Some(best.method),
                confidence,
                note: best.note,
                show,
                candidates: Vec::new(),
                evidence,
            });
        }
        let note = if disagreeing {
            "The id lists and the air dates point at different places.".to_string()
        } else {
            best.note
                .clone()
                .unwrap_or_else(|| "A likely match, but not a certain one.".to_string())
        };
        let scored = match scored.take() {
            Some(list) => list,
            None => scored_results(state, remote, &mut searched).await?,
        };
        let mut options = candidates(&scored);
        // The best guess comes first, preselected in Settings.
        options.insert(0, guess(state, best.target).await);
        return Ok(Placement {
            target: None,
            outcome: if unreleased(remote) {
                Outcome::Upcoming
            } else {
                Outcome::NotConfident
            },
            method: Some(best.method),
            confidence: best.confidence,
            note: Some(note),
            show,
            candidates: options,
            evidence,
        });
    }

    let scored = match scored.take() {
        Some(list) => list,
        None => scored_results(state, remote, &mut searched).await?,
    };
    if unreleased(remote) {
        let note = match show {
            Some(id) => {
                let detail = state.tmdb.tv(id).await.unwrap_or(Value::Null);
                let name = models::str_at(&detail, "name").unwrap_or_else(|| "its show".into());
                format!("Not out yet, and not on TMDB as part of {name} yet. Frog places it once it is.")
            }
            None => "Not out yet, and not on TMDB yet. Frog places it once it is.".to_string(),
        };
        return Ok(Placement {
            target: None,
            outcome: Outcome::Upcoming,
            method: None,
            confidence: 0,
            note: Some(note),
            show,
            candidates: candidates(&scored),
            evidence,
        });
    }

    let options = candidates(&scored);
    let (outcome, note) = match scored.first() {
        Some(best) if best.score > 0 => (
            Outcome::NotConfident,
            format!(
                "Closest is {}, but {}.",
                best.result["title"]
                    .as_str()
                    .or_else(|| best.result["name"].as_str())
                    .unwrap_or("a title"),
                if best.problems.is_empty() {
                    "another title is just as close".to_string()
                } else {
                    best.problems.join(" and ")
                }
            ),
        ),
        _ => (
            Outcome::NoCandidate,
            "Nothing on TMDB looks like it.".to_string(),
        ),
    };
    Ok(Placement {
        target: None,
        outcome,
        method: None,
        confidence: 0,
        note: Some(note),
        show,
        candidates: options,
        evidence,
    })
}

/// A placement as a candidate for the person to confirm. Best effort: a
/// missing name never stands in the way of the suggestion itself.
async fn guess(state: &AppState, target: Target) -> Value {
    match target {
        Target::Movie(id) => {
            let d = state.tmdb.movie(id).await.unwrap_or(Value::Null);
            json!({
                "kind": "movie", "tmdb_id": id,
                "name": models::str_at(&d, "title").unwrap_or_default(),
                "year": models::str_at(&d, "release_date").and_then(|r| r.get(..4).map(str::to_string)),
                "poster_path": models::str_at(&d, "poster_path"),
                "suggested": true,
            })
        }
        Target::Show {
            tmdb_id,
            season,
            offset,
        } => {
            let d = state.tmdb.tv(tmdb_id).await.unwrap_or(Value::Null);
            json!({
                "kind": "tv", "tmdb_id": tmdb_id,
                "name": models::str_at(&d, "name").unwrap_or_default(),
                "year": models::str_at(&d, "first_air_date").and_then(|r| r.get(..4).map(str::to_string)),
                "poster_path": models::str_at(&d, "poster_path"),
                "season": season,
                "from_episode": offset + 1,
                "suggested": true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Jujutsu Kaisen as TMDB has it: one long season.
    const JJK: [(i64, i64); 1] = [(1, 47)];

    #[test]
    fn a_later_tracker_season_lands_inside_one_long_tmdb_season() {
        assert_eq!(place_tv(2, 0, Some(23), &JJK, Some(24)), Some((1, 24)));
        assert_eq!(place_tv(1, 0, Some(24), &JJK, Some(0)), Some((1, 0)));
    }

    #[test]
    fn matching_numbering_is_kept_and_a_split_cour_keeps_its_offset() {
        let aot = [(1, 25), (2, 12), (3, 22), (4, 30)];
        assert_eq!(place_tv(3, 12, Some(10), &aot, None), Some((3, 12)));
        assert_eq!(place_tv(4, 0, Some(16), &aot, None), Some((4, 0)));
    }

    #[test]
    fn absolute_order_walks_tmdb_seasons() {
        let seasons = [(0, 5), (1, 61), (2, 16), (3, 14)];
        // Specials never count towards the position.
        assert_eq!(place_tv(-1, 70, None, &seasons, None), Some((2, 9)));
        assert_eq!(place_tv(-1, 0, None, &seasons, None), Some((1, 0)));
        assert_eq!(locate(1000, &seasons), None);
    }

    #[test]
    fn an_unknown_earlier_season_means_unsure_not_a_guess() {
        assert_eq!(place_tv(2, 0, Some(12), &JJK, None), None);
        assert_eq!(place_tv(0, 0, Some(1), &JJK, Some(0)), None);
    }

    #[test]
    fn titles_lose_what_tmdb_does_not_use() {
        assert_eq!(clean("Angels of Death (ONA)"), "Angels of Death");
        assert_eq!(clean("Dandadan 3rd Season"), "Dandadan");
        assert_eq!(clean("Gachiakuta Season 2"), "Gachiakuta");
        assert_eq!(clean("Dr. STONE New World Part 2"), "Dr. STONE New World");
        assert_eq!(
            clean("Attack on Titan The Final Season THE FINAL CHAPTERS Special 1"),
            "Attack on Titan THE FINAL CHAPTERS Special 1"
        );
        assert_eq!(
            clean("Dr. STONE SCIENCE FUTURE Cour 2"),
            "Dr. STONE SCIENCE FUTURE"
        );
        assert_eq!(
            base("One Piece: Heart of Gold").as_deref(),
            Some("One Piece")
        );
        assert_eq!(numbered("Infinity Castle Part 2"), "Infinity Castle 2");
        assert_eq!(numbered("Dandadan 3rd Season"), "Dandadan 3");
        assert_eq!(
            plain(&clean_keep_numbers(
                "Sword Art Online the Movie: Integral Domain"
            )),
            plain("Sword Art Online: Integral Domain")
        );
        assert_eq!(plain("Pokémon: Ōkido"), "pokemonokido");
        assert_eq!(
            plain("Frieren: Beyond Journey’s End"),
            plain("FRIEREN - Beyond Journey's End")
        );
    }

    #[test]
    fn near_names_are_near() {
        assert!(similarity("chainsawmanassassinarc", "chainsawmanassassinsarc") > 0.9);
        assert!(similarity("onepiece", "naruto") < 0.3);
    }
}
