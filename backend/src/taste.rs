//! How alike two people rate things, for someone else's profile.
//!
//! The calculation is `score`, on plain values. Everyone rates towards the top
//! of the scale, so a plain average of the differences puts almost any two
//! people near 100; doubling it spreads the answer over the range that means
//! something: ten points apart on average reads as 80%, a coin toss around 50%.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;

/// Fewer titles than this in common says more about chance than about taste.
pub const MIN_SHARED: usize = 5;

/// Percent from pairs of ratings on the 0–100 scale, one pair per title. `None`
/// until there are enough of them.
pub fn score(pairs: &[(f64, f64)]) -> Option<u8> {
    if pairs.len() < MIN_SHARED {
        return None;
    }
    let apart = pairs.iter().map(|(a, b)| (a - b).abs()).sum::<f64>() / pairs.len() as f64;
    Some((100.0 - apart * 2.0).round().clamp(0.0, 100.0) as u8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TasteMatch {
    pub percent: u8,
    /// Titles both have rated.
    pub shared: usize,
}

/// A viewer's match with someone whose profile they may see, or `None` until
/// they share enough. The caller checks that they may; ratings stay in here and
/// only the two figures come out.
///
/// A show rated by season counts at the average of its seasons, since many
/// come that way from AniList and have no rating of their own.
pub async fn between(pool: &SqlitePool, viewer: i64, other: i64) -> AppResult<Option<TasteMatch>> {
    let pairs: Vec<(f64, f64)> = sqlx::query_as(
        "WITH rated AS (
             SELECT l.user_id, l.title_id,
                    COALESCE(l.rating, (SELECT AVG(s.rating) FROM season_rating s
                                         WHERE s.user_id = l.user_id
                                           AND s.title_id = l.title_id)) AS rating
               FROM library_entry l
              WHERE l.user_id IN (?1, ?2)
         )
         SELECT a.rating, b.rating
           FROM rated a JOIN rated b ON b.title_id = a.title_id
          WHERE a.user_id = ?1 AND b.user_id = ?2
            AND a.rating IS NOT NULL AND b.rating IS NOT NULL",
    )
    .bind(viewer)
    .bind(other)
    .fetch_all(pool)
    .await?;
    Ok(score(&pairs).map(|percent| TasteMatch {
        percent,
        shared: pairs.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(n: usize, a: f64, b: f64) -> Vec<(f64, f64)> {
        vec![(a, b); n]
    }

    #[test]
    fn it_stays_hidden_until_five_titles_are_shared() {
        assert_eq!(score(&[]), None);
        assert_eq!(score(&same(4, 80.0, 80.0)), None);
        assert_eq!(score(&same(5, 80.0, 80.0)), Some(100));
    }

    #[test]
    fn the_same_ratings_are_a_full_match() {
        assert_eq!(score(&same(8, 35.0, 35.0)), Some(100));
    }

    #[test]
    fn ten_points_apart_reads_as_eighty() {
        assert_eq!(score(&same(5, 80.0, 70.0)), Some(80));
    }

    #[test]
    fn it_does_not_care_who_is_who() {
        let pairs = [
            (90.0, 40.0),
            (70.0, 75.0),
            (20.0, 60.0),
            (100.0, 100.0),
            (55.0, 50.0),
        ];
        let flipped: Vec<(f64, f64)> = pairs.iter().map(|(a, b)| (*b, *a)).collect();
        assert_eq!(score(&pairs), score(&flipped));
    }

    #[test]
    fn opposites_stop_at_nothing() {
        assert_eq!(score(&same(5, 100.0, 0.0)), Some(0));
        assert_eq!(score(&same(5, 90.0, 20.0)), Some(0), "never below zero");
        assert_eq!(score(&same(5, 75.0, 50.0)), Some(50));
    }

    #[test]
    fn differences_do_not_cancel_out() {
        let pairs = [
            (90.0, 70.0),
            (70.0, 90.0),
            (90.0, 70.0),
            (70.0, 90.0),
            (90.0, 70.0),
        ];
        assert_eq!(score(&pairs), Some(60));
    }

    #[test]
    fn it_rounds_to_a_whole_percent() {
        let pairs = [
            (80.0, 80.0),
            (80.0, 80.0),
            (80.0, 80.0),
            (80.0, 79.0),
            (80.0, 79.0),
            (80.0, 79.0),
        ];
        assert_eq!(score(&pairs), Some(99));
    }
}
