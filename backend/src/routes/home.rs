use axum::extract::State;
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::auth::Auth;
use crate::error::AppResult;
use crate::state::AppState;
use crate::store::{self, LibraryItem, NextEpisode, TitleRow};

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
}

/// One show, with every episode of it due inside the window.
#[derive(Debug, Serialize)]
pub struct Airing {
    pub item: LibraryItem,
    pub episodes: Vec<AiringEpisode>,
    pub air_date: Option<String>,
}

/// (title_id, season, episode, name, air_date)
type AiringRow = (i64, i64, i64, Option<String>, Option<String>);

const TITLE_COLUMNS: &str = "t.id, t.kind, t.tmdb_id, t.name, t.overview, t.poster_path,
        t.backdrop_path, t.logo_path, t.release_date, t.last_air_date, t.tmdb_status, t.runtime,
        t.episode_count, t.season_count, t.genres_json, t.tmdb_vote, t.imdb_id,
        t.collection_id, t.accent_json";

/// Everything the home screen draws, in one request.
pub async fn home(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let pool = &state.pool;
    let me = auth.user_id();

    let watching = rows(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l
               JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ?1 AND l.status = 'watching'
              ORDER BY (SELECT MAX(w.watched_at) FROM watch_event w
                         WHERE w.user_id = ?1 AND w.title_id = t.id) DESC,
                       l.updated_at DESC
              LIMIT 24"
        ),
        me,
    )
    .await?;

    let mut in_flight = Vec::with_capacity(watching.len());
    for row in watching {
        let id = row.id;
        let item = store::build_item(pool, me, row).await?;
        let next = store::next_episode(pool, me, id).await?;
        in_flight.push(UpNext { item, next });
    }

    // A show whose next episode has not aired is not something to continue,
    // it is something to wait for. Saying so beats a play button pointing at
    // an episode that does not exist yet.
    // Nothing out yet is neither to continue nor caught up on: it is coming,
    // and the calendar has it.
    let (waiting, continuing): (Vec<UpNext>, Vec<UpNext>) = in_flight
        .into_iter()
        .filter(|u| u.item.total_episodes > 0)
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

    let this_week = airing_this_week(pool, me).await?;

    Ok(Json(json!({
        "hero": hero,
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
    })))
}

/// Episodes due in the next week, one entry per show.
///
/// Two episodes of one show in the same week are a double bill, not two
/// separate things to keep track of, so they fold into a single row.
async fn airing_this_week(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Airing>> {
    let found: Vec<AiringRow> = sqlx::query_as(
        "SELECT e.title_id, e.season_number, e.episode_number, e.name, e.air_date
           FROM episode e
           JOIN library_entry l ON l.title_id = e.title_id AND l.user_id = ?
          WHERE l.status IN ('watching', 'planned', 'paused')
            AND e.air_date IS NOT NULL
            AND e.air_date >= date('now')
            AND e.air_date <= date('now', '+7 day')
          ORDER BY e.air_date, e.season_number, e.episode_number
          LIMIT 60",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut grouped: Vec<(i64, Vec<AiringEpisode>)> = Vec::new();
    for (title_id, season_number, episode_number, name, air_date) in found {
        let episode = AiringEpisode {
            season_number,
            episode_number,
            name,
            air_date,
        };
        match grouped.iter_mut().find(|(id, _)| *id == title_id) {
            Some((_, list)) => list.push(episode),
            None => grouped.push((title_id, vec![episode])),
        }
    }

    let mut out = Vec::with_capacity(grouped.len());
    for (title_id, episodes) in grouped {
        let row = store::get_title(pool, title_id).await?;
        let item = store::build_item(pool, user_id, row).await?;
        let air_date = episodes.first().and_then(|e| e.air_date.clone());
        out.push(Airing {
            item,
            episodes,
            air_date,
        });
    }

    Ok(out)
}

/// How many "Because you watched" rows Home shows, and how many titles each.
const BECAUSE_ROWS: usize = 3;
const BECAUSE_ITEMS: usize = 14;
/// A row with fewer picks than this left after filtering is not worth a rail.
const BECAUSE_MIN: usize = 4;

/// (kind, tmdb_id, name)
type SeedRow = (String, i64, String);

/// The titles recommendations grow from: what you rated well (7 and up, or a
/// heart), best first, with recent watching breaking ties.
pub async fn seeds(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<SeedRow>> {
    Ok(sqlx::query_as(
        // The rating-or-favourite test has to stay bracketed, or the account
        // check would only apply to one side of the OR.
        "SELECT t.kind, t.tmdb_id, t.name
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?1 AND (l.rating >= 70 OR l.favourite = 1)
          ORDER BY COALESCE(l.rating, 75) DESC,
                   (SELECT MAX(w.watched_at) FROM watch_event w
                     WHERE w.user_id = ?1 AND w.title_id = t.id) DESC,
                   l.updated_at DESC
          LIMIT 12",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

/// Which seeds get a row today. The best one always does; the rest rotate
/// daily through the other highly rated titles, so Home does not show the
/// same three rows for months.
pub fn pick_seeds<T: Clone>(seeds: &[T], day: i64) -> Vec<T> {
    let Some((first, rest)) = seeds.split_first() else {
        return Vec::new();
    };
    let mut out = vec![first.clone()];
    if !rest.is_empty() {
        let start = day.rem_euclid(rest.len() as i64) as usize;
        out.extend(
            rest.iter()
                .cycle()
                .skip(start)
                .take((BECAUSE_ROWS - 1).min(rest.len()))
                .cloned(),
        );
    }
    out
}

/// "Because you watched X": TMDB's recommendations for titles you rated
/// highly, minus everything already in your library and anything an earlier
/// row already offered. Separate from `/home` because it may need TMDB, and
/// Home itself must stay instant.
pub async fn because(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    if !state.tmdb.is_configured() {
        return Ok(Json(json!({ "rows": [] })));
    }
    let pool = &state.pool;
    let me = auth.user_id();
    let tracked: std::collections::HashSet<(String, i64)> = sqlx::query_as::<_, (String, i64)>(
        "SELECT t.kind, t.tmdb_id FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?",
    )
    .bind(me)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let day = chrono::Utc::now().timestamp() / 86_400;
    let mut offered = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for (kind, tmdb_id, name) in pick_seeds(&seeds(pool, me).await?, day) {
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
            .filter(|p| !tracked.contains(&(p.kind.clone(), p.tmdb_id)))
            .filter(|p| offered.insert((p.kind.clone(), p.tmdb_id)))
            .take(BECAUSE_ITEMS)
            .collect();
        if picks.len() >= BECAUSE_MIN {
            rows.push(json!({
                "seed": { "kind": kind, "tmdb_id": tmdb_id, "name": name },
                "items": picks,
            }));
        }
    }
    Ok(Json(json!({ "rows": rows })))
}

/// Every row query here is scoped to one account, and each uses `?1` for it so
/// a single bind covers however many times the account appears in the SQL.
async fn rows(pool: &SqlitePool, sql: &str, user_id: i64) -> AppResult<Vec<TitleRow>> {
    Ok(sqlx::query_as::<_, TitleRow>(sql)
        .bind(user_id)
        .fetch_all(pool)
        .await?)
}

async fn items(pool: &SqlitePool, sql: &str, user_id: i64) -> AppResult<Vec<LibraryItem>> {
    let found = rows(pool, sql, user_id).await?;
    let mut out = Vec::with_capacity(found.len());
    for row in found {
        out.push(store::build_item(pool, user_id, row).await?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::pick_seeds;

    #[test]
    fn the_best_seed_stays_and_the_rest_rotate_daily() {
        let seeds = ["best", "b", "c", "d", "e"];
        assert_eq!(pick_seeds(&seeds, 0), ["best", "b", "c"]);
        assert_eq!(pick_seeds(&seeds, 1), ["best", "c", "d"]);
        assert_eq!(pick_seeds(&seeds, 3), ["best", "e", "b"], "wraps around");
        assert_eq!(pick_seeds(&seeds[..2], 7), ["best", "b"]);
        assert_eq!(pick_seeds(&["only"], 9), ["only"]);
        assert!(pick_seeds::<&str>(&[], 0).is_empty());
    }
}
