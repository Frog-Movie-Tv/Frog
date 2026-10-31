use axum::extract::State;
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::auth::Auth;
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

/// Everything the home screen draws, in one request.
pub async fn home(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let pool = &state.pool;
    let me = auth.user_id();
    // Read once: every row below is built from it.
    let facts = store::ItemFacts::load(pool, me).await?;

    let watching = rows(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l
               JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ?1 AND l.status = 'watching'
              ORDER BY (SELECT MAX(w.watched_at) FROM watch_event w
                         WHERE w.user_id = ?1 AND w.title_id = t.id AND w.undated = 0) DESC,
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
            crate::release::continues(item.watched_episodes, item.total_episodes, u.next.is_some())
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
    let shelves = crate::routes::shelves::summaries(pool, me).await?;

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
        "shelves": shelves,
    })))
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
    let found: Vec<AiringRow> = sqlx::query_as(&format!(
        "SELECT e.title_id, e.season_number, e.episode_number, e.name, e.air_date
           FROM episode e
           JOIN library_entry l ON l.title_id = e.title_id AND l.user_id = ?
          WHERE {FOLLOWED}
            AND l.status != 'completed'
            AND e.air_date IS NOT NULL
            AND e.air_date >= date('now')
            AND e.air_date <= date('now', '+7 day')
          ORDER BY e.air_date, e.season_number, e.episode_number
          LIMIT 60"
    ))
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
    // TMDB ids already in the library, as `title_external` writes them.
    let tracked: std::collections::HashSet<String> = sqlx::query_as::<_, (String,)>(
        "SELECT x.external_id FROM library_entry l
           JOIN title_external x ON x.title_id = l.title_id AND x.provider = 'tmdb'
          WHERE l.user_id = ?",
    )
    .bind(me)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.0)
    .collect();

    let day = chrono::Utc::now().timestamp() / 86_400;
    let mut offered = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for (title_id, key, name) in pick_seeds(&seeds(pool, me).await?, day) {
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

async fn items(
    pool: &SqlitePool,
    sql: &str,
    user_id: i64,
    facts: &store::ItemFacts,
) -> AppResult<Vec<LibraryItem>> {
    let found = rows(pool, sql, user_id).await?;
    Ok(found.into_iter().map(|row| facts.item(row)).collect())
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
