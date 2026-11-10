//! TheTVDB v4: the day and time an episode or a film comes out.
//!
//! Only release days and times are read here; everything else about a title
//! is TMDB's or AniList's. A project key is exchanged for a token that lasts a
//! month (`POST /login`), kept in memory and fetched again on a 401. What the
//! answers mean for Frog's episodes is `crate::airing`.
//!
//! TheTVDB gives an episode's day, and its show's time of day as the
//! network's own clock ("21:00" for a US network is Eastern time). The zone is
//! read from the network's country (`zone_for_country`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Tokens last a month; asking again a little early costs one request.
const TOKEN_LIFE: Duration = Duration::from_secs(60 * 60 * 24 * 25);

/// The token, shared by every request this process makes.
pub type Session = Arc<Mutex<Option<(String, Instant)>>>;

/// A show as TheTVDB has it: when it airs, and its episodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub id: i64,
    /// "21:00", the network's clock.
    pub airs_time: Option<NaiveTime>,
    pub zone: Tz,
    pub episodes: Vec<Episode>,
}

/// One episode in TheTVDB's aired order.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub season: i64,
    pub number: i64,
    pub aired: Option<NaiveDate>,
}

/// A film as TheTVDB has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Movie {
    pub id: i64,
    pub first_release: Option<NaiveDate>,
}

/// TheTVDB's faults only reach the worker's log, never a page.
fn fail(why: impl Into<String>) -> AppError {
    AppError::Other(anyhow::anyhow!(why.into()))
}

pub fn is_configured(state: &AppState) -> bool {
    state.config.tvdb.is_some()
}

async fn login(state: &AppState) -> AppResult<String> {
    let client = state
        .config
        .tvdb
        .as_ref()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("TheTVDB is not configured")))?;
    let mut held = state.tvdb.lock().await;
    if let Some((token, at)) = held.as_ref() {
        if at.elapsed() < TOKEN_LIFE {
            return Ok(token.clone());
        }
    }
    let res = state
        .http
        .post(format!("{}/login", client.api_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "apikey": client.key }))
        .send()
        .await
        .map_err(|e| fail(format!("TheTVDB sign-in: {}", e.without_url())))?;
    if !res.status().is_success() {
        return Err(fail(format!("TheTVDB sign-in answered {}", res.status())));
    }
    let body: Value = res
        .json()
        .await
        .map_err(|e| fail(format!("TheTVDB sign-in: {}", e.without_url())))?;
    let token = body["data"]["token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| fail("TheTVDB sign-in gave no token"))?
        .to_string();
    *held = Some((token.clone(), Instant::now()));
    Ok(token)
}

/// One GET, signed in. `Ok(None)` is TheTVDB saying it has no such thing.
async fn get(state: &AppState, path: &str) -> AppResult<Option<Value>> {
    let Some(client) = state.config.tvdb.as_ref() else {
        return Ok(None);
    };
    let url = format!("{}{path}", client.api_url.trim_end_matches('/'));
    for attempt in 0..2 {
        let token = login(state).await?;
        let res = state
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| fail(format!("TheTVDB: {}", e.without_url())))?;
        match res.status().as_u16() {
            200 => {
                let body: Value = res
                    .json()
                    .await
                    .map_err(|e| fail(format!("TheTVDB: {}", e.without_url())))?;
                return Ok(Some(body));
            }
            404 => return Ok(None),
            // A token TheTVDB stopped accepting: sign in once more.
            401 if attempt == 0 => {
                *state.tvdb.lock().await = None;
            }
            status => {
                return Err(fail(format!("TheTVDB answered {status}")));
            }
        }
    }
    Err(fail("TheTVDB refused the token"))
}

/// What a remote id (an IMDb id) names on TheTVDB: (series ids, movie ids).
async fn remote(state: &AppState, imdb: &str) -> AppResult<(Vec<i64>, Vec<i64>)> {
    let path = format!("/search/remoteid/{}", urlencoding::encode(imdb));
    let Some(body) = get(state, &path).await? else {
        return Ok((Vec::new(), Vec::new()));
    };
    Ok(remote_ids(&body))
}

/// The series and movie ids in a `/search/remoteid` answer.
pub fn remote_ids(body: &Value) -> (Vec<i64>, Vec<i64>) {
    let mut series = Vec::new();
    let mut movies = Vec::new();
    for hit in body["data"].as_array().into_iter().flatten() {
        if let Some(id) = hit["series"]["id"].as_i64() {
            series.push(id);
        }
        if let Some(id) = hit["movie"]["id"].as_i64() {
            movies.push(id);
        }
    }
    (series, movies)
}

/// The show TheTVDB knows by `tvdb_id` (from TMDB), else by IMDb id.
pub async fn find_series(
    state: &AppState,
    tvdb_id: Option<i64>,
    imdb: Option<&str>,
) -> AppResult<Option<i64>> {
    if let Some(id) = tvdb_id.filter(|id| *id > 0) {
        return Ok(Some(id));
    }
    match imdb {
        Some(imdb) => Ok(remote(state, imdb).await?.0.first().copied()),
        None => Ok(None),
    }
}

pub async fn find_movie(state: &AppState, imdb: Option<&str>) -> AppResult<Option<i64>> {
    match imdb {
        Some(imdb) => Ok(remote(state, imdb).await?.1.first().copied()),
        None => Ok(None),
    }
}

/// A show's time of day and every episode in aired order, specials included.
pub async fn series(state: &AppState, id: i64) -> AppResult<Option<Series>> {
    let Some(head) = get(state, &format!("/series/{id}/extended?short=true")).await? else {
        return Ok(None);
    };
    let mut episodes = Vec::new();
    // 500 a page; a show with more (a long-running anime) takes a few.
    for page in 0..20 {
        let Some(body) = get(state, &format!("/series/{id}/episodes/default?page={page}")).await?
        else {
            break;
        };
        episodes.extend(parse_episodes(&body));
        if body["links"]["next"].is_null() {
            break;
        }
    }
    Ok(Some(parse_series(id, &head["data"], episodes)))
}

pub async fn movie(state: &AppState, id: i64) -> AppResult<Option<Movie>> {
    let Some(body) = get(state, &format!("/movies/{id}/extended?short=true")).await? else {
        return Ok(None);
    };
    Ok(Some(parse_movie(id, &body["data"])))
}

pub fn parse_series(id: i64, data: &Value, episodes: Vec<Episode>) -> Series {
    let country = data["originalNetwork"]["country"]
        .as_str()
        .or_else(|| data["latestNetwork"]["country"].as_str())
        .or_else(|| data["originalCountry"].as_str())
        .unwrap_or("");
    Series {
        id,
        airs_time: data["airsTime"].as_str().and_then(parse_time),
        zone: zone_for_country(country),
        episodes,
    }
}

pub fn parse_episodes(body: &Value) -> Vec<Episode> {
    body["data"]["episodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some(Episode {
                season: e["seasonNumber"].as_i64()?,
                number: e["number"].as_i64()?,
                aired: e["aired"].as_str().and_then(parse_date),
            })
        })
        .collect()
}

pub fn parse_movie(id: i64, data: &Value) -> Movie {
    let first = data["first_release"]["date"].as_str().and_then(parse_date);
    // No first release: the earliest of the ones listed.
    let earliest = || {
        data["releases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["date"].as_str().and_then(parse_date))
            .min()
    };
    Movie {
        id,
        first_release: first.or_else(earliest),
    }
}

fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()
}

/// "21:00", "9:30" or "21:00:00"; anything else is no time.
pub fn parse_time(raw: &str) -> Option<NaiveTime> {
    let raw = raw.trim();
    NaiveTime::parse_from_str(raw, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(raw, "%H:%M:%S"))
        .ok()
}

/// The clock a network keeps, from TheTVDB's three-letter country. A country
/// with several zones takes the one its networks schedule by: Eastern for
/// the US and Canada, Sydney for Australia. Unknown countries keep UTC.
pub fn zone_for_country(country: &str) -> Tz {
    use chrono_tz::*;
    match country.to_ascii_lowercase().as_str() {
        "usa" => America::New_York,
        "can" => America::Toronto,
        "mex" => America::Mexico_City,
        "bra" => America::Sao_Paulo,
        "arg" => America::Argentina::Buenos_Aires,
        "gbr" | "irl" => Europe::London,
        "fra" => Europe::Paris,
        "deu" => Europe::Berlin,
        "esp" => Europe::Madrid,
        "ita" => Europe::Rome,
        "nld" => Europe::Amsterdam,
        "bel" => Europe::Brussels,
        "swe" => Europe::Stockholm,
        "nor" => Europe::Oslo,
        "dnk" => Europe::Copenhagen,
        "fin" => Europe::Helsinki,
        "pol" => Europe::Warsaw,
        "tur" => Europe::Istanbul,
        "rus" => Europe::Moscow,
        "isr" => Asia::Jerusalem,
        "ind" => Asia::Kolkata,
        "chn" => Asia::Shanghai,
        "twn" => Asia::Taipei,
        "hkg" => Asia::Hong_Kong,
        "jpn" => Asia::Tokyo,
        "kor" => Asia::Seoul,
        "tha" => Asia::Bangkok,
        "phl" => Asia::Manila,
        "aus" => Australia::Sydney,
        "nzl" => Pacific::Auckland,
        "zaf" => Africa::Johannesburg,
        _ => UTC,
    }
}

/// The moment a show's episode comes out: its day at the network's time.
/// A time that a clock change skips is taken an hour later; one it repeats,
/// the first time round.
pub fn instant(day: NaiveDate, time: NaiveTime, zone: Tz) -> Option<DateTime<Utc>> {
    let local = day.and_time(time);
    let found = zone.from_local_datetime(&local).earliest().or_else(|| {
        zone.from_local_datetime(&(local + chrono::Duration::hours(1)))
            .earliest()
    })?;
    Some(found.with_timezone(&Utc))
}

/// `air_at` as the database keeps it: UTC, to the second, with a Z, so it
/// sorts and compares as text.
pub fn stamp(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_us_show_at_nine_is_nine_eastern() {
        let day = NaiveDate::from_ymd_opt(2025, 3, 20).unwrap();
        let at = instant(day, parse_time("21:00").unwrap(), zone_for_country("usa")).unwrap();
        // Daylight time in March: UTC-4.
        assert_eq!(stamp(at), "2025-03-21T01:00:00Z");
        let winter = NaiveDate::from_ymd_opt(2025, 1, 16).unwrap();
        let at = instant(
            winter,
            parse_time("21:00").unwrap(),
            zone_for_country("usa"),
        )
        .unwrap();
        assert_eq!(stamp(at), "2025-01-17T02:00:00Z");
    }

    #[test]
    fn tokyo_late_night_is_afternoon_in_utc() {
        let day = NaiveDate::from_ymd_opt(2023, 9, 29).unwrap();
        let at = instant(day, parse_time("23:00").unwrap(), zone_for_country("JPN")).unwrap();
        assert_eq!(stamp(at), "2023-09-29T14:00:00Z");
    }

    #[test]
    fn a_skipped_hour_moves_on_rather_than_vanishing() {
        // 02:30 does not exist in New York on 9 March 2025.
        let day = NaiveDate::from_ymd_opt(2025, 3, 9).unwrap();
        let at = instant(day, parse_time("02:30").unwrap(), zone_for_country("usa")).unwrap();
        assert_eq!(stamp(at), "2025-03-09T07:30:00Z");
    }

    #[test]
    fn unknown_countries_and_times_are_not_guessed() {
        assert_eq!(zone_for_country(""), chrono_tz::UTC);
        assert_eq!(zone_for_country("xyz"), chrono_tz::UTC);
        assert_eq!(parse_time(""), None);
        assert_eq!(parse_time("evening"), None);
        assert_eq!(parse_time("9:30"), NaiveTime::from_hms_opt(9, 30, 0));
    }

    #[test]
    fn series_episodes_and_films_read_from_their_answers() {
        let head = json!({
            "airsTime": "21:00",
            "originalNetwork": { "country": "usa" },
            "originalCountry": "gbr"
        });
        let eps = json!({ "data": { "episodes": [
            { "seasonNumber": 2, "number": 10, "aired": "2025-03-20" },
            { "seasonNumber": 3, "number": 1, "aired": null },
            { "seasonNumber": null, "number": 1 }
        ]}});
        let s = parse_series(371980, &head, parse_episodes(&eps));
        assert_eq!(s.zone, chrono_tz::America::New_York);
        assert_eq!(s.airs_time, NaiveTime::from_hms_opt(21, 0, 0));
        assert_eq!(s.episodes.len(), 2);
        assert_eq!(s.episodes[1].aired, None);

        let film = parse_movie(
            113,
            &json!({ "first_release": { "country": "global", "date": "2010-07-08" } }),
        );
        assert_eq!(film.first_release, NaiveDate::from_ymd_opt(2010, 7, 8));
        let film = parse_movie(
            1,
            &json!({ "releases": [{ "date": "2010-07-16" }, { "date": "2010-07-09" }] }),
        );
        assert_eq!(film.first_release, NaiveDate::from_ymd_opt(2010, 7, 9));

        let (series, movies) = remote_ids(&json!({ "data": [
            { "series": { "id": 7 } }, { "movie": { "id": 9 } }
        ]}));
        assert_eq!((series, movies), (vec![7], vec![9]));
    }
}
