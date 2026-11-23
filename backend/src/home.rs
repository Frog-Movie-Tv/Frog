//! Home, for one account: what it is watching, waiting for and airing this
//! week, and what to try because of what it rated well. `routes::home` is
//! HTTP only.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::catalog::{ExternalId, TitleRow, TITLE_COLUMNS};
use crate::error::AppResult;
use crate::release::FOLLOWED;
use crate::state::AppState;
use crate::store::{self, LibraryItem, NextEpisode};

#[derive(Debug, Serialize)]
pub struct UpNext {
    pub item: LibraryItem,
    pub next: Option<NextEpisode>,
}

#[derive(Debug, Serialize)]
pub struct AiringEpisode {
    pub season_number: i64,
    pub episode_number: i64,
    pub name: Option<String>,
    pub air_date: Option<String>,
    /// The moment, when AniList or TVmaze knows the time.
    pub air_at: Option<String>,
}

/// One show, with every episode of it due inside the window.
#[derive(Debug, Serialize)]
pub struct Airing {
    pub item: LibraryItem,
    pub episodes: Vec<AiringEpisode>,
    pub air_date: Option<String>,
}

/// (title_id, season, episode, name, air_date, air_at)
type AiringRow = (
    i64,
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Everything the home screen draws, in one request.
pub async fn home(state: &AppState, me: i64) -> AppResult<Value> {
    let pool = &state.pool;
    // Read once: every row below is built from it.
    let facts = store::ItemFacts::load(pool, me).await?;

    let watching = rows(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l
               JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ?1 AND l.status = 'watching'
              ORDER BY COALESCE(
                         (SELECT MAX(w.watched_at) FROM watch_event w
                           WHERE w.user_id = ?1 AND w.title_id = t.id AND w.undated = 0),
                         (SELECT MAX(r.read_at) FROM chapter_read r
                           WHERE r.user_id = ?1 AND r.title_id = t.id)) DESC,
                       l.updated_at DESC
              LIMIT 24"
        ),
        me,
    )
    .await?;

    let mut in_flight = Vec::with_capacity(watching.len());
    for row in watching {
        let id = row.id;
        let item = facts.item(row);
        let next = store::next_episode(pool, me, id).await?;
        in_flight.push(UpNext { item, next });
    }

    // A show whose next episode has not aired is not something to continue,
    // it is something to wait for. Saying so beats a play button pointing at
    // an episode that does not exist yet.
    let (waiting, continuing): (Vec<UpNext>, Vec<UpNext>) = in_flight
        .into_iter()
        .filter(|u| {
            let item = &u.item;
            crate::release::in_progress(
                &item.title.kind,
                item.watched_episodes,
                item.total_episodes,
                u.next.is_some(),
            )
        })
        .partition(|u| matches!(&u.next, Some(n) if !n.aired));

    let recent = items(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ?1
              ORDER BY l.added_at DESC LIMIT 20"
        ),
        me,
        &facts,
    )
    .await?;

    let favourites = items(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ?1 AND l.favourite = 1
              ORDER BY l.updated_at DESC LIMIT 20"
        ),
        me,
        &facts,
    )
    .await?;

    // The hero is whatever is most live, and whatever it is, it never appears
    // again in the row directly underneath itself.
    let hero_id = continuing
        .first()
        .or_else(|| waiting.first())
        .map(|u| u.item.title.id)
        .or_else(|| recent.first().map(|i| i.title.id));

    let hero = continuing
        .iter()
        .chain(waiting.iter())
        .find(|u| Some(u.item.title.id) == hero_id)
        .map(|u| json!({ "item": u.item, "next": u.next }))
        .or_else(|| {
            recent
                .iter()
                .find(|i| Some(i.title.id) == hero_id)
                .map(|i| json!({ "item": i, "next": Value::Null }))
        });

    let this_week = airing_this_week(pool, me, &facts).await?;
    let shelves = crate::shelves::summaries(pool, me, false).await?;
    let recommended = recommended(pool, me, &facts).await?;
    // Which row the hero was taken out of: when the hero shows other things,
    // the page puts it back there.
    let hero_row = hero_id.map(|id| {
        if continuing.iter().any(|u| u.item.title.id == id) {
            "continue"
        } else if waiting.iter().any(|u| u.item.title.id == id) {
            "waiting"
        } else {
            "recent"
        }
    });

    Ok(json!({
        "hero": hero,
        "hero_row": hero_row,
        "events": crate::events::view::for_home(pool, me, &facts).await?,
        "recommended": recommended,
        "continue": continuing
            .iter()
            .filter(|u| Some(u.item.title.id) != hero_id)
            .collect::<Vec<_>>(),
        "waiting": waiting
            .iter()
            .filter(|u| Some(u.item.title.id) != hero_id)
            .collect::<Vec<_>>(),
        "this_week": this_week,
        "recent": recent
            .iter()
            .filter(|i| Some(i.title.id) != hero_id)
            .collect::<Vec<_>>(),
        "favourites": favourites,
        "shelves": shelves,
    }))
}

/// How many recommendations Home's hero can show.
const HERO_RECOMMENDATIONS: i64 = 6;

/// Recommendations to this account not answered yet, each with its title as
/// the rest of Home draws one.
async fn recommended(
    pool: &SqlitePool,
    me: i64,
    facts: &store::ItemFacts,
) -> AppResult<Vec<Value>> {
    let mut out = Vec::new();
    for w in crate::recommend::waiting(pool, me, HERO_RECOMMENDATIONS).await? {
        let row: Option<TitleRow> = sqlx::query_as(&format!(
            "SELECT {TITLE_COLUMNS} FROM title t WHERE t.id = ?"
        ))
        .bind(w.title_id)
        .fetch_optional(pool)
        .await?;
        let Some(row) = row else { continue };
        out.push(json!({
            "id": w.id,
            "note": w.note,
            "created_at": w.created_at,
            "from": w.from,
            "item": facts.item(row),
        }));
    }
    Ok(out)
}

/// Episodes due in the next week, one entry per show.
///
/// Two episodes of one show in the same week are a double bill, not two
/// separate things to keep track of, so they fold into a single row.
async fn airing_this_week(
    pool: &SqlitePool,
    user_id: i64,
    facts: &store::ItemFacts,
) -> AppResult<Vec<Airing>> {
    // A day either side of the window: a timed episode's moment can fall on
    // the day before the day it is filed under (an anime's day in Japan).
    let found: Vec<AiringRow> = sqlx::query_as(&format!(
        "SELECT e.title_id, e.season_number, e.episode_number, e.name, e.air_date, e.air_at
           FROM episode e
           JOIN library_entry l ON l.title_id = e.title_id AND l.user_id = ?
          WHERE {FOLLOWED}
            AND l.status != 'completed'
            AND e.air_date IS NOT NULL
            AND e.air_date >= date('now', '-1 day')
            AND e.air_date <= date('now', '+8 day')
          ORDER BY COALESCE(e.air_at, e.air_date), e.season_number, e.episode_number
          LIMIT 80"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let keys: Vec<(i64, &str, i64, Option<&str>)> = found
        .iter()
        .map(|r| (r.0, r.4.as_deref().unwrap_or(""), r.1, r.5.as_deref()))
        .collect();
    let keep = still_due(&keys, chrono::Utc::now());
    let found: Vec<AiringRow> = found
        .into_iter()
        .zip(keep)
        .filter_map(|(row, keep)| keep.then_some(row))
        .collect();

    let mut grouped: Vec<(i64, Vec<AiringEpisode>)> = Vec::new();
    for (title_id, season_number, episode_number, name, air_date, air_at) in found {
        let episode = AiringEpisode {
            season_number,
            episode_number,
            name,
            air_date,
            air_at,
        };
        match grouped.iter_mut().find(|(id, _)| *id == title_id) {
            Some((_, list)) => list.push(episode),
            None => grouped.push((title_id, vec![episode])),
        }
    }

    let mut out = Vec::with_capacity(grouped.len());
    for (title_id, episodes) in grouped {
        let item = facts.item(crate::catalog::get_title(pool, title_id).await?);
        let air_date = episodes.first().and_then(|e| e.air_date.clone());
        out.push(Airing {
            item,
            episodes,
            air_date,
        });
    }

    Ok(out)
}

/// Which of these episodes, each (title, day, season, moment), are still to
/// come in the next week. One with a moment is due until that moment, so an
/// episode that aired an hour ago is not "tomorrow" because its day is
/// Japan's; one with a day alone, until that day is over. A special on the
/// same day as a regular episode of its show is that episode's companion
/// (`release::calendar_keeps`) and goes with it, aired or not.
pub fn still_due(rows: &[(i64, &str, i64, Option<&str>)], now: DateTime<Utc>) -> Vec<bool> {
    let days: Vec<(i64, &str, i64)> = rows.iter().map(|&(t, d, s, _)| (t, d, s)).collect();
    let today = now.date_naive();
    let last = today + Duration::days(7);
    crate::release::calendar_keeps(&days)
        .into_iter()
        .zip(rows)
        .map(|(kept, &(_, day, _, at))| {
            let moment = at
                .and_then(|a| DateTime::parse_from_rfc3339(a).ok())
                .map(|a| a.with_timezone(&Utc));
            kept && match moment {
                Some(at) => at > now && at.date_naive() <= last,
                None => NaiveDate::parse_from_str(day, "%Y-%m-%d")
                    .is_ok_and(|d| d >= today && d <= last),
            }
        })
        .collect()
}

/// How many "Because you watched" rows Home shows, and how many titles each.
const BECAUSE_ROWS: usize = 3;
const BECAUSE_ITEMS: usize = 14;
/// A row with fewer picks than this left after filtering is not worth a rail.
const BECAUSE_MIN: usize = 4;

/// (title id, TMDB id as `title_external` holds it, name)
type SeedRow = (i64, String, String);

/// The titles recommendations grow from: what you rated well (7 and up, or a
/// heart), best first, with recent watching breaking ties. Only titles TMDB
/// knows, since TMDB is what recommends.
pub async fn seeds(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<SeedRow>> {
    Ok(sqlx::query_as(
        // The rating-or-favourite test has to stay bracketed, or the account
        // check would only apply to one side of the OR.
        "SELECT t.id, x.external_id, t.name
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
           JOIN title_external x ON x.title_id = t.id AND x.provider = 'tmdb' AND x.is_primary = 1
          WHERE l.user_id = ?1 AND (l.rating >= 70 OR l.favourite = 1)
          ORDER BY COALESCE(l.rating, 75) DESC,
                   (SELECT MAX(w.watched_at) FROM watch_event w
                     WHERE w.user_id = ?1 AND w.title_id = t.id AND w.undated = 0) DESC,
                   l.updated_at DESC
          LIMIT 12",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

/// Which seeds get a row now. Every highly rated title takes its turn at the
/// front, a new one every few hours, and the next ones follow it: Home shows
/// the first row, which used to be the best rated title, every day, for good.
pub fn pick_seeds<T: Clone>(seeds: &[T], slot: i64) -> Vec<T> {
    if seeds.is_empty() {
        return Vec::new();
    }
    let start = slot.rem_euclid(seeds.len() as i64) as usize;
    seeds
        .iter()
        .cycle()
        .skip(start)
        .take(BECAUSE_ROWS.min(seeds.len()))
        .cloned()
        .collect()
}

/// How long one seed leads before the next takes over: two minutes, so a
/// visit later in the day, or a Home left open, sees other picks.
const SEED_SECONDS: i64 = 120;

/// Pages of TMDB's recommendations, then of its similar titles, that "View
/// more" goes through: past these the picks drift too far from the title.
const MORE_PAGES: u32 = 3;

/// "Because you watched X": TMDB's recommendations for titles you rated
/// highly, minus everything already in your library and anything an earlier
/// row already offered. Separate from `/home` because it may need TMDB, and
/// Home itself must stay instant.
pub async fn because(state: &AppState, me: i64) -> AppResult<Value> {
    if !state.tmdb.is_configured() {
        return Ok(json!({ "rows": [] }));
    }
    let pool = &state.pool;
    let tracked = tracked_tmdb(pool, me).await?;

    let slot = chrono::Utc::now().timestamp() / SEED_SECONDS;
    let mut offered = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for (title_id, key, name) in pick_seeds(&seeds(pool, me).await?, slot) {
        let Some(ExternalId::Tmdb { kind, id: tmdb_id }) = ExternalId::parse("tmdb", &key) else {
            continue;
        };
        let detail = match kind.as_str() {
            "movie" => state.tmdb.movie(tmdb_id).await,
            _ => state.tmdb.tv(tmdb_id).await,
        };
        let detail = match detail {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(tmdb_id, error = %e, "no recommendations for this seed");
                continue;
            }
        };
        let mut picks = crate::tmdb::models::related(&detail, "recommendations", 40);
        if picks.len() < BECAUSE_MIN {
            picks.extend(crate::tmdb::models::related(&detail, "similar", 40));
        }
        let picks: Vec<_> = picks
            .into_iter()
            .filter(|p| p.poster_path.is_some())
            .filter(|p| p.external().is_some_and(|x| !tracked.contains(&x.key())))
            .filter(|p| offered.insert((p.kind.clone(), p.tmdb_id)))
            .take(BECAUSE_ITEMS)
            .collect();
        if picks.len() >= BECAUSE_MIN {
            rows.push(json!({
                "seed": { "id": title_id, "name": name },
                "items": picks,
            }));
        }
    }
    Ok(json!({ "rows": rows }))
}

/// TMDB ids already in this account's library, as `title_external` writes them.
async fn tracked_tmdb(pool: &SqlitePool, me: i64) -> AppResult<std::collections::HashSet<String>> {
    Ok(sqlx::query_as::<_, (String,)>(
        "SELECT x.external_id FROM library_entry l
           JOIN title_external x ON x.title_id = l.title_id AND x.provider = 'tmdb'
          WHERE l.user_id = ?",
    )
    .bind(me)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.0)
    .collect())
}

/// How many trending titles Home's hero features.
const FEATURED: usize = 4;

/// What Home's hero features beside recommendations: this week's trending
/// films and shows on TMDB that this account does not have yet, each with a
/// picture to fill the screen. Separate from `home` because a pick Frog has
/// not seen before is fetched from TMDB to become a Frog title, and Home
/// itself must stay instant. A TMDB that does not answer features nothing.
pub async fn featured(state: &AppState, me: i64) -> AppResult<Value> {
    let none = || json!({ "items": [] });
    if !state.tmdb.is_configured() {
        return Ok(none());
    }
    let raw = match state.tmdb.trending(1).await {
        Ok(raw) => raw,
        Err(e) => {
            tracing::debug!(error = %e, "nothing featured: no trending list");
            return Ok(none());
        }
    };
    let pool = &state.pool;
    let tracked = tracked_tmdb(pool, me).await?;
    let picks: Vec<crate::tmdb::models::SearchItem> = raw["results"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| crate::tmdb::models::SearchItem::from_value(v, None))
                .collect()
        })
        .unwrap_or_default();

    // Its place on TMDB's list, counted before anything is left out, so "#3"
    // means what it says.
    let ranked = crate::tmdb::models::unique_items(picks)
        .into_iter()
        .filter(|p| matches!(p.kind.as_str(), "movie" | "tv"))
        .enumerate()
        .map(|(i, p)| (i + 1, p));

    let facts = store::ItemFacts::load(pool, me).await?;
    let mut items: Vec<Value> = Vec::new();
    let mut chosen: Vec<i64> = Vec::new();
    for (rank, pick) in ranked {
        if chosen.len() >= FEATURED {
            break;
        }
        if pick.backdrop_path.is_none() {
            continue;
        }
        let Some(source) = pick.external() else {
            continue;
        };
        if tracked.contains(&source.key()) {
            continue;
        }
        // A title Frog already holds is read as it is; only a new one waits
        // for TMDB, once.
        let title_id = match crate::catalog::find(pool, &source).await? {
            Some(id) => id,
            None => match crate::catalog::ensure(state, &source).await {
                Ok((id, _)) => id,
                Err(e) => {
                    tracing::debug!(key = %source.key(), error = %e, "not featured");
                    continue;
                }
            },
        };
        let item = facts.item(crate::catalog::get_title(pool, title_id).await?);
        // In the library under another provider's id: not something to offer.
        if item.entry.is_some() || chosen.contains(&item.title.id) {
            continue;
        }
        chosen.push(item.title.id);
        items.push(json!({ "item": item, "rank": rank }));
    }
    Ok(json!({ "items": items }))
}

/// Every row query here is scoped to one account, and each uses `?1` for it so
/// a single bind covers however many times the account appears in the SQL.
async fn rows(pool: &SqlitePool, sql: &str, user_id: i64) -> AppResult<Vec<TitleRow>> {
    Ok(sqlx::query_as::<_, TitleRow>(sql)
        .bind(user_id)
        .fetch_all(pool)
        .await?)
}

async fn items(
    pool: &SqlitePool,
    sql: &str,
    user_id: i64,
    facts: &store::ItemFacts,
) -> AppResult<Vec<LibraryItem>> {
    let found = rows(pool, sql, user_id).await?;
    Ok(found.into_iter().map(|row| facts.item(row)).collect())
}

/// "View more" from a title: TMDB's recommendations for it a page at a time,
/// then its similar titles once those run out. Returns the title's name, a
/// page of picks, and the next page when there is one.
pub async fn more_like(
    state: &AppState,
    title_id: i64,
    page: u32,
) -> AppResult<(String, Vec<crate::tmdb::models::SearchItem>, Option<u32>)> {
    let title = crate::catalog::get_title(&state.pool, title_id).await?;
    let Some(ExternalId::Tmdb { kind, id }) =
        crate::catalog::primary(&state.pool, title_id).await?
    else {
        // Only TMDB recommends.
        return Ok((title.name, Vec::new(), None));
    };
    let page = page.clamp(1, MORE_PAGES * 2);
    let (which, at) = if page <= MORE_PAGES {
        ("recommendations", page)
    } else {
        ("similar", page - MORE_PAGES)
    };
    let answer = state.tmdb.related_page(&kind, id, which, at).await?;
    let pages = answer["total_pages"].as_u64().unwrap_or(0) as u32;
    let mut wrapped = json!({});
    wrapped[which] = answer;
    let items = crate::tmdb::models::related(&wrapped, which, 100);
    let next = if which == "recommendations" {
        // Similar titles follow, whether or not recommendations filled all
        // their pages.
        Some(if at < pages.min(MORE_PAGES) {
            page + 1
        } else {
            MORE_PAGES + 1
        })
    } else {
        (at < pages.min(MORE_PAGES)).then_some(page + 1)
    };
    Ok((title.name, items, next))
}

#[cfg(test)]
mod tests {
    use super::{pick_seeds, still_due};

    #[test]
    fn an_episode_that_has_aired_is_not_still_to_come() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-15T18:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        // An anime filed under Monday in Japan that aired on Sunday afternoon,
        // and the special TMDB lists beside it, with a day alone.
        let rows = [
            (1, "2026-11-16", 0, None),
            (1, "2026-11-16", 1, Some("2026-11-15T16:20:00Z")),
            (2, "2026-11-16", 1, Some("2026-11-15T20:00:00Z")),
            (3, "2026-11-15", 1, None),
            (4, "2026-11-14", 1, None),
            (5, "2026-11-23", 1, Some("2026-11-22T16:20:00Z")),
            (6, "2026-11-23", 1, Some("2026-11-23T16:20:00Z")),
        ];
        assert_eq!(
            still_due(&rows, now),
            [false, false, true, true, false, true, false]
        );
    }

    #[test]
    fn every_seed_takes_its_turn_at_the_front() {
        let seeds = ["best", "b", "c", "d", "e"];
        assert_eq!(pick_seeds(&seeds, 0), ["best", "b", "c"]);
        assert_eq!(pick_seeds(&seeds, 1), ["b", "c", "d"]);
        assert_eq!(pick_seeds(&seeds, 4), ["e", "best", "b"], "wraps around");
        assert_eq!(pick_seeds(&seeds[..2], 7), ["b", "best"]);
        assert_eq!(pick_seeds(&["only"], 9), ["only"]);
        assert!(pick_seeds::<&str>(&[], 0).is_empty());
    }
}
