//! What each account is told when something it follows comes out.
//!
//! An episode is announced at the moment it airs when AniList or TVmaze gave
//! a time (`episode.air_at`, `crate::airtime`), and otherwise at midnight on
//! TMDB's day in the account's
//! own time zone, which the browser reports when notifications are switched
//! on. A film on someone's list is announced at midnight on its release day.
//! Each is announced once (`push_sent`, `push_sent_title`), never after it
//! was watched, and not when it is more than a day and a half late, which is
//! what a server that was off would otherwise send in a burst.
//!
//! The worker asks every minute (`crate::jobs`); `due` decides and
//! `Announcement::payload` is what the notification says.

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::airtime::stamp;
use crate::error::AppResult;
use crate::state::AppState;

/// The latest an announcement still goes out after its moment.
const LATE_HOURS: i64 = 36;

/// `app_setting` key holding the account's IANA zone ("Europe/Brussels").
pub const TIME_ZONE_KEY: &str = "time_zone";

/// One notification: every new episode of one show, or one film.
#[derive(Debug, PartialEq)]
pub struct Announcement {
    /// Empty for a film.
    pub episode_ids: Vec<i64>,
    /// Frog's id for the title.
    pub title_id: i64,
    pub name: String,
    pub film: bool,
    /// The wide picture for the notification: backdrop, else still, else poster.
    pub picture: Option<String>,
    /// (season, episode, name)
    pub episodes: Vec<(i64, i64, Option<String>)>,
}

/// The zone the account last reported; UTC until it has.
pub async fn time_zone(pool: &SqlitePool, user_id: i64) -> Tz {
    let raw: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = ?")
            .bind(user_id)
            .bind(TIME_ZONE_KEY)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    raw.and_then(|(z,)| z.parse().ok()).unwrap_or(Tz::UTC)
}

/// Keep the zone a browser reported, when it is one.
pub async fn set_time_zone(pool: &SqlitePool, user_id: i64, zone: &str) -> AppResult<bool> {
    if zone.parse::<Tz>().is_err() {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO app_setting (user_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(TIME_ZONE_KEY)
    .bind(zone)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(true)
}

/// (episode id, title id, show, backdrop, still, poster, season, episode, episode name)
type EpisodeRow = (
    i64,
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    i64,
    Option<String>,
);

/// (title id, film, backdrop, poster)
type FilmRow = (i64, String, Option<String>, Option<String>);

/// What is due for one account at `now`, in its zone `zone`.
pub async fn due(
    pool: &SqlitePool,
    user_id: i64,
    now: DateTime<Utc>,
    zone: Tz,
) -> AppResult<Vec<Announcement>> {
    let today: NaiveDate = now.with_timezone(&zone).date_naive();
    let yesterday = today - chrono::Duration::days(1);
    let since = stamp(now - chrono::Duration::hours(LATE_HOURS));

    let rows: Vec<EpisodeRow> = sqlx::query_as(&format!(
        "SELECT e.id, t.id, t.name, t.backdrop_path, e.still_path, t.poster_path,
                e.season_number, e.episode_number, e.name
           FROM episode e
           JOIN title t ON t.id = e.title_id
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = ?1
          WHERE t.kind = 'tv'
            AND {followed}
            AND l.status != 'completed'
            AND l.notify = 1
            AND e.season_number > 0
            AND ((e.air_at IS NOT NULL AND e.air_at <= ?2 AND e.air_at > ?3)
                 OR (e.air_at IS NULL AND e.air_date IN (?4, ?5)))
            AND NOT EXISTS (
                  SELECT 1 FROM push_sent p
                   WHERE p.user_id = ?1 AND p.episode_id = e.id)
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ?1 AND w.episode_id = e.id)
          ORDER BY t.name, e.season_number, e.episode_number",
        followed = crate::release::FOLLOWED
    ))
    .bind(user_id)
    .bind(stamp(now))
    .bind(&since)
    .bind(today.to_string())
    .bind(yesterday.to_string())
    .fetch_all(pool)
    .await?;

    let mut out: Vec<Announcement> = Vec::new();
    for (id, title_id, show, backdrop, still, poster, season, episode, name) in rows {
        match out.last_mut() {
            Some(a) if a.title_id == title_id => {
                a.episode_ids.push(id);
                a.episodes.push((season, episode, name));
            }
            _ => out.push(Announcement {
                episode_ids: vec![id],
                title_id,
                name: show,
                film: false,
                picture: backdrop.or(still).or(poster),
                episodes: vec![(season, episode, name)],
            }),
        }
    }

    // Films: TMDB's release day, at midnight here.
    let films: Vec<FilmRow> = sqlx::query_as(
        "SELECT t.id, t.name, t.backdrop_path, t.poster_path
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?1
            AND t.kind = 'movie'
            AND l.notify = 1
            AND l.status NOT IN ('completed', 'dropped')
            AND t.release_date IN (?2, ?3)
            AND NOT EXISTS (
                  SELECT 1 FROM push_sent_title p
                   WHERE p.user_id = ?1 AND p.title_id = t.id)
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ?1 AND w.title_id = t.id)
          ORDER BY t.name",
    )
    .bind(user_id)
    .bind(today.to_string())
    .bind(yesterday.to_string())
    .fetch_all(pool)
    .await?;
    out.extend(
        films
            .into_iter()
            .map(|(title_id, name, backdrop, poster)| Announcement {
                episode_ids: Vec::new(),
                title_id,
                name,
                film: true,
                picture: backdrop.or(poster),
                episodes: Vec::new(),
            }),
    );
    Ok(out)
}

/// TMDB's stand-in for a name it does not have yet: "Episode 4".
fn real_name(name: &Option<String>, number: i64) -> Option<&str> {
    name.as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty() && *n != format!("Episode {number}"))
}

impl Announcement {
    /// The notification: the title, what came out, its wide picture. The
    /// service worker adds Frog's icon and badge.
    pub fn payload(&self) -> Value {
        let code = |s: i64, e: i64| format!("S{s} E{e}");
        let body = if self.film {
            "Out today".to_string()
        } else {
            match self.episodes.as_slice() {
                [(s, e, name)] => match real_name(name, *e) {
                    Some(name) => format!("New episode · {} · {name}", code(*s, *e)),
                    None => format!("New episode · {}", code(*s, *e)),
                },
                [first, .., last] if first.0 == last.0 => format!(
                    "{} new episodes · S{} E{}–E{}",
                    self.episodes.len(),
                    first.0,
                    first.1,
                    last.1
                ),
                [first, .., last] => format!(
                    "{} new episodes · {}–{}",
                    self.episodes.len(),
                    code(first.0, first.1),
                    code(last.0, last.1)
                ),
                [] => String::new(),
            }
        };
        json!({
            "title": self.name,
            "body": body,
            "url": format!("/t/{}", self.title_id),
            // One notification per title: a later one replaces it, never stacks.
            "tag": format!("frog-title-{}", self.title_id),
            "image": self.picture.as_ref().map(|p| format!("/img/w780{p}")),
        })
    }
}

/// Tell every account with a subscribed browser what is due for it now.
pub async fn run(state: &AppState, now: DateTime<Utc>) -> AppResult<()> {
    // Only accounts with a subscribed browser can be told anything, and each
    // is told about its own titles from its own library.
    let listeners: Vec<(i64,)> = sqlx::query_as("SELECT DISTINCT user_id FROM push_subscription")
        .fetch_all(&state.pool)
        .await?;

    for (user_id,) in listeners {
        let zone = time_zone(&state.pool, user_id).await;
        for a in due(&state.pool, user_id, now, zone).await? {
            let result = crate::push::broadcast(state, user_id, &a.payload()).await?;
            // Nobody reached, because the network is down or the only device
            // is still settling: try again next time.
            if result.sent == 0 && (result.failed > 0 || result.settling > 0) {
                continue;
            }
            let sent_at = Utc::now().to_rfc3339();
            if a.film {
                sqlx::query(
                    "INSERT OR IGNORE INTO push_sent_title (user_id, title_id, sent_at)
                     VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(a.title_id)
                .bind(&sent_at)
                .execute(&state.pool)
                .await?;
            }
            for id in &a.episode_ids {
                sqlx::query(
                    "INSERT OR IGNORE INTO push_sent (user_id, episode_id, sent_at)
                     VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(id)
                .bind(&sent_at)
                .execute(&state.pool)
                .await?;
            }
            tracing::info!(
                user_id,
                title = %a.name,
                episodes = a.episode_ids.len(),
                "announced"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(episodes: Vec<(i64, i64, Option<&str>)>) -> Announcement {
        Announcement {
            episode_ids: vec![1],
            title_id: 7,
            name: "Severance".into(),
            film: false,
            picture: Some("/b.jpg".into()),
            episodes: episodes
                .into_iter()
                .map(|(s, e, n)| (s, e, n.map(String::from)))
                .collect(),
        }
    }

    #[test]
    fn a_notification_names_the_episode_and_carries_the_banner() {
        let p = show(vec![(2, 10, Some("Cold Harbor"))]).payload();
        assert_eq!(p["title"], "Severance");
        assert_eq!(p["body"], "New episode · S2 E10 · Cold Harbor");
        assert_eq!(p["image"], "/img/w780/b.jpg");
        assert_eq!(p["url"], "/t/7");
    }

    #[test]
    fn placeholder_names_and_runs_of_episodes_read_cleanly() {
        let p = show(vec![(1, 4, Some("Episode 4"))]).payload();
        assert_eq!(p["body"], "New episode · S1 E4");
        let p = show(vec![(1, 1, None), (1, 2, None), (1, 3, None)]).payload();
        assert_eq!(p["body"], "3 new episodes · S1 E1–E3");
        let p = show(vec![(1, 8, None), (2, 1, None)]).payload();
        assert_eq!(p["body"], "2 new episodes · S1 E8–S2 E1");
    }

    #[test]
    fn a_film_says_it_is_out() {
        let mut a = show(Vec::new());
        a.film = true;
        a.name = "Dune: Part Three".into();
        assert_eq!(a.payload()["body"], "Out today");
    }
}
