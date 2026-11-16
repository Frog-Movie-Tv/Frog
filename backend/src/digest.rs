//! The weekly digest: one notification on Monday morning in the account's
//! zone, saying what it and its people watched over the week before and how
//! much of what it follows airs in the week ahead.
//!
//! Written once per account per week (`ref_key` `digest:{user}:{monday}`),
//! and not at all for a week with nothing to say. Only dated watches count,
//! like the diary. The people are those `store::shown_to` gives, so a
//! private profile or a switched-off account is never named.

use std::collections::HashSet;
use std::sync::Mutex;

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde_json::json;
use sqlx::SqlitePool;

use crate::clock::Stamp;
use crate::error::AppResult;
use crate::inbox;
use crate::state::AppState;

/// The hour on Monday, in the account's zone, the digest goes out.
pub const HOUR: u32 = 9;

/// How many people are named before the rest are a number.
const NAMED: usize = 3;

/// Weeks already settled for an account since this process started, so a
/// Monday's every minute is one look, not a week's worth of counting.
static SETTLED: Mutex<Option<HashSet<(i64, NaiveDate)>>> = Mutex::new(None);

/// One account's watching over a week.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Week {
    pub episodes: i64,
    pub films: i64,
}

impl Week {
    fn is_empty(&self) -> bool {
        self.episodes == 0 && self.films == 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct Digest {
    pub mine: Week,
    /// Other people by name, most watched first.
    pub people: Vec<(String, Week)>,
    /// Episodes of what the account follows airing in the week ahead.
    pub ahead: i64,
    /// The show the first of them belongs to.
    pub first: Option<String>,
}

/// The Monday a digest is for, when one is due at `now` in `zone`: from
/// `HOUR` on Monday until the week turns over.
pub fn due_monday(now: DateTime<Utc>, zone: Tz) -> Option<NaiveDate> {
    let local = now.with_timezone(&zone);
    let monday = local.date_naive() - Duration::days(local.weekday().num_days_from_monday() as i64);
    (local.weekday() != Weekday::Mon || local.hour() >= HOUR).then_some(monday)
}

fn count(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn watched(w: &Week) -> String {
    match (w.episodes, w.films) {
        (e, 0) => count(e, "episode", "episodes"),
        (0, f) => count(f, "film", "films"),
        (e, f) => format!(
            "{} and {}",
            count(e, "episode", "episodes"),
            count(f, "film", "films")
        ),
    }
}

/// What the digest says, or nothing for a week with nothing in it.
pub fn words(d: &Digest) -> Option<String> {
    let mut parts = Vec::new();
    if !d.mine.is_empty() {
        parts.push(format!("You watched {}.", watched(&d.mine)));
    }
    let shown: Vec<&(String, Week)> = d.people.iter().filter(|(_, w)| !w.is_empty()).collect();
    for (name, w) in shown.iter().take(NAMED) {
        parts.push(format!("{name} watched {}.", watched(w)));
    }
    if shown.len() > NAMED {
        let rest = shown.len() - NAMED;
        parts.push(format!(
            "{rest} more {} watched something too.",
            if rest == 1 { "person" } else { "people" }
        ));
    }
    if d.ahead > 0 {
        let first = d
            .first
            .as_deref()
            .map(|name| format!(", starting with {name}"))
            .unwrap_or_default();
        parts.push(format!(
            "{} of what you follow {} this week{first}.",
            count(d.ahead, "episode", "episodes"),
            if d.ahead == 1 { "airs" } else { "air" }
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// The moment a day in `zone` starts, as Frog stores moments.
fn start_of(day: NaiveDate, zone: Tz) -> String {
    let midnight = day.and_hms_opt(0, 0, 0).expect("midnight");
    zone.from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&midnight))
        .stamp()
}

async fn week_of(pool: &SqlitePool, user_id: i64, from: &str, to: &str) -> AppResult<Week> {
    let (episodes, films): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(w.episode_id),
                COUNT(CASE WHEN w.episode_id IS NULL AND t.kind = 'movie' THEN 1 END)
           FROM watch_event w JOIN title t ON t.id = w.title_id
          WHERE w.user_id = ? AND w.undated = 0 AND w.watched_at >= ? AND w.watched_at < ?",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    Ok(Week { episodes, films })
}

/// Everything the digest for the week starting `monday` says.
pub async fn gather(
    pool: &SqlitePool,
    user_id: i64,
    monday: NaiveDate,
    zone: Tz,
) -> AppResult<Digest> {
    let (from, to) = (
        start_of(monday - Duration::days(7), zone),
        start_of(monday, zone),
    );
    let mine = week_of(pool, user_id, &from, &to).await?;
    let mut people = Vec::new();
    for person in crate::store::shown_to(pool, user_id).await? {
        let week = week_of(pool, person.id, &from, &to).await?;
        if !week.is_empty() {
            let name = person
                .shown
                .display_name
                .clone()
                .unwrap_or_else(|| person.shown.username.clone());
            people.push((name, week));
        }
    }
    people.sort_by_key(|(name, w)| (-(w.episodes + w.films), name.clone()));

    // Episodes of what is followed airing this week, the first by day.
    let ahead: Vec<(String,)> = sqlx::query_as(&format!(
        "SELECT t.name
           FROM episode e
           JOIN title t ON t.id = e.title_id
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = ?1
          WHERE t.kind = 'tv' AND {followed} AND e.season_number > 0
            AND e.air_date >= ?2 AND e.air_date < ?3
          ORDER BY e.air_date, COALESCE(e.air_at, ''), t.name",
        followed = crate::release::FOLLOWED
    ))
    .bind(user_id)
    .bind(monday.to_string())
    .bind((monday + Duration::days(7)).to_string())
    .fetch_all(pool)
    .await?;
    let first = ahead.first().map(|(name,)| name.clone());
    let ahead = ahead.len() as i64;
    Ok(Digest {
        mine,
        people,
        ahead,
        first,
    })
}

fn settled(user_id: i64, monday: NaiveDate) -> bool {
    let guard = SETTLED.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .is_some_and(|set| set.contains(&(user_id, monday)))
}

fn settle(user_id: i64, monday: NaiveDate) {
    let mut guard = SETTLED.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get_or_insert_with(HashSet::new)
        .insert((user_id, monday));
}

/// Write this week's digest for every account it is due for.
pub async fn run(state: &AppState, now: DateTime<Utc>) -> AppResult<()> {
    let accounts: Vec<(i64,)> = sqlx::query_as("SELECT id FROM app_user WHERE disabled = 0")
        .fetch_all(&state.pool)
        .await?;
    for (user_id,) in accounts {
        let zone = crate::announce::time_zone(&state.pool, user_id).await;
        let Some(monday) = due_monday(now, zone) else {
            continue;
        };
        if settled(user_id, monday) {
            continue;
        }
        let key = format!("digest:{user_id}:{monday}");
        let (written,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM notification WHERE user_id = ? AND ref_key = ?")
                .bind(user_id)
                .bind(&key)
                .fetch_one(&state.pool)
                .await?;
        // A week that started before this account did says nothing.
        let (joined,): (Option<String>,) =
            sqlx::query_as("SELECT MIN(created_at) FROM app_user WHERE id = ?")
                .bind(user_id)
                .fetch_one(&state.pool)
                .await?;
        let new = joined.is_some_and(|j| j > start_of(monday - Duration::days(7), zone));
        if written > 0 || new {
            settle(user_id, monday);
            continue;
        }
        let digest = gather(&state.pool, user_id, monday, zone).await?;
        if let Some(body) = words(&digest) {
            let event = inbox::Event {
                kind: inbox::DIGEST,
                payload: json!({ "body": body }),
                ref_key: Some(key),
                ..Default::default()
            };
            let push = json!({
                "title": "Your week",
                "body": body,
                "url": "/notifications",
                "tag": "frog-digest",
            });
            inbox::tell(state, user_id, &event, Some(push)).await?;
        }
        settle(user_id, monday);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn it_is_due_from_nine_on_monday_in_the_account_s_zone() {
        let brussels: Tz = "Europe/Brussels".parse().unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        // 08:30 in Brussels: not yet.
        assert_eq!(due_monday(at("2026-11-16T07:30:00Z"), brussels), None);
        // 09:00 in Brussels.
        assert_eq!(
            due_monday(at("2026-11-16T08:00:00Z"), brussels),
            Some(monday)
        );
        // Still owed on Wednesday, if the server was down on Monday.
        assert_eq!(
            due_monday(at("2026-11-18T12:00:00Z"), brussels),
            Some(monday)
        );
        // Sunday night in Brussels is still last week's.
        assert_eq!(
            due_monday(at("2026-11-15T22:30:00Z"), brussels),
            Some(NaiveDate::from_ymd_opt(2026, 11, 9).unwrap())
        );
    }

    #[test]
    fn a_week_is_said_in_one_short_line() {
        let d = Digest {
            mine: Week {
                episodes: 5,
                films: 1,
            },
            people: vec![
                (
                    "Sam".into(),
                    Week {
                        episodes: 3,
                        films: 0,
                    },
                ),
                (
                    "Ana".into(),
                    Week {
                        episodes: 0,
                        films: 1,
                    },
                ),
            ],
            ahead: 4,
            first: Some("Dark".into()),
        };
        assert_eq!(
            words(&d).unwrap(),
            "You watched 5 episodes and 1 film. Sam watched 3 episodes. Ana watched 1 film. \
             4 episodes of what you follow air this week, starting with Dark."
        );
    }

    #[test]
    fn many_people_end_in_a_number_and_an_empty_week_says_nothing() {
        let w = Week {
            episodes: 1,
            films: 0,
        };
        let d = Digest {
            people: ["A", "B", "C", "D", "E"]
                .iter()
                .map(|n| (n.to_string(), w))
                .collect(),
            ..Default::default()
        };
        assert!(words(&d)
            .unwrap()
            .ends_with("2 more people watched something too."));
        assert_eq!(words(&Digest::default()), None);
    }
}
