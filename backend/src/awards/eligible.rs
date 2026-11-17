//! Which watches may earn a badge about a moment.
//!
//! A badge means what it says: someone who adds five films they watched years
//! ago, today, must not get Triple Bill. Totals count every watch; a moment
//! counts only a watch that was recorded one at a time, close to when it
//! happened, at a pace someone could have watched it, on a day that was not a
//! library update. Each rule is here, on plain values, and tested on its own.

use crate::clock::Stamp;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};

/// How long after a watch it may be recorded and still be "as it happened".
pub const RECORDED_WITHIN_HOURS: i64 = 36;
/// The least share of a watch's runtime that must pass since the one before.
pub const PACE_SHARE: f64 = 0.5;
/// More titles than this recorded in one day is a library update.
pub const UPDATE_DAY_TITLES: usize = 10;
/// Runtimes when neither the episode nor the title has one, in minutes.
pub const EPISODE_MINUTES: f64 = 25.0;
pub const FILM_MINUTES: f64 = 100.0;
/// A clock a little ahead of the server's still records "now".
const CLOCK_SLACK_MINUTES: i64 = 10;
/// Undo carries how a watch was recorded only for one recorded this recently;
/// anything older comes back counting for totals alone.
const CARRY_DAYS: i64 = 7;

/// How a watch reached Frog (`watch_event.logged_how`).
pub const TICK: &str = "tick";
pub const EXTENSION: &str = "extension";
pub const ANILIST: &str = "anilist";
pub const TOGETHER: &str = "together";
/// The ways that can count for a moment. Never `bulk`, `import`,
/// `anilist_first` or `unknown`.
pub const ONE_AT_A_TIME: [&str; 4] = [TICK, EXTENSION, ANILIST, TOGETHER];
const KNOWN: [&str; 8] = [
    TICK,
    EXTENSION,
    ANILIST,
    TOGETHER,
    "bulk",
    "import",
    "anilist_first",
    "unknown",
];

/// One watch, as the rules see it.
#[derive(Debug, Clone)]
pub struct Logged {
    pub id: i64,
    pub title_id: i64,
    pub watched_at: DateTime<Utc>,
    pub logged_at: Option<DateTime<Utc>>,
    pub how: Option<String>,
    pub undated: bool,
    /// Its runtime, or the stand-in for its kind.
    pub minutes: f64,
}

/// A local day, in the account's zone.
pub fn day(at: DateTime<Utc>, tz: Tz) -> NaiveDate {
    at.with_timezone(&tz).date_naive()
}

pub fn parse(at: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// What undo writes back for how a watch was recorded: the record's own, when
/// it names a way Frog knows and was recorded in the last week; otherwise
/// `unknown`, which counts for totals only. The browser sends the record
/// back, so this is what keeps it from making an old watch new.
pub fn carried(logged_at: Option<&str>, how: Option<&str>) -> (Option<String>, String) {
    let now = Utc::now();
    match (logged_at.and_then(parse), how) {
        (Some(at), Some(how))
            if KNOWN.contains(&how) && at <= now && now - at <= Duration::days(CARRY_DAYS) =>
        {
            (Some(at.stamp()), how.to_string())
        }
        (at, _) => (at.map(|a| a.stamp()), "unknown".to_string()),
    }
}

/// Rules 1, 2, 5 and 6: recorded after badges came, one at a time, close to
/// when it was watched, with a real date, and not in the future.
pub fn recorded_as_it_happened(w: &Logged, now: DateTime<Utc>) -> bool {
    let Some(logged) = w.logged_at else {
        return false;
    };
    let one_at_a_time = w.how.as_deref().is_some_and(|h| ONE_AT_A_TIME.contains(&h));
    one_at_a_time
        && !w.undated
        && w.watched_at <= now
        && w.watched_at <= logged + Duration::minutes(CLOCK_SLACK_MINUTES)
        && logged - w.watched_at <= Duration::hours(RECORDED_WITHIN_HOURS)
}

/// Rule 8: the days, by when things were recorded, on which more than
/// `UPDATE_DAY_TITLES` different titles were. Every way counts here, imports
/// and marking many at once too: a day like that is a library update.
pub fn update_days(all: &[Logged], tz: Tz) -> HashSet<NaiveDate> {
    let mut titles: HashMap<NaiveDate, HashSet<i64>> = HashMap::new();
    for w in all {
        if let Some(at) = w.logged_at {
            titles.entry(day(at, tz)).or_default().insert(w.title_id);
        }
    }
    titles
        .into_iter()
        .filter(|(_, t)| t.len() > UPDATE_DAY_TITLES)
        .map(|(d, _)| d)
        .collect()
}

/// Rule 7 for watches recorded one by one: in the order they were recorded,
/// at least `PACE_SHARE` of each one's runtime since the one before. One that
/// comes too soon, and the one before it, are both catching up. Answers the
/// ids that keep their pace.
pub fn paced(mut watches: Vec<&Logged>) -> HashSet<i64> {
    watches.sort_by_key(|w| (w.logged_at, w.id));
    let mut fast = HashSet::new();
    for pair in watches.windows(2) {
        let (before, this) = (pair[0], pair[1]);
        let gap = (this.logged_at.unwrap() - before.logged_at.unwrap()).num_seconds() as f64 / 60.0;
        if gap < this.minutes * PACE_SHARE {
            fast.insert(before.id);
            fast.insert(this.id);
        }
    }
    watches
        .iter()
        .map(|w| w.id)
        .filter(|id| !fast.contains(id))
        .collect()
}

/// Rule 7 for a later AniList sync, which records everything at one moment:
/// per show per sync, the time since anything was last recorded must cover
/// `PACE_SHARE` of the runtime the sync added for that show. `all` is every
/// watch of the account, so a sync that follows a tick a minute before it
/// does not pass. Answers the ids of the syncs that keep their pace.
pub fn paced_syncs(synced: &[&Logged], all: &[Logged]) -> HashSet<i64> {
    let mut batches: HashMap<(i64, DateTime<Utc>), Vec<&Logged>> = HashMap::new();
    for w in synced {
        batches
            .entry((w.title_id, w.watched_at))
            .or_default()
            .push(w);
    }
    let mut out = HashSet::new();
    for batch in batches.values() {
        let ids: HashSet<i64> = batch.iter().map(|w| w.id).collect();
        let Some(moment) = batch.iter().filter_map(|w| w.logged_at).min() else {
            continue;
        };
        let before = all
            .iter()
            .filter(|w| !ids.contains(&w.id))
            .filter_map(|w| w.logged_at)
            .filter(|at| *at < moment - Duration::seconds(1))
            .max();
        let Some(before) = before else {
            continue;
        };
        let gap = (moment - before).num_seconds() as f64 / 60.0;
        let needed: f64 = batch.iter().map(|w| w.minutes).sum::<f64>() * PACE_SHARE;
        if gap >= needed {
            out.extend(ids);
        }
    }
    out
}

/// Every rule together: the ids of the watches that may earn a moment.
pub fn eligible(all: &[Logged], tz: Tz, now: DateTime<Utc>) -> HashSet<i64> {
    let updates = update_days(all, tz);
    let candidates: Vec<&Logged> = all
        .iter()
        .filter(|w| recorded_as_it_happened(w, now))
        .filter(|w| !updates.contains(&day(w.logged_at.unwrap(), tz)))
        .collect();
    let (synced, single): (Vec<&Logged>, Vec<&Logged>) = candidates
        .into_iter()
        .partition(|w| w.how.as_deref() == Some(ANILIST));
    let mut out = paced(single);
    out.extend(paced_syncs(&synced, all));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        parse(s).unwrap()
    }

    fn w(id: i64, title: i64, watched: &str, logged: &str, how: &str, minutes: f64) -> Logged {
        Logged {
            id,
            title_id: title,
            watched_at: at(watched),
            logged_at: Some(at(logged)),
            how: Some(how.into()),
            undated: false,
            minutes,
        }
    }

    const NOW: &str = "2026-12-01T12:00:00Z";
    const UTC: Tz = chrono_tz::UTC;

    #[test]
    fn an_old_date_picked_today_counts_for_nothing_here() {
        let old = w(
            1,
            1,
            "2024-03-14T20:00:00Z",
            "2026-11-20T10:00:00Z",
            TICK,
            100.0,
        );
        assert!(!recorded_as_it_happened(&old, at(NOW)));
        let fresh = w(
            2,
            1,
            "2026-11-20T09:00:00Z",
            "2026-11-20T10:00:00Z",
            TICK,
            100.0,
        );
        assert!(recorded_as_it_happened(&fresh, at(NOW)));
    }

    #[test]
    fn only_one_at_a_time_ways_count() {
        for how in ["bulk", "import", "anilist_first", "unknown"] {
            let x = w(
                1,
                1,
                "2026-11-20T09:00:00Z",
                "2026-11-20T09:00:00Z",
                how,
                25.0,
            );
            assert!(!recorded_as_it_happened(&x, at(NOW)), "{how}");
        }
        let mut none = w(
            1,
            1,
            "2026-11-20T09:00:00Z",
            "2026-11-20T09:00:00Z",
            TICK,
            25.0,
        );
        none.logged_at = None;
        assert!(!recorded_as_it_happened(&none, at(NOW)), "before badges");
        let mut undated = w(
            1,
            1,
            "2026-11-20T09:00:00Z",
            "2026-11-20T09:00:00Z",
            TICK,
            25.0,
        );
        undated.undated = true;
        assert!(!recorded_as_it_happened(&undated, at(NOW)));
    }

    #[test]
    fn a_future_watch_waits() {
        let later = w(
            1,
            1,
            "2026-12-02T09:00:00Z",
            "2026-12-02T09:00:00Z",
            TICK,
            25.0,
        );
        assert!(!recorded_as_it_happened(&later, at(NOW)));
    }

    #[test]
    fn five_films_in_two_minutes_are_one_burst() {
        let films: Vec<Logged> = (0..5)
            .map(|i| {
                let t = (at("2026-11-20T20:00:00Z") + Duration::seconds(25 * i)).stamp();
                w(i, i, &t, &t, TICK, 110.0)
            })
            .collect();
        assert!(eligible(&films, UTC, at(NOW)).is_empty());
    }

    #[test]
    fn three_films_two_hours_apart_keep_their_pace() {
        let films = vec![
            w(
                1,
                1,
                "2026-11-20T14:00:00Z",
                "2026-11-20T14:00:00Z",
                TICK,
                110.0,
            ),
            w(
                2,
                2,
                "2026-11-20T16:00:00Z",
                "2026-11-20T16:00:00Z",
                TICK,
                110.0,
            ),
            w(
                3,
                3,
                "2026-11-20T18:05:00Z",
                "2026-11-20T18:05:00Z",
                TICK,
                110.0,
            ),
        ];
        assert_eq!(eligible(&films, UTC, at(NOW)).len(), 3);
    }

    #[test]
    fn a_burst_takes_the_watch_before_it_along() {
        let x = vec![
            w(
                1,
                1,
                "2026-11-20T10:00:00Z",
                "2026-11-20T10:00:00Z",
                TICK,
                25.0,
            ),
            w(
                2,
                1,
                "2026-11-20T12:00:00Z",
                "2026-11-20T12:00:00Z",
                TICK,
                25.0,
            ),
            w(
                3,
                1,
                "2026-11-20T12:01:00Z",
                "2026-11-20T12:01:00Z",
                TICK,
                25.0,
            ),
            w(
                4,
                1,
                "2026-11-20T13:00:00Z",
                "2026-11-20T13:00:00Z",
                TICK,
                25.0,
            ),
        ];
        let ok = eligible(&x, UTC, at(NOW));
        assert_eq!(ok, HashSet::from([1, 4]));
    }

    #[test]
    fn eleven_titles_in_a_day_is_a_library_update() {
        let mut x: Vec<Logged> = (0..11)
            .map(|i| {
                let t = format!("2026-11-20T{:02}:00:00Z", 8 + i);
                w(i, i, &t, &t, TICK, 25.0)
            })
            .collect();
        assert!(eligible(&x, UTC, at(NOW)).is_empty());
        x.pop();
        assert_eq!(eligible(&x, UTC, at(NOW)).len(), 10);
    }

    #[test]
    fn an_import_the_same_day_makes_it_a_library_update() {
        let mut x = vec![
            w(
                1,
                1,
                "2026-11-20T14:00:00Z",
                "2026-11-20T14:00:00Z",
                TICK,
                110.0,
            ),
            w(
                2,
                2,
                "2026-11-20T17:00:00Z",
                "2026-11-20T17:00:00Z",
                TICK,
                110.0,
            ),
        ];
        for i in 0..30 {
            x.push(w(
                100 + i,
                100 + i,
                "2020-01-01T20:00:00Z",
                "2026-11-20T21:00:00Z",
                "import",
                100.0,
            ));
        }
        assert!(eligible(&x, UTC, at(NOW)).is_empty());
    }

    #[test]
    fn many_episodes_of_one_show_is_not_an_update() {
        let x: Vec<Logged> = (0..12)
            .map(|i| {
                let t = (at("2026-11-20T08:00:00Z") + Duration::minutes(25 * i)).stamp();
                w(i, 7, &t, &t, TICK, 24.0)
            })
            .collect();
        assert_eq!(eligible(&x, UTC, at(NOW)).len(), 12);
    }

    #[test]
    fn a_later_sync_keeps_its_pace_or_loses_it_whole() {
        let before = w(
            1,
            5,
            "2026-11-20T10:00:00Z",
            "2026-11-20T10:00:00Z",
            "anilist_first",
            24.0,
        );
        let two: Vec<Logged> = (0..2)
            .map(|i| {
                w(
                    10 + i,
                    5,
                    "2026-11-20T11:00:00Z",
                    "2026-11-20T11:00:00Z",
                    ANILIST,
                    24.0,
                )
            })
            .collect();
        let mut all = vec![before.clone()];
        all.extend(two.clone());
        assert_eq!(eligible(&all, UTC, at(NOW)).len(), 2);

        let many: Vec<Logged> = (0..24)
            .map(|i| {
                w(
                    20 + i,
                    5,
                    "2026-11-20T10:10:00Z",
                    "2026-11-20T10:10:00Z",
                    ANILIST,
                    24.0,
                )
            })
            .collect();
        let mut all = vec![before];
        all.extend(many);
        assert!(eligible(&all, UTC, at(NOW)).is_empty());
    }

    #[test]
    fn days_are_the_accounts_own() {
        let tokyo: Tz = "Asia/Tokyo".parse().unwrap();
        assert_eq!(
            day(at("2026-11-20T16:00:00Z"), tokyo),
            NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
        );
    }

    #[test]
    fn undo_carries_a_recent_record_and_nothing_older() {
        let now = Utc::now();
        let recent = (now - Duration::hours(3)).stamp();
        assert_eq!(carried(Some(&recent), Some(TICK)).1, TICK);
        let old = (now - Duration::days(30)).stamp();
        assert_eq!(carried(Some(&old), Some(TICK)).1, "unknown");
        assert_eq!(carried(Some(&recent), Some("made-up")).1, "unknown");
        assert_eq!(carried(None, None), (None, "unknown".to_string()));
    }
}
