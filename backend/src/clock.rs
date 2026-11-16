//! One way to write a moment.
//!
//! Every timestamp Frog stores is `YYYY-MM-DDTHH:MM:SSZ`, UTC to the second:
//! what imports and day-only watches (`…T20:00:00Z`) already wrote, and what
//! the database's own triggers write. SQL compares these as text (`<`, `MAX`,
//! `ORDER BY`), which is only right when every row has the same shape;
//! `chrono`'s `to_rfc3339` writes `+00:00` and nanoseconds instead, so it is
//! never used for a stored value. Migration 202611161800 rewrote the rows
//! written before.

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};

pub trait Stamp {
    /// The moment as Frog stores it.
    fn stamp(&self) -> String;
}

impl<Tz: TimeZone> Stamp for DateTime<Tz> {
    fn stamp(&self) -> String {
        self.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true)
    }
}

/// Now, as Frog stores it.
pub fn now() -> String {
    Utc::now().stamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_moment_is_written_one_way() {
        let at: DateTime<Utc> = "2026-11-16T09:30:05.123456789+00:00".parse().unwrap();
        assert_eq!(at.stamp(), "2026-11-16T09:30:05Z");
        let brussels = at.with_timezone(&chrono::FixedOffset::east_opt(3600).unwrap());
        assert_eq!(brussels.stamp(), "2026-11-16T09:30:05Z");
    }
}
