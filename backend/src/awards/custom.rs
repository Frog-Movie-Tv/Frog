//! Badges made here that Frog gives on its own once a count is reached. The
//! administrator picks what is counted, from the ladders `rules` already
//! climbs, and how far. Pure: `super::run` reads them and `super::give`
//! gives them.

use serde::{Deserialize, Serialize};

use super::rules::{Earned, Evidence, Tally};

/// What a badge made here can count: every ladder `rules::climb` keeps, so
/// the day a count was reached is known as well as the count.
pub const COUNTS: [&str; 17] = [
    "films",
    "episodes",
    "hours",
    "library",
    "completed",
    "ratings",
    "favourites",
    "notes",
    "chapters",
    "manga",
    "rewatches",
    "extension",
    "together",
    "years",
    "anime",
    "seasons",
    "days",
];

/// The highest a count can be asked to reach.
pub const MOST: i64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub count: String,
    pub at: i64,
}

impl Condition {
    /// Why it cannot be, in words for the administrator.
    pub fn check(&self) -> Result<(), &'static str> {
        if !COUNTS.contains(&self.count.as_str()) {
            return Err("Frog cannot count that.");
        }
        if !(1..=MOST).contains(&self.at) {
            return Err("Pick a number from 1 to 1,000,000.");
        }
        Ok(())
    }

    /// As kept on the badge; anything unreadable counts as no condition.
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str::<Self>(text)
            .ok()
            .filter(|c| c.check().is_ok())
    }

    /// Its key where progress is kept, beside the catalogue's.
    pub fn key(badge_id: i64) -> String {
        format!("custom:{badge_id}")
    }
}

/// How far along one account is, and, once it has got there, the day it did
/// and what did it.
pub fn reach(tally: &Tally, badge_id: i64, c: &Condition) -> (f64, Option<Earned>) {
    let Some(steps) = tally.reached.get(&c.count) else {
        return (0.0, None);
    };
    let have = steps.last().map_or(0.0, |r| r.sum);
    let at = c.at as f64;
    let earned = steps.iter().find(|r| r.sum >= at).map(|r| Earned {
        key: Condition::key(badge_id),
        on: r.on,
        evidence: Evidence {
            title: r.title,
            watches: r.watch.into_iter().collect(),
        },
    });
    (have, earned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::awards::rules::Reached;
    use chrono::NaiveDate;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    #[test]
    fn a_count_is_reached_on_the_day_of_the_thing_that_reached_it() {
        let mut tally = Tally::default();
        tally.reached.insert(
            "films".into(),
            (1..=4)
                .map(|n| Reached {
                    sum: n as f64,
                    on: day(n),
                    title: Some(100 + n as i64),
                    watch: Some(n as i64),
                })
                .collect(),
        );
        let three = Condition {
            count: "films".into(),
            at: 3,
        };
        let (have, earned) = reach(&tally, 7, &three);
        assert_eq!(have, 4.0);
        let earned = earned.unwrap();
        assert_eq!(earned.key, "custom:7");
        assert_eq!(earned.on, day(3));
        assert_eq!(earned.evidence.title, Some(103));

        let ten = Condition {
            count: "films".into(),
            at: 10,
        };
        assert_eq!(reach(&tally, 7, &ten), (4.0, None));
        let unseen = Condition {
            count: "episodes".into(),
            at: 1,
        };
        assert_eq!(reach(&tally, 7, &unseen), (0.0, None));
    }

    #[test]
    fn only_what_frog_counts_and_a_sensible_number() {
        let ok = |count: &str, at| {
            Condition {
                count: count.into(),
                at,
            }
            .check()
            .is_ok()
        };
        assert!(ok("films", 25));
        assert!(!ok("streak", 3), "a streak is no running count");
        assert!(!ok("films", 0));
        assert!(!ok("films", MOST + 1));
        assert_eq!(
            Condition::parse(r#"{"count":"hours","at":100}"#),
            Some(Condition {
                count: "hours".into(),
                at: 100
            })
        );
        assert_eq!(Condition::parse(r#"{"count":"nope","at":1}"#), None);
        for count in COUNTS {
            assert!(
                crate::awards::catalogue::ladder(count).next().is_some(),
                "{count} is a ladder"
            );
        }
    }
}
