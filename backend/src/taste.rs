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

/// One account's library entry, as a comparison sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Held {
    pub title_id: i64,
    pub status: String,
    /// Its own rating, or the average of its seasons'.
    pub rating: Option<f64>,
}

/// Two libraries side by side: what both have, and what only one does.
#[derive(Debug, Default, PartialEq)]
pub struct Compared {
    /// (mine, theirs), the titles both rated first, then by how far apart
    /// the ratings are, the closest first.
    pub both: Vec<(Held, Held)>,
    pub only_mine: Vec<Held>,
    pub only_theirs: Vec<Held>,
}

/// Lay two libraries side by side. The decision, on plain values.
pub fn compare(mine: &[Held], theirs: &[Held]) -> Compared {
    let mut out = Compared::default();
    for m in mine {
        match theirs.iter().find(|t| t.title_id == m.title_id) {
            Some(t) => out.both.push((m.clone(), t.clone())),
            None => out.only_mine.push(m.clone()),
        }
    }
    out.only_theirs = theirs
        .iter()
        .filter(|t| !mine.iter().any(|m| m.title_id == t.title_id))
        .cloned()
        .collect();
    let apart = |(m, t): &(Held, Held)| match (m.rating, t.rating) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => 1000.0,
    };
    out.both.sort_by(|a, b| apart(a).total_cmp(&apart(b)));
    // What the other loved first: the best leads to recommend.
    let best = |h: &Held| -h.rating.unwrap_or(-1.0);
    out.only_theirs.sort_by(|a, b| best(a).total_cmp(&best(b)));
    out.only_mine.sort_by(|a, b| best(a).total_cmp(&best(b)));
    out
}

/// One account's whole library for a comparison.
pub async fn library_of(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Held>> {
    let rows: Vec<(i64, String, Option<f64>)> = sqlx::query_as(
        "SELECT l.title_id, l.status,
                COALESCE(l.rating, (SELECT AVG(s.rating) FROM season_rating s
                                     WHERE s.user_id = l.user_id AND s.title_id = l.title_id))
           FROM library_entry l
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(title_id, status, rating)| Held {
            title_id,
            status,
            rating,
        })
        .collect())
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

    #[test]
    fn two_libraries_split_into_shared_and_their_own() {
        let held = |id: i64, rating: Option<f64>| Held {
            title_id: id,
            status: "completed".into(),
            rating,
        };
        let mine = [held(1, Some(80.0)), held(2, Some(90.0)), held(3, None)];
        let theirs = [
            held(2, Some(88.0)),
            held(1, Some(40.0)),
            held(4, Some(95.0)),
            held(5, None),
        ];
        let c = compare(&mine, &theirs);
        // Closest ratings first.
        let both: Vec<i64> = c.both.iter().map(|(m, _)| m.title_id).collect();
        assert_eq!(both, [2, 1]);
        assert_eq!(
            c.only_mine.iter().map(|h| h.title_id).collect::<Vec<_>>(),
            [3]
        );
        // Their best first.
        assert_eq!(
            c.only_theirs.iter().map(|h| h.title_id).collect::<Vec<_>>(),
            [4, 5]
        );
    }
}
