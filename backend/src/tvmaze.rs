//! TVmaze: the moment an episode airs, and nothing else.
//!
//! Only an episode's season, number, day and `airstamp` are read, for
//! episodes TMDB already lists (`crate::airtime` matches them), and a show's
//! usual time and ids, for finding it. No title, name or picture from TVmaze
//! reaches a page. Its data is CC BY-SA, credited
//! in the footer and on the Airing page.
//!
//! The public API needs no key and allows about 20 calls every 10 seconds, so
//! calls are spaced across the whole process and a 429 waits.

use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// 20 calls every 10 seconds is one every half second; a little over that.
const SPACING: Duration = Duration::from_millis(600);

/// Tries of one call before it counts as failed.
const ATTEMPTS: u32 = 4;

static GATE: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);

/// One episode as TVmaze lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub season: i64,
    /// None for a special TVmaze does not number.
    pub number: Option<i64>,
    /// The day on the network's own clock.
    pub airdate: Option<NaiveDate>,
    /// The moment, only when TVmaze knows the time of day.
    pub at: Option<DateTime<Utc>>,
    /// For pairing with TMDB's episode by name.
    pub name: Option<String>,
}

#[derive(Deserialize)]
struct RawEpisode {
    season: Option<i64>,
    number: Option<i64>,
    name: Option<String>,
    airdate: Option<String>,
    airtime: Option<String>,
    airstamp: Option<String>,
}

/// A show's episodes, and the zone its channel keeps time in: what a slot
/// is worked out in (`crate::airtime::usual_slot`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listing {
    pub zone: Option<Tz>,
    pub episodes: Vec<Episode>,
}

#[derive(Deserialize)]
struct RawShow {
    id: i64,
}

fn fail(why: impl Into<String>) -> AppError {
    AppError::Other(anyhow::anyhow!(why.into()))
}

/// Episodes from `/shows/{id}/episodes`. With no time of day TVmaze still
/// writes an `airstamp`, at a stand-in 12:00 UTC; that is no time at all.
pub fn parse_episodes(body: &[u8]) -> AppResult<Vec<Episode>> {
    let raw: Vec<RawEpisode> =
        serde_json::from_slice(body).map_err(|e| fail(format!("TVmaze episodes: {e}")))?;
    Ok(episodes_from(raw))
}

/// A show's zone: its network's, or its streaming channel's when it has no
/// network. A channel for the whole world has none.
fn zone_of(show: &Value) -> Option<Tz> {
    ["network", "webChannel"]
        .iter()
        .find_map(|k| show[k]["country"]["timezone"].as_str())
        .and_then(|z| z.parse::<Tz>().ok())
}

/// The time a show's schedule gives, "21:00", when it gives one.
fn schedule_time(show: &Value) -> Option<NaiveTime> {
    show["schedule"]["time"]
        .as_str()
        .and_then(|t| NaiveTime::parse_from_str(t.trim(), "%H:%M").ok())
}

/// An episode TVmaze gives a day and no time takes the show's own time, on
/// that day in its channel's zone: TVmaze writes the time on the show and
/// leaves it off episodes that keep it.
fn timed_by_show(episodes: &mut [Episode], zone: Option<Tz>, time: Option<NaiveTime>) {
    let (Some(zone), Some(time)) = (zone, time) else {
        return;
    };
    for e in episodes.iter_mut().filter(|e| e.at.is_none()) {
        let Some(day) = e.airdate else { continue };
        e.at = zone
            .from_local_datetime(&day.and_time(time))
            .earliest()
            .map(|at| at.with_timezone(&Utc));
    }
}

/// A show from `/shows/{id}?embed=episodeswithspecials`: its episodes and its
/// network's zone, or its streaming channel's when it has no network. An
/// episode with a day and no time takes the show's (`timed_by_show`).
pub fn parse_listing(body: &[u8]) -> AppResult<Listing> {
    let show: Value =
        serde_json::from_slice(body).map_err(|e| fail(format!("TVmaze show: {e}")))?;
    let zone = zone_of(&show);
    let raw: Vec<RawEpisode> = match &show["_embedded"]["episodeswithspecials"] {
        Value::Null => Vec::new(),
        list => serde_json::from_value(list.clone())
            .map_err(|e| fail(format!("TVmaze episodes: {e}")))?,
    };
    let mut episodes = episodes_from(raw);
    timed_by_show(&mut episodes, zone, schedule_time(&show));
    Ok(Listing { zone, episodes })
}

/// One line of a day's schedule: the episode, and enough of its show to tell
/// which of TMDB's it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheduled {
    pub show: i64,
    pub name: String,
    pub tvdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub episode: Episode,
}

/// A day's schedule, `/schedule?country=…` (the show under `show`) or
/// `/schedule/web` (under `_embedded.show`). An episode with no time takes
/// its show's, as in a listing.
pub fn parse_schedule(body: &[u8]) -> AppResult<Vec<Scheduled>> {
    let lines: Vec<Value> =
        serde_json::from_slice(body).map_err(|e| fail(format!("TVmaze schedule: {e}")))?;
    let mut out = Vec::new();
    for line in lines {
        let show = match &line["show"] {
            Value::Null => &line["_embedded"]["show"],
            show => show,
        };
        let Some(id) = show["id"].as_i64() else {
            continue;
        };
        let Ok(raw) = serde_json::from_value::<RawEpisode>(line.clone()) else {
            continue;
        };
        let mut episodes = episodes_from(vec![raw]);
        timed_by_show(&mut episodes, zone_of(show), schedule_time(show));
        let Some(episode) = episodes.pop() else {
            continue;
        };
        out.push(Scheduled {
            show: id,
            name: show["name"].as_str().unwrap_or("").to_string(),
            tvdb_id: show["externals"]["thetvdb"].as_i64().filter(|id| *id > 0),
            imdb_id: show["externals"]["imdb"]
                .as_str()
                .filter(|id| id.starts_with("tt"))
                .map(str::to_string),
            episode,
        });
    }
    Ok(out)
}

fn episodes_from(raw: Vec<RawEpisode>) -> Vec<Episode> {
    raw.into_iter()
        .filter_map(|e| {
            let timed = e.airtime.as_deref().is_some_and(|t| !t.trim().is_empty());
            Some(Episode {
                season: e.season?,
                number: e.number,
                name: e.name,
                airdate: e
                    .airdate
                    .as_deref()
                    .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
                at: e
                    .airstamp
                    .as_deref()
                    .filter(|_| timed)
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|at| at.with_timezone(&Utc)),
            })
        })
        .collect()
}

/// Wait for this process's turn.
async fn turn() {
    let mut last = GATE.lock().await;
    if let Some(at) = *last {
        let wait = SPACING.saturating_sub(at.elapsed());
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
    *last = Some(Instant::now());
}

/// One GET. `Ok(None)` is TVmaze saying it has no such thing.
async fn get(state: &AppState, path: &str) -> AppResult<Option<Vec<u8>>> {
    let url = format!("{}{path}", state.config.tvmaze_url.trim_end_matches('/'));
    let mut last = String::from("no answer");
    for attempt in 0..ATTEMPTS {
        turn().await;
        let res = match state.http.get(&url).send().await {
            Ok(res) => res,
            Err(e) => {
                last = e.without_url().to_string();
                tokio::time::sleep(Duration::from_secs(2 << attempt)).await;
                continue;
            }
        };
        let status = res.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let secs = res
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(10)
                .min(60);
            tracing::info!(secs, "TVmaze asked Frog to slow down");
            last = "rate limited".into();
            tokio::time::sleep(Duration::from_secs(secs)).await;
            continue;
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_server_error() {
            last = status.to_string();
            tokio::time::sleep(Duration::from_secs(2 << attempt)).await;
            continue;
        }
        if !status.is_success() {
            return Err(fail(format!("TVmaze answered {status}")));
        }
        let body = res
            .bytes()
            .await
            .map_err(|e| fail(format!("TVmaze: {}", e.without_url())))?;
        return Ok(Some(body.to_vec()));
    }
    Err(fail(format!("TVmaze: {last}")))
}

/// The TVmaze show TMDB's TheTVDB id names, else its IMDb id. TVmaze answers a
/// lookup with a redirect to the show, which counts as a second call.
pub async fn find(
    state: &AppState,
    tvdb_id: Option<i64>,
    imdb: Option<&str>,
) -> AppResult<Option<i64>> {
    let mut tries = Vec::new();
    if let Some(id) = tvdb_id.filter(|id| *id > 0) {
        tries.push(format!("/lookup/shows?thetvdb={id}"));
    }
    if let Some(imdb) = imdb.filter(|i| i.starts_with("tt")) {
        tries.push(format!("/lookup/shows?imdb={}", urlencoding::encode(imdb)));
    }
    for path in tries {
        turn().await;
        if let Some(body) = get(state, &path).await? {
            let show: RawShow =
                serde_json::from_slice(&body).map_err(|e| fail(format!("TVmaze show: {e}")))?;
            return Ok(Some(show.id));
        }
    }
    Ok(None)
}

/// TVmaze's answer to a search by name, for a show TMDB's ids do not find
/// (`crate::airtime` decides which, if any, is the show).
pub async fn search(state: &AppState, name: &str) -> AppResult<Value> {
    match get(
        state,
        &format!("/search/shows?q={}", urlencoding::encode(name)),
    )
    .await?
    {
        Some(body) => {
            serde_json::from_slice(&body).map_err(|e| fail(format!("TVmaze search: {e}")))
        }
        None => Ok(Value::Array(Vec::new())),
    }
}

/// What airs on `day`: on television in `country` (an ISO code), or, with
/// `None`, on streaming channels everywhere.
pub async fn schedule(
    state: &AppState,
    country: Option<&str>,
    day: NaiveDate,
) -> AppResult<Vec<Scheduled>> {
    let path = match country {
        Some(c) => format!(
            "/schedule?country={}&date={day}",
            urlencoding::encode(&c.to_ascii_uppercase())
        ),
        None => format!("/schedule/web?date={day}"),
    };
    match get(state, &path).await? {
        Some(body) => parse_schedule(&body),
        None => Ok(Vec::new()),
    }
}

/// Every episode of a show, specials included, and its zone, in one call.
pub async fn listing(state: &AppState, show: i64) -> AppResult<Listing> {
    match get(state, &format!("/shows/{show}?embed=episodeswithspecials")).await? {
        Some(body) => parse_listing(&body),
        None => Ok(Listing::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Stamp;

    #[test]
    fn an_empty_airtime_is_no_time_whatever_the_airstamp_says() {
        let body = br#"[
            { "season": 4, "number": 3, "airdate": "2026-11-15", "airtime": "21:00",
              "airstamp": "2026-11-16T02:00:00+00:00" },
            { "season": 1, "number": 2, "airdate": "2026-11-20", "airtime": "",
              "airstamp": "2026-11-20T12:00:00+00:00" },
            { "season": 1, "number": null, "airdate": null, "airtime": null, "airstamp": null },
            { "season": null, "number": 1 }
        ]"#;
        let eps = parse_episodes(body).unwrap();
        assert_eq!(eps.len(), 3);
        assert_eq!(
            eps[0].at.map(|a| a.stamp()),
            Some("2026-11-16T02:00:00Z".into())
        );
        assert_eq!(eps[1].at, None, "12:00 UTC stand-in is not a time");
        assert_eq!(eps[1].airdate, NaiveDate::from_ymd_opt(2026, 11, 20));
        assert_eq!(eps[2].number, None);
    }

    #[test]
    fn a_listing_carries_its_networks_zone() {
        let body = br#"{
            "id": 3479,
            "network": { "name": "SBS", "country": { "timezone": "Asia/Seoul" } },
            "webChannel": null,
            "_embedded": { "episodeswithspecials": [
                { "season": 2025, "number": 81, "airdate": "2026-08-02", "airtime": "18:10",
                  "airstamp": "2026-08-02T09:10:00+00:00" }
            ] }
        }"#;
        let listing = parse_listing(body).unwrap();
        assert_eq!(listing.zone, Some(chrono_tz::Asia::Seoul));
        assert_eq!(listing.episodes.len(), 1);

        let streamed = br#"{ "network": null,
            "webChannel": { "name": "Prime Video", "country": null } }"#;
        let listing = parse_listing(streamed).unwrap();
        assert_eq!(listing.zone, None);
        assert!(listing.episodes.is_empty());
    }

    #[test]
    fn an_episode_with_a_day_and_no_time_takes_the_shows_time() {
        let body = br#"{
            "id": 2193,
            "schedule": { "time": "19:30", "days": ["Monday"] },
            "network": { "name": "Syndication", "country": { "timezone": "America/New_York" } },
            "_embedded": { "episodeswithspecials": [
                { "season": 43, "number": 51, "airdate": "2026-11-16", "airtime": "",
                  "airstamp": "2026-11-16T12:00:00+00:00" },
                { "season": 43, "number": 52, "airdate": "2026-11-17", "airtime": "20:00",
                  "airstamp": "2026-11-18T01:00:00+00:00" },
                { "season": 43, "number": 53, "airdate": null, "airtime": "", "airstamp": null }
            ] }
        }"#;
        let eps = parse_listing(body).unwrap().episodes;
        // 19:30 in New York in November is 00:30 UTC the next day.
        assert_eq!(
            eps[0].at.map(|a| a.stamp()),
            Some("2026-11-17T00:30:00Z".into())
        );
        assert_eq!(
            eps[1].at.map(|a| a.stamp()),
            Some("2026-11-18T01:00:00Z".into()),
            "its own time stands"
        );
        assert_eq!(eps[2].at, None, "no day, no time");

        // No zone to read the show's time in, or no time: nothing.
        let worldwide = br#"{ "schedule": { "time": "03:00" },
            "webChannel": { "name": "Netflix", "country": null },
            "_embedded": { "episodeswithspecials": [
                { "season": 1, "number": 1, "airdate": "2026-11-20", "airtime": "" } ] } }"#;
        assert_eq!(parse_listing(worldwide).unwrap().episodes[0].at, None);
        let untimed = br#"{ "schedule": { "time": "" },
            "network": { "country": { "timezone": "Europe/London" } },
            "_embedded": { "episodeswithspecials": [
                { "season": 1, "number": 1, "airdate": "2026-11-20", "airtime": "" } ] } }"#;
        assert_eq!(parse_listing(untimed).unwrap().episodes[0].at, None);
    }

    #[test]
    fn both_kinds_of_schedule_are_read() {
        let tv = br#"[
            { "season": 2026, "number": 210, "name": "Episode 210", "airdate": "2026-11-17",
              "airtime": "", "airstamp": "2026-11-17T12:00:00+00:00",
              "show": { "id": 500, "name": "El hormiguero",
                        "schedule": { "time": "21:45" },
                        "network": { "country": { "timezone": "Europe/Madrid" } },
                        "externals": { "thetvdb": 0, "imdb": "tt1234567" } } },
            { "season": 1, "number": 2, "airdate": "2026-11-17", "airtime": "20:00",
              "airstamp": "2026-11-17T19:00:00+00:00", "show": { "name": "No id" } }
        ]"#;
        let lines = parse_schedule(tv).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].show, 500);
        assert_eq!(lines[0].tvdb_id, None, "0 is no id");
        assert_eq!(lines[0].imdb_id.as_deref(), Some("tt1234567"));
        assert_eq!(
            lines[0].episode.at.map(|a| a.stamp()),
            Some("2026-11-17T20:45:00Z".into())
        );

        let web = br#"[
            { "season": 3, "number": 4, "airdate": "2026-11-17", "airtime": "18:00",
              "airstamp": "2026-11-17T10:00:00+00:00",
              "_embedded": { "show": { "id": 77, "name": "Streamed",
                                       "externals": { "thetvdb": 4242, "imdb": null } } } }
        ]"#;
        let lines = parse_schedule(web).unwrap();
        assert_eq!(lines[0].show, 77);
        assert_eq!(lines[0].tvdb_id, Some(4242));
        assert_eq!(
            lines[0].episode.at.map(|a| a.stamp()),
            Some("2026-11-17T10:00:00Z".into())
        );
    }
}
