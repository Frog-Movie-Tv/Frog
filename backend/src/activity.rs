//! What everyone here has been watching and reading, the viewer included, for a
//! row on Home and the Activity page behind it. A manga's chapters read (`chapter_read`)
//! come in as its episodes do: one with no season, its number the chapter.
//!
//! This reads one account's diary for another, so what it may touch is narrow on
//! purpose. Only accounts that are switched on and have their profile on
//! (`shown_to`). Only dated watches, because an undated one holds the day it was
//! recorded, not a day anyone watched anything. Only titles still in that
//! library, because taking one out is how a person says it is not to be seen.
//! No notes: no query here selects them and no type here has a field for them.
//!
//! A card is one title on one day, the viewer's day, so the row on Home and
//! the page agree: "2 episodes, today" means two today. Home keeps each
//! person's newest day of a title; the page has every day.
//!
//! Deciding what a run of watches adds up to is `summarise`, on plain values, so
//! it is tested without a database.

use crate::clock::Stamp;
use std::collections::HashSet;

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::store;

/// How far back the row looks.
const WINDOW_DAYS: i64 = 14;
/// The most the row shows. Activity has the rest.
const ROW_LENGTH: usize = 12;

/// One watch, as `summarise` needs it.
#[derive(Debug, Clone)]
pub struct Watch {
    pub title_id: i64,
    pub name: String,
    pub kind: String,
    pub poster_path: Option<String>,
    pub watched_at: String,
    pub is_rewatch: bool,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// The last regular episode of its show.
    pub last_episode: bool,
    /// The show or film stands as completed in that library.
    pub completed: bool,
}

/// What a person did with a title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Did {
    Finished,
    Watched,
    Rewatched,
}

/// The title a card is about.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Title {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub poster_path: Option<String>,
}

/// One title's worth of watching by one person. Newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub title: Title,
    pub did: Did,
    /// How many episodes; zero for a film.
    pub episodes: usize,
    /// Which one, when there was just the one.
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// When the newest of them was.
    pub at: String,
}

/// The day a moment falls on in `zone`, as YYYY-MM-DD; the date as written
/// when it cannot be read as a moment.
fn day_of(at: &str, zone: Tz) -> String {
    DateTime::parse_from_rfc3339(at).map_or_else(
        |_| at.get(..10).unwrap_or(at).to_string(),
        |m| m.with_timezone(&zone).date_naive().to_string(),
    )
}

/// The moment the day after `at`'s day begins in `zone`, as RFC 3339: every
/// watch of that day comes before it.
fn end_of_day(at: &str, zone: Tz) -> Option<String> {
    let day = DateTime::parse_from_rfc3339(at)
        .ok()?
        .with_timezone(&zone)
        .date_naive()
        .succ_opt()?;
    let midnight = zone
        .from_local_datetime(&day.and_time(NaiveTime::MIN))
        .earliest()?;
    Some(midnight.with_timezone(&Utc).stamp())
}

/// Add up one person's watches, newest first, into a card per title per day
/// in `zone`, newest first, each with its day.
fn by_day(watches: &[Watch], zone: Tz) -> Vec<(String, Summary)> {
    let mut groups: Vec<(String, Vec<&Watch>)> = Vec::new();
    for watch in watches {
        let day = day_of(&watch.watched_at, zone);
        match groups
            .iter_mut()
            .find(|(d, g)| *d == day && g[0].title_id == watch.title_id)
        {
            Some((_, group)) => group.push(watch),
            None => groups.push((day, vec![watch])),
        }
    }
    groups
        .into_iter()
        .map(|(day, group)| (day, summary(&group)))
        .collect()
}

/// Add up one person's watches, newest first, into a card per title per day
/// in `zone`.
pub fn summarise(watches: &[Watch], zone: Tz) -> Vec<Summary> {
    by_day(watches, zone).into_iter().map(|(_, s)| s).collect()
}

fn summary(group: &[&Watch]) -> Summary {
    let newest = group[0];
    let episodes = group.iter().filter(|w| w.episode.is_some()).count();
    // A manga is finished by its last chapter as a show is by its last episode.
    let is_show = newest.kind != "movie";
    let did = if group.iter().all(|w| w.is_rewatch) {
        Did::Rewatched
    } else if is_show && newest.completed && group.iter().any(|w| w.last_episode) {
        Did::Finished
    } else {
        Did::Watched
    };
    let one = episodes == 1;
    Summary {
        title: Title {
            id: newest.title_id,
            kind: newest.kind.clone(),
            name: newest.name.clone(),
            poster_path: newest.poster_path.clone(),
        },
        did,
        episodes,
        season: newest.season.filter(|_| one),
        episode: newest.episode.filter(|_| one),
        at: newest.watched_at.clone(),
    }
}

/// A card as Home draws it. Built field by field: a new column on a watch stays
/// private until someone names it here.
#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub who: store::Shown,
    #[serde(flatten)]
    pub summary: Summary,
}

/// Everyone's cards, newest first, cut to the length of the row.
fn row(mut cards: Vec<Card>) -> Vec<Card> {
    cards.sort_by(|a, b| b.summary.at.cmp(&a.summary.at));
    cards.truncate(ROW_LENGTH);
    cards
}

/// (title id, name, kind, poster, watched at, rewatch, season, episode, last
/// regular episode, completed)
type WatchRow = (
    i64,
    String,
    String,
    Option<String>,
    String,
    bool,
    Option<i64>,
    Option<i64>,
    bool,
    bool,
);

/// One person's dated watches and reads, newest first: from `from` (when
/// given) up to but not including `to`, at most `limit`.
async fn watches(
    pool: &SqlitePool,
    user_id: i64,
    from: Option<&str>,
    to: &str,
    limit: i64,
) -> AppResult<Vec<Watch>> {
    let rows: Vec<WatchRow> = sqlx::query_as(
        "SELECT w.title_id, t.name, t.kind, t.poster_path, w.watched_at AS at, w.is_rewatch,
                e.season_number, e.episode_number,
                COALESCE(e.id = (SELECT f.id FROM episode f
                                  WHERE f.title_id = w.title_id AND f.season_number > 0
                                  ORDER BY f.season_number DESC, f.episode_number DESC
                                  LIMIT 1), 0),
                l.status = 'completed'
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ?1 AND w.undated = 0
            AND (?2 IS NULL OR w.watched_at >= ?2) AND w.watched_at < ?3
         UNION ALL
         SELECT r.title_id, t.name, t.kind, t.poster_path, r.read_at AS at, 0,
                NULL, r.chapter, COALESCE(r.chapter = t.episode_count, 0),
                l.status = 'completed'
           FROM chapter_read r
           JOIN title t ON t.id = r.title_id
           JOIN library_entry l ON l.title_id = r.title_id AND l.user_id = r.user_id
          WHERE r.user_id = ?1 AND (?2 IS NULL OR r.read_at >= ?2) AND r.read_at < ?3
          ORDER BY at DESC
          LIMIT ?4",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                title_id,
                name,
                kind,
                poster_path,
                watched_at,
                is_rewatch,
                season,
                episode,
                last_episode,
                completed,
            )| Watch {
                title_id,
                name,
                kind,
                poster_path,
                watched_at,
                is_rewatch,
                season,
                episode,
                last_episode,
                completed,
            },
        )
        .collect())
}

/// Whose watching the viewer sees: their own, whatever their profile says, then
/// everyone they may see.
async fn people(pool: &SqlitePool, viewer: i64) -> AppResult<Vec<store::ShownAccount>> {
    let mut people = vec![store::myself(pool, viewer).await?];
    people.extend(store::shown_to(pool, viewer).await?);
    Ok(people)
}

/// The row for one viewer: themselves and everyone they may see, each
/// person's newest day of each title.
pub async fn feed(pool: &SqlitePool, viewer: i64) -> AppResult<Vec<Card>> {
    let zone = crate::announce::time_zone(pool, viewer).await;
    let now = Utc::now();
    let from = (now - Duration::days(WINDOW_DAYS)).stamp();
    let to = (now + Duration::seconds(1)).stamp();
    let mut cards = Vec::new();
    for person in people(pool, viewer).await? {
        let watches = watches(pool, person.id, Some(&from), &to, i64::MAX).await?;
        let mut shown = HashSet::new();
        cards.extend(
            summarise(&watches, zone)
                .into_iter()
                .filter(|s| shown.insert(s.title.id))
                .map(|summary| Card {
                    who: person.shown.clone(),
                    summary,
                }),
        );
    }
    Ok(row(cards))
}

/// Watches read at first for one person's page of Activity: a page of 40
/// cards rarely needs more. Grown fourfold while a day may be cut short.
const PAGE_WATCHES: i64 = 400;
/// The most read for one person's page, however long their days.
const MOST_WATCHES: i64 = 25_600;

/// Everything everyone the viewer may see has watched, all of it, a title a
/// day per card in the viewer's zone, newest first, `limit` at a time before
/// `before`. The second value is where the next page starts, when there is
/// one.
pub async fn everything(
    pool: &SqlitePool,
    viewer: i64,
    before: Option<&str>,
    limit: usize,
) -> AppResult<(Vec<Card>, Option<String>)> {
    let zone = crate::announce::time_zone(pool, viewer).await;
    // Read to the end of the day `before` falls on, so its card is whole.
    let to = before
        .and_then(|b| end_of_day(b, zone))
        .unwrap_or_else(|| "9999".into());
    let mut cards = Vec::new();
    for person in people(pool, viewer).await? {
        let mut want = PAGE_WATCHES;
        loop {
            let watches = watches(pool, person.id, None, &to, want).await?;
            let cut = watches.len() as i64 == want;
            let oldest = watches.last().map(|w| day_of(&w.watched_at, zone));
            let mut found: Vec<Summary> = by_day(&watches, zone)
                .into_iter()
                // The oldest day read may go on past what was read.
                .filter(|(day, _)| !cut || Some(day) != oldest.as_ref())
                .map(|(_, s)| s)
                .filter(|s| before.is_none_or(|b| s.at.as_str() < b))
                .collect();
            if found.len() > limit || !cut || want >= MOST_WATCHES {
                found.truncate(limit + 1);
                cards.extend(found.into_iter().map(|summary| Card {
                    who: person.shown.clone(),
                    summary,
                }));
                break;
            }
            want *= 4;
        }
    }
    cards.sort_by(|a, b| b.summary.at.cmp(&a.summary.at));
    let next = (cards.len() > limit).then(|| cards[limit - 1].summary.at.clone());
    cards.truncate(limit);
    Ok((cards, next))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn film(id: i64, name: &str, at: &str) -> Watch {
        Watch {
            title_id: id,
            name: name.into(),
            kind: "movie".into(),
            poster_path: None,
            watched_at: at.into(),
            is_rewatch: false,
            season: None,
            episode: None,
            last_episode: false,
            completed: true,
        }
    }

    fn episode(id: i64, name: &str, number: i64, at: &str) -> Watch {
        Watch {
            kind: "tv".into(),
            season: Some(1),
            episode: Some(number),
            completed: false,
            ..film(id, name, at)
        }
    }

    #[test]
    fn one_episode_says_which_one() {
        let cards = summarise(&[episode(1, "Dark", 3, "2026-11-01")], Tz::UTC);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].did, Did::Watched);
        assert_eq!(cards[0].episodes, 1);
        assert_eq!((cards[0].season, cards[0].episode), (Some(1), Some(3)));
    }

    #[test]
    fn a_binge_is_one_card_with_a_count() {
        let watches: Vec<Watch> = (1..=6)
            .rev()
            .map(|n| episode(1, "Dark", n, &format!("2026-11-01T2{n}:00:00Z")))
            .collect();
        let cards = summarise(&watches, Tz::UTC);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].episodes, 6);
        assert_eq!((cards[0].season, cards[0].episode), (None, None));
        assert_eq!(cards[0].at, "2026-11-01T26:00:00Z", "the newest of them");
    }

    #[test]
    fn a_show_is_finished_only_by_its_last_episode() {
        let last = Watch {
            last_episode: true,
            completed: true,
            ..episode(1, "Dark", 26, "2026-11-01")
        };
        assert_eq!(
            summarise(std::slice::from_ref(&last), Tz::UTC)[0].did,
            Did::Finished
        );

        // Completed, but this week's watch was a special afterwards.
        let special = Watch {
            completed: true,
            ..episode(1, "Dark", 2, "2026-11-01")
        };
        assert_eq!(summarise(&[special], Tz::UTC)[0].did, Did::Watched);

        // The last episode of a show still going is not the end of it.
        let running = Watch {
            completed: false,
            ..last
        };
        assert_eq!(summarise(&[running], Tz::UTC)[0].did, Did::Watched);
    }

    #[test]
    fn a_film_is_watched_never_finished() {
        let cards = summarise(&[film(2, "Heat", "2026-11-01")], Tz::UTC);
        assert_eq!(cards[0].did, Did::Watched);
        assert_eq!(cards[0].episodes, 0);
    }

    #[test]
    fn a_rewatch_says_so_unless_something_new_is_in_it() {
        let again = Watch {
            is_rewatch: true,
            ..film(2, "Heat", "2026-11-01")
        };
        assert_eq!(summarise(&[again], Tz::UTC)[0].did, Did::Rewatched);

        let finale = Watch {
            is_rewatch: true,
            last_episode: true,
            completed: true,
            ..episode(1, "Dark", 26, "2026-11-01T22:00:00Z")
        };
        assert_eq!(
            summarise(std::slice::from_ref(&finale), Tz::UTC)[0].did,
            Did::Rewatched
        );
        let mixed = [
            finale,
            Watch {
                is_rewatch: false,
                ..episode(1, "Dark", 25, "2026-11-01T21:00:00Z")
            },
        ];
        assert_eq!(summarise(&mixed, Tz::UTC)[0].did, Did::Finished);
    }

    /// Two episodes a week apart are two cards, each with its own count: a
    /// card's count is what was watched on its day.
    #[test]
    fn each_day_is_its_own_card() {
        let watches = [
            episode(1, "Black Clover", 7, "2026-11-16T10:00:00Z"),
            episode(1, "Black Clover", 6, "2026-11-09T10:00:00Z"),
        ];
        let cards = summarise(&watches, Tz::UTC);
        assert_eq!(cards.len(), 2);
        assert_eq!((cards[0].episodes, cards[0].episode), (1, Some(7)));
        assert_eq!((cards[1].episodes, cards[1].episode), (1, Some(6)));
    }

    /// The day is the viewer's: 23:30 and 00:30 in Brussels are two days,
    /// though both fall on one day in UTC.
    #[test]
    fn a_day_is_the_viewers_day() {
        let watches = [
            episode(1, "Dark", 2, "2026-11-15T23:30:00Z"), // 00:30 Monday
            episode(1, "Dark", 1, "2026-11-15T22:30:00Z"), // 23:30 Sunday
        ];
        assert_eq!(summarise(&watches, Tz::UTC).len(), 1);
        let brussels: Tz = "Europe/Brussels".parse().unwrap();
        assert_eq!(summarise(&watches, brussels).len(), 2);
        assert_eq!(day_of("2026-11-15T23:30:00Z", brussels), "2026-11-16");
        assert_eq!(
            end_of_day("2026-11-15T22:30:00Z", brussels).as_deref(),
            Some("2026-11-15T23:00:00Z"),
            "Sunday ends at midnight in Brussels"
        );
    }

    #[test]
    fn a_large_import_is_a_card_per_title() {
        let watches: Vec<Watch> = (0..200)
            .map(|n| {
                film(
                    n,
                    &format!("Film {n}"),
                    &format!("2026-11-01T00:00:{n:02}Z"),
                )
            })
            .collect();
        let cards = summarise(&watches, Tz::UTC);
        assert_eq!(cards.len(), 200);
        assert_eq!(cards[0].title.name, "Film 0", "in the order they came");
    }

    #[test]
    fn nothing_watched_is_no_cards() {
        assert!(summarise(&[], Tz::UTC).is_empty());
    }

    #[test]
    fn the_row_is_newest_first_and_ends() {
        let cards: Vec<Card> = (0..30)
            .map(|n| Card {
                who: store::Shown {
                    username: "mihai".into(),
                    display_name: None,
                    avatar: None,
                },
                summary: summarise(&[film(n, "Heat", &format!("2026-11-{n:02}"))], Tz::UTC)
                    .remove(0),
            })
            .collect();
        let kept = row(cards);
        assert_eq!(kept.len(), ROW_LENGTH);
        assert_eq!(kept[0].summary.at, "2026-11-29");
    }

    /// A card is all that leaves this module, so a field for private text could
    /// only arrive by being added to `Summary` or `Shown`.
    #[test]
    fn what_goes_out_has_no_notes() {
        let card =
            serde_json::to_value(&summarise(&[film(2, "Heat", "2026-11-01")], Tz::UTC)[0]).unwrap();
        let keys: Vec<&String> = card.as_object().unwrap().keys().collect();
        assert!(!keys.iter().any(|k| k.contains("note")), "{keys:?}");
    }
}
