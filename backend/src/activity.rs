//! What other people here have been watching, for a row on Home.
//!
//! This reads one account's diary for another, so what it may touch is narrow on
//! purpose. Only accounts that are switched on and have their profile on
//! (`shown_to`). Only dated watches, because an undated one holds the day it was
//! recorded, not a day anyone watched anything. Only titles still in that
//! library, because taking one out is how a person says it is not to be seen.
//! No notes: no query here selects them and no type here has a field for them.
//!
//! Deciding what a run of watches adds up to is `summarise`, on plain values, so
//! it is tested without a database.

use chrono::{Duration, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::store;

/// How far back the row looks.
const WINDOW_DAYS: i64 = 14;
/// Titles one person gets in the row. Anything past it is a number on their
/// last card, so one binge or one import cannot fill the row.
const TITLES_EACH: usize = 3;
/// The most the row shows.
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
    /// Other titles this person watched that did not get a card.
    pub also: usize,
}

/// Add up one person's watches, newest first, into at most `TITLES_EACH` cards.
pub fn summarise(watches: &[Watch]) -> Vec<Summary> {
    let mut titles: Vec<Vec<&Watch>> = Vec::new();
    for watch in watches {
        match titles.iter_mut().find(|t| t[0].title_id == watch.title_id) {
            Some(group) => group.push(watch),
            None => titles.push(vec![watch]),
        }
    }

    let others = titles.len().saturating_sub(TITLES_EACH);
    let mut cards: Vec<Summary> = titles
        .iter()
        .take(TITLES_EACH)
        .map(|group| summary(group))
        .collect();
    if let Some(last) = cards.last_mut() {
        last.also = others;
    }
    cards
}

fn summary(group: &[&Watch]) -> Summary {
    let newest = group[0];
    let episodes = group.iter().filter(|w| w.episode.is_some()).count();
    let is_show = newest.kind == "tv";
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
        also: 0,
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

async fn recent_watches(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Watch>> {
    let now = Utc::now();
    let rows: Vec<WatchRow> = sqlx::query_as(
        "SELECT w.title_id, t.name, t.kind, t.poster_path, w.watched_at, w.is_rewatch,
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
          WHERE w.user_id = ? AND w.undated = 0 AND w.watched_at >= ? AND w.watched_at <= ?
          ORDER BY w.watched_at DESC",
    )
    .bind(user_id)
    .bind((now - Duration::days(WINDOW_DAYS)).to_rfc3339())
    .bind(now.to_rfc3339())
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

/// The row for one viewer: everyone they may see, and never themselves.
pub async fn feed(pool: &SqlitePool, viewer: i64) -> AppResult<Vec<Card>> {
    let mut cards = Vec::new();
    for person in store::shown_to(pool, viewer).await? {
        let watches = recent_watches(pool, person.id).await?;
        cards.extend(summarise(&watches).into_iter().map(|summary| Card {
            who: person.shown.clone(),
            summary,
        }));
    }
    Ok(row(cards))
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
        let cards = summarise(&[episode(1, "Dark", 3, "2026-11-01")]);
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
        let cards = summarise(&watches);
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
        assert_eq!(summarise(std::slice::from_ref(&last))[0].did, Did::Finished);

        // Completed, but this week's watch was a special afterwards.
        let special = Watch {
            completed: true,
            ..episode(1, "Dark", 2, "2026-11-01")
        };
        assert_eq!(summarise(&[special])[0].did, Did::Watched);

        // The last episode of a show still going is not the end of it.
        let running = Watch {
            completed: false,
            ..last
        };
        assert_eq!(summarise(&[running])[0].did, Did::Watched);
    }

    #[test]
    fn a_film_is_watched_never_finished() {
        let cards = summarise(&[film(2, "Heat", "2026-11-01")]);
        assert_eq!(cards[0].did, Did::Watched);
        assert_eq!(cards[0].episodes, 0);
    }

    #[test]
    fn a_rewatch_says_so_unless_something_new_is_in_it() {
        let again = Watch {
            is_rewatch: true,
            ..film(2, "Heat", "2026-11-01")
        };
        assert_eq!(summarise(&[again])[0].did, Did::Rewatched);

        let finale = Watch {
            is_rewatch: true,
            last_episode: true,
            completed: true,
            ..episode(1, "Dark", 26, "2026-11-02")
        };
        assert_eq!(
            summarise(std::slice::from_ref(&finale))[0].did,
            Did::Rewatched
        );
        let mixed = [
            finale,
            Watch {
                is_rewatch: false,
                ..episode(1, "Dark", 25, "2026-11-01")
            },
        ];
        assert_eq!(summarise(&mixed)[0].did, Did::Finished);
    }

    #[test]
    fn a_large_import_stays_three_cards_and_a_number() {
        let watches: Vec<Watch> = (0..200)
            .map(|n| {
                film(
                    n,
                    &format!("Film {n}"),
                    &format!("2026-11-01T00:00:{n:02}Z"),
                )
            })
            .collect();
        let cards = summarise(&watches);
        assert_eq!(cards.len(), TITLES_EACH);
        assert_eq!(cards[0].title.name, "Film 0", "in the order they came");
        assert_eq!(cards[TITLES_EACH - 1].also, 197, "on the last card");
        assert_eq!(cards[0].also, 0);
    }

    #[test]
    fn nothing_watched_is_no_cards() {
        assert!(summarise(&[]).is_empty());
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
                summary: summarise(&[film(n, "Heat", &format!("2026-11-{n:02}"))]).remove(0),
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
        let card = serde_json::to_value(&summarise(&[film(2, "Heat", "2026-11-01")])[0]).unwrap();
        let keys: Vec<&String> = card.as_object().unwrap().keys().collect();
        assert!(!keys.iter().any(|k| k.contains("note")), "{keys:?}");
    }
}
