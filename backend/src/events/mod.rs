//! Events: a countdown to one upcoming film or season, with what to catch up
//! on before it, and a premiere that plays once for each account at zero.
//!
//! - `pack`: the events this release knows, compiled in. Every event is
//!   made by hand with the `/event` skill; the app only switches one off,
//!   pins its moment and previews its premiere.
//! - `records`: the `event*` tables, and how far one account is.
//! - `sync`: packs into the database, TMDB's day, and the notifications.
//! - `view`: an event as one account sees it, for Home and the pages.
//! - here: the decisions, as pure functions on plain values.
//!
//! Zero is midnight on the day it opens, wherever each person is, the same
//! rule as a film announced with a day alone, unless an administrator pins
//! one moment for everyone.

pub mod pack;
pub mod records;
pub mod sync;
pub mod view;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Serialize;

/// How long after zero Opening Night may still be earned.
pub const OPENING_DAYS: i64 = 3;

/// Where an event is in its life, for one viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Not on Home yet.
    Scheduled,
    /// Counting down.
    Live,
    /// Out, and still on Home for a while.
    Premiered,
    /// Kept as a record.
    Past,
}

/// The moment an event opens for someone in `tz`.
pub fn zero(day: NaiveDate, pinned: Option<DateTime<Utc>>, tz: Tz) -> DateTime<Utc> {
    if let Some(at) = pinned {
        return at;
    }
    // A zone that skips midnight for summer time starts the day an hour on.
    [
        NaiveTime::MIN,
        NaiveTime::from_hms_opt(1, 0, 0).unwrap_or(NaiveTime::MIN),
    ]
    .into_iter()
    .find_map(|t| tz.from_local_datetime(&day.and_time(t)).earliest())
    .map(|at| at.with_timezone(&Utc))
    .unwrap_or_else(|| Utc.from_utc_datetime(&day.and_time(NaiveTime::MIN)))
}

pub fn phase(now: DateTime<Utc>, zero: DateTime<Utc>, shows_from: i64, ends_after: i64) -> Phase {
    if now < zero - Duration::days(shows_from) {
        Phase::Scheduled
    } else if now < zero {
        Phase::Live
    } else if now < zero + Duration::days(ends_after) {
        Phase::Premiered
    } else {
        Phase::Past
    }
}

/// Minutes a day that see everything left before zero, rounded up. None
/// once it is out or nothing is left.
pub fn per_day(minutes_left: i64, now: DateTime<Utc>, zero: DateTime<Utc>) -> Option<i64> {
    if minutes_left <= 0 || now >= zero {
        return None;
    }
    let seconds = (zero - now).num_seconds();
    let days = ((seconds + 86_399) / 86_400).max(1);
    Some((minutes_left + days - 1) / days)
}

/// How far one account is with one title on the list.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Standing {
    pub seen: bool,
    /// Episodes, for a show: watched of the listed seasons.
    pub watched: i64,
    pub total: i64,
    pub minutes_left: i64,
    /// "S2E3": the first episode not watched yet.
    pub next: Option<String>,
    /// When Frog first knew it was all seen.
    #[serde(skip)]
    pub seen_at: Option<DateTime<Utc>>,
}

/// A show is seen once every listed episode is, and a show with no episodes
/// listed yet is not seen. Its moment is when the last of them was first
/// recorded.
pub fn show_standing(
    episodes: &[(i64, i64, i64, Option<DateTime<Utc>>)],
    fallback_minutes: i64,
) -> Standing {
    let total = episodes.len() as i64;
    let watched = episodes.iter().filter(|e| e.3.is_some()).count() as i64;
    let minutes_left = episodes
        .iter()
        .filter(|e| e.3.is_none())
        .map(|e| if e.2 > 0 { e.2 } else { fallback_minutes })
        .sum();
    let next = episodes
        .iter()
        .find(|e| e.3.is_none())
        .map(|e| format!("S{}E{}", e.0, e.1));
    let seen = total > 0 && watched == total;
    Standing {
        seen,
        watched,
        total,
        minutes_left,
        next,
        seen_at: if seen {
            episodes.iter().filter_map(|e| e.3).max()
        } else {
            None
        },
    }
}

/// Ready: every essential seen, each recorded before zero.
pub fn ready_by<'a>(
    essentials: impl IntoIterator<Item = &'a Standing>,
    zero: DateTime<Utc>,
) -> bool {
    let mut any = false;
    for s in essentials {
        any = true;
        if !s.seen || s.seen_at.is_none_or(|at| at >= zero) {
            return false;
        }
    }
    any
}

/// The three things an account is told, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Milestone {
    Week,
    Tomorrow,
    Out,
}

impl Milestone {
    pub fn key(self) -> &'static str {
        match self {
            Milestone::Week => "week",
            Milestone::Tomorrow => "tomorrow",
            Milestone::Out => "out",
        }
    }
}

/// The milestone due now, if any. Each is told only in the hours after its
/// moment, so a server that was away does not say "a week to go" days late.
pub fn milestone(now: DateTime<Utc>, zero: DateTime<Utc>) -> Option<Milestone> {
    [
        (Milestone::Week, Duration::days(7), Duration::hours(12)),
        (Milestone::Tomorrow, Duration::days(1), Duration::hours(6)),
        (Milestone::Out, Duration::zero(), Duration::hours(12)),
    ]
    .into_iter()
    .find(|(_, before, window)| {
        let at = zero - *before;
        now >= at && now < at + *window
    })
    .map(|(m, _, _)| m)
}

/// Opening Night: a watch of the event's own title in the days after zero.
pub fn on_opening(watched_at: DateTime<Utc>, zero: DateTime<Utc>) -> bool {
    watched_at >= zero && watched_at < zero + Duration::days(OPENING_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }
    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn zero_is_midnight_where_each_person_is() {
        let d = day("2026-12-18");
        assert_eq!(
            zero(d, None, chrono_tz::Europe::Brussels),
            at("2026-12-17T23:00:00Z")
        );
        assert_eq!(
            zero(d, None, chrono_tz::America::New_York),
            at("2026-12-18T05:00:00Z")
        );
        assert_eq!(zero(d, None, chrono_tz::UTC), at("2026-12-18T00:00:00Z"));
    }

    #[test]
    fn a_pinned_moment_is_the_same_for_everyone() {
        let pin = at("2026-12-18T03:00:00Z");
        for tz in [chrono_tz::Asia::Tokyo, chrono_tz::America::Los_Angeles] {
            assert_eq!(zero(day("2026-12-18"), Some(pin), tz), pin);
        }
    }

    #[test]
    fn a_zone_that_skips_midnight_starts_the_day_an_hour_on() {
        // Santiago moved its clocks from 00:00 to 01:00 on 2026-09-06.
        let z = zero(day("2026-09-06"), None, chrono_tz::America::Santiago);
        assert_eq!(
            z.with_timezone(&chrono_tz::America::Santiago)
                .format("%H:%M")
                .to_string(),
            "01:00"
        );
        // And the day summer time ends has its midnight as usual.
        let z = zero(day("2026-10-25"), None, chrono_tz::Europe::Brussels);
        assert_eq!(z, at("2026-10-24T22:00:00Z"));
    }

    #[test]
    fn phases_follow_the_days_around_zero() {
        let z = at("2026-12-18T00:00:00Z");
        let p = |now: &str| phase(at(now), z, 45, 14);
        assert_eq!(p("2026-11-02T00:00:00Z"), Phase::Scheduled);
        assert_eq!(p("2026-11-03T00:00:00Z"), Phase::Live);
        assert_eq!(p("2026-12-17T23:59:59Z"), Phase::Live);
        assert_eq!(p("2026-12-18T00:00:00Z"), Phase::Premiered);
        assert_eq!(p("2027-01-01T00:00:00Z"), Phase::Past);
    }

    #[test]
    fn minutes_a_day_round_up_and_stop_at_zero() {
        let z = at("2026-12-18T00:00:00Z");
        // 875 minutes over 31 days.
        assert_eq!(per_day(875, at("2026-11-17T00:00:00Z"), z), Some(29));
        // Part of a day left counts as a day.
        assert_eq!(per_day(90, at("2026-12-17T20:00:00Z"), z), Some(90));
        assert_eq!(per_day(0, at("2026-11-17T00:00:00Z"), z), None);
        assert_eq!(per_day(90, z, z), None);
    }

    #[test]
    fn a_show_counts_its_listed_episodes() {
        let w = Some(at("2026-11-01T20:00:00Z"));
        let later = Some(at("2026-11-10T20:00:00Z"));
        let eps = [(1, 1, 50, w), (1, 2, 50, later), (2, 1, 0, None)];
        let s = show_standing(&eps, 45);
        assert!(!s.seen);
        assert_eq!((s.watched, s.total, s.minutes_left), (2, 3, 45));
        assert_eq!(s.next.as_deref(), Some("S2E1"));
        let done = show_standing(&eps[..2], 45);
        assert!(done.seen);
        assert_eq!(done.seen_at, later);
        assert!(!show_standing(&[], 45).seen, "nothing listed is not seen");
    }

    #[test]
    fn ready_needs_every_essential_seen_before_zero() {
        let z = at("2026-12-18T00:00:00Z");
        let seen = |when: &str| Standing {
            seen: true,
            seen_at: Some(at(when)),
            ..Default::default()
        };
        let before = seen("2026-12-01T20:00:00Z");
        let after = seen("2026-12-18T10:00:00Z");
        assert!(ready_by([&before, &before], z));
        assert!(!ready_by([&before, &after], z));
        assert!(!ready_by([&before, &Standing::default()], z));
        assert!(!ready_by(std::iter::empty(), z), "no list, no Ready");
    }

    #[test]
    fn milestones_are_told_in_their_hours_only() {
        let z = at("2026-12-18T00:00:00Z");
        let m = |now: &str| milestone(at(now), z);
        assert_eq!(m("2026-12-11T00:00:00Z"), Some(Milestone::Week));
        assert_eq!(m("2026-12-11T11:59:00Z"), Some(Milestone::Week));
        assert_eq!(m("2026-12-11T12:00:00Z"), None);
        assert_eq!(m("2026-12-17T02:00:00Z"), Some(Milestone::Tomorrow));
        assert_eq!(m("2026-12-18T00:00:00Z"), Some(Milestone::Out));
        assert_eq!(m("2026-12-19T00:00:00Z"), None);
    }

    #[test]
    fn opening_night_is_the_three_days_from_zero() {
        let z = at("2026-12-18T00:00:00Z");
        assert!(on_opening(at("2026-12-18T19:00:00Z"), z));
        assert!(on_opening(at("2026-12-20T23:59:00Z"), z));
        assert!(!on_opening(at("2026-12-21T00:00:00Z"), z));
        assert!(!on_opening(at("2026-12-17T23:00:00Z"), z));
    }
}
