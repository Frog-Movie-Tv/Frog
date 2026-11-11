//! When titles come out, from TheTVDB.
//!
//! TMDB lists a show's episodes and their days; TheTVDB adds the time of day
//! and is taken over TMDB for the day as well. Episodes are matched by
//! season and number, one season at a time: where the two number a season
//! differently (anime often does), most of its days disagree, and that
//! season keeps TMDB's days rather than taking another episode's. The
//! decision is `plan`, a pure function; `refresh` reads and writes.
//!
//! An episode TheTVDB set is marked `air_source = 'tvdb'`, and
//! `catalog::upsert_episodes` leaves its day alone. A film keeps TheTVDB's
//! first release in `title_airing.release_date`, for its notification.

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::state::AppState;
use crate::tvdb::{self, Series};

/// Days apart before two providers are taken to mean different episodes.
/// A day either way is a time zone (21:00 Eastern is the next day in Europe).
const SAME_EPISODE_DAYS: i64 = 2;

/// Titles read from TheTVDB in one pass of the worker.
const BATCH: i64 = 10;

/// One episode's day, and moment when the time is known, from TheTVDB.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub season: i64,
    pub episode: i64,
    pub date: NaiveDate,
    /// RFC 3339 UTC, as `tvdb::stamp` writes it.
    pub at: Option<String>,
}

/// Frog's episode as TMDB dated it: (season, episode, day).
pub type Known = (i64, i64, Option<NaiveDate>);

/// Which of Frog's episodes take TheTVDB's day and time.
pub fn plan(frog: &[Known], series: &Series) -> Vec<Planned> {
    let theirs: HashMap<(i64, i64), NaiveDate> = series
        .episodes
        .iter()
        .filter_map(|e| Some(((e.season, e.number), e.aired?)))
        .collect();

    let mut seasons: Vec<i64> = frog.iter().map(|k| k.0).collect();
    seasons.sort_unstable();
    seasons.dedup();

    let mut out = Vec::new();
    for season in seasons {
        let ours: Vec<&Known> = frog.iter().filter(|k| k.0 == season).collect();
        let (mut both, mut apart) = (0, 0);
        for (s, e, day) in &ours {
            if let (Some(mine), Some(other)) = (day, theirs.get(&(*s, *e))) {
                both += 1;
                if (*mine - *other).num_days().abs() > SAME_EPISODE_DAYS {
                    apart += 1;
                }
            }
        }
        // Numbered another way: keep TMDB's days for the whole season.
        if both > 0 && apart * 2 > both {
            continue;
        }
        for (s, e, _) in ours {
            let Some(date) = theirs.get(&(*s, *e)).copied() else {
                continue;
            };
            let at = series
                .airs_time
                .and_then(|time| tvdb::instant(date, time, series.zone))
                .map(tvdb::stamp);
            out.push(Planned {
                season: *s,
                episode: *e,
                date,
                at,
            });
        }
    }
    out
}

/// What TheTVDB found for one title, as `title_airing` keeps it.
#[derive(Debug, Default)]
struct Found {
    tvdb_id: Option<i64>,
    airs_time: Option<String>,
    time_zone: Option<String>,
    release_date: Option<String>,
}

/// Read one title from TheTVDB and write what it says. A title TheTVDB does
/// not have is recorded too, so it is not asked about again for a while.
pub async fn refresh(state: &AppState, title_id: i64) -> AppResult<()> {
    if !tvdb::is_configured(state) {
        return Ok(());
    }
    let (kind,): (String,) = sqlx::query_as("SELECT kind FROM title WHERE id = ?")
        .bind(title_id)
        .fetch_one(&state.pool)
        .await?;
    let externals = crate::catalog::externals(&state.pool, title_id).await?;
    let imdb = externals.imdb.first().map(String::as_str);
    let tmdb = externals.tmdb.as_ref().map(|t| t.id);

    let found = match kind.as_str() {
        "tv" => {
            // TMDB says which TheTVDB show it is; IMDb is the way round.
            let tvdb_id = match tmdb {
                Some(id) => state
                    .tmdb
                    .tv(id)
                    .await
                    .ok()
                    .and_then(|d| d["external_ids"]["tvdb_id"].as_i64()),
                None => None,
            };
            match tvdb::find_series(state, tvdb_id, imdb).await? {
                Some(id) => match tvdb::series(state, id).await? {
                    Some(series) => {
                        write_episodes(&state.pool, title_id, &series).await?;
                        Found {
                            tvdb_id: Some(id),
                            airs_time: series.airs_time.map(|t| t.format("%H:%M").to_string()),
                            time_zone: Some(series.zone.name().to_string()),
                            release_date: None,
                        }
                    }
                    None => Found::default(),
                },
                None => Found::default(),
            }
        }
        "movie" => match tvdb::find_movie(state, imdb).await? {
            Some(id) => Found {
                tvdb_id: Some(id),
                release_date: tvdb::movie(state, id)
                    .await?
                    .and_then(|m| m.first_release)
                    .map(|d| d.to_string()),
                ..Found::default()
            },
            None => Found::default(),
        },
        // Manga has no release day to announce.
        _ => Found::default(),
    };

    sqlx::query(
        "INSERT INTO title_airing (title_id, tvdb_id, airs_time, time_zone, release_date, checked_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (title_id) DO UPDATE SET
            tvdb_id = excluded.tvdb_id,
            airs_time = excluded.airs_time,
            time_zone = excluded.time_zone,
            release_date = excluded.release_date,
            checked_at = excluded.checked_at",
    )
    .bind(title_id)
    .bind(found.tvdb_id)
    .bind(found.airs_time)
    .bind(found.time_zone)
    .bind(found.release_date)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// (season, episode, day)
type EpisodeDay = (i64, i64, Option<String>);

async fn write_episodes(pool: &SqlitePool, title_id: i64, series: &Series) -> AppResult<usize> {
    let rows: Vec<EpisodeDay> = sqlx::query_as(
        "SELECT season_number, episode_number, air_date FROM episode WHERE title_id = ?",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    let known: Vec<Known> = rows
        .into_iter()
        .map(|(s, e, day)| {
            let day = day.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
            (s, e, day)
        })
        .collect();
    let planned = plan(&known, series);

    let mut tx = pool.begin().await?;
    for p in &planned {
        sqlx::query(
            "UPDATE episode SET air_date = ?, air_at = ?, air_source = 'tvdb'
              WHERE title_id = ? AND season_number = ? AND episode_number = ?",
        )
        .bind(p.date.to_string())
        .bind(&p.at)
        .bind(title_id)
        .bind(p.season)
        .bind(p.episode)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(planned.len())
}

/// Titles somebody tracks that TheTVDB has not been asked about, or not
/// lately: running shows twice a day, films still to come every few days,
/// everything else monthly.
pub async fn refresh_due(state: &AppState) {
    if !tvdb::is_configured(state) {
        return;
    }
    let now = Utc::now();
    let ago = |hours: i64| (now - chrono::Duration::hours(hours)).to_rfc3339();
    let due: Vec<(i64,)> = match sqlx::query_as(
        "SELECT t.id FROM title t
           LEFT JOIN title_airing a ON a.title_id = t.id
          WHERE t.kind IN ('tv', 'movie') AND t.merged_into IS NULL
            AND EXISTS (SELECT 1 FROM library_entry l WHERE l.title_id = t.id)
            AND (a.title_id IS NULL
                 OR (t.kind = 'tv'
                     AND COALESCE(t.tmdb_status, '') IN ('Returning Series', 'In Production', 'Planned')
                     AND a.checked_at < ?1)
                 OR (t.kind = 'movie'
                     AND (t.release_date IS NULL OR t.release_date >= date('now', '-30 day'))
                     AND a.checked_at < ?2)
                 OR a.checked_at < ?3)
          ORDER BY a.checked_at IS NOT NULL, a.checked_at
          LIMIT ?4",
    )
    .bind(ago(12))
    .bind(ago(72))
    .bind(ago(24 * 30))
    .bind(BATCH)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "could not list titles for TheTVDB");
            return;
        }
    };
    for (title_id,) in due {
        if let Err(e) = refresh(state, title_id).await {
            // TheTVDB unreachable: the rest of the batch would fail the same way.
            tracing::warn!(title_id, error = %e, "TheTVDB not read");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tvdb::Episode;
    use chrono::NaiveTime;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn series(episodes: &[(i64, i64, &str)]) -> Series {
        Series {
            id: 1,
            airs_time: NaiveTime::from_hms_opt(21, 0, 0),
            zone: chrono_tz::America::New_York,
            episodes: episodes
                .iter()
                .map(|(s, n, d)| Episode {
                    season: *s,
                    number: *n,
                    aired: Some(day(d)),
                })
                .collect(),
        }
    }

    #[test]
    fn matching_seasons_take_the_day_and_the_time() {
        let frog = [
            (2, 9, Some(day("2025-03-13"))),
            (2, 10, Some(day("2025-03-21"))),
            // Not on TMDB's calendar yet; TheTVDB has it.
            (2, 11, None),
        ];
        let theirs = series(&[
            (2, 9, "2025-03-13"),
            (2, 10, "2025-03-20"),
            (2, 11, "2025-03-27"),
        ]);
        let p = plan(&frog, &theirs);
        assert_eq!(p.len(), 3);
        assert_eq!(p[1].date, day("2025-03-20"), "TheTVDB's day wins");
        assert_eq!(p[1].at.as_deref(), Some("2025-03-21T01:00:00Z"));
        assert_eq!(p[2].date, day("2025-03-27"));
    }

    #[test]
    fn a_season_numbered_another_way_keeps_tmdbs_days() {
        // TMDB has one long season; TheTVDB splits it, so its 1x13 is weeks off.
        let frog = [
            (1, 12, Some(day("2024-01-05"))),
            (1, 13, Some(day("2024-01-12"))),
            (1, 14, Some(day("2024-01-19"))),
        ];
        let theirs = series(&[
            (1, 12, "2023-12-22"),
            (1, 13, "2023-12-29"),
            (1, 14, "2024-01-19"),
        ]);
        assert!(plan(&frog, &theirs).is_empty());
    }

    #[test]
    fn one_moved_episode_does_not_throw_out_its_season() {
        let frog = [
            (1, 1, Some(day("2024-01-01"))),
            (1, 2, Some(day("2024-01-08"))),
            (1, 3, Some(day("2024-01-15"))),
        ];
        // Episode 3 was pushed back a fortnight; TheTVDB has the new day.
        let theirs = series(&[
            (1, 1, "2024-01-01"),
            (1, 2, "2024-01-08"),
            (1, 3, "2024-01-29"),
        ]);
        let p = plan(&frog, &theirs);
        assert_eq!(p.len(), 3);
        assert_eq!(p[2].date, day("2024-01-29"));
    }

    #[test]
    fn no_time_of_day_keeps_the_day_alone() {
        let mut theirs = series(&[(1, 1, "2024-01-01")]);
        theirs.airs_time = None;
        let p = plan(&[(1, 1, None)], &theirs);
        assert_eq!(p[0].at, None);
        assert_eq!(p[0].date, day("2024-01-01"));
    }
}
