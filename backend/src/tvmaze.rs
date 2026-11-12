//! TVmaze: the moment an episode airs, and nothing else.
//!
//! Only an episode's season, number, day and `airstamp` are read, for
//! episodes TMDB already lists (`crate::airtime` matches them). No title,
//! name or picture from TVmaze reaches a page. Its data is CC BY-SA, credited
//! in the footer and on the Airing page.
//!
//! The public API needs no key and allows about 20 calls every 10 seconds, so
//! calls are spaced across the whole process and a 429 waits.

use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

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
    Ok(raw
        .into_iter()
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
        .collect())
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

/// Every episode of a show, specials included.
pub async fn episodes(state: &AppState, show: i64) -> AppResult<Vec<Episode>> {
    match get(state, &format!("/shows/{show}/episodes?specials=1")).await? {
        Some(body) => parse_episodes(&body),
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            eps[0].at.map(|a| a.to_rfc3339()),
            Some("2026-11-16T02:00:00+00:00".into())
        );
        assert_eq!(eps[1].at, None, "12:00 UTC stand-in is not a time");
        assert_eq!(eps[1].airdate, NaiveDate::from_ymd_opt(2026, 11, 20));
        assert_eq!(eps[2].number, None);
    }
}
