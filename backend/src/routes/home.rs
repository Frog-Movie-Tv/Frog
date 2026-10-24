use axum::extract::State;
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::auth::Auth;
use crate::error::AppResult;
use crate::state::AppState;
use crate::store::{self, LibraryItem, TitleRow};

/// The next episode of a show, and whether it is out yet.
#[derive(Debug, Serialize, Clone)]
pub struct NextEpisode {
    pub season_number: i64,
    pub episode_number: i64,
    pub name: Option<String>,
    pub still_path: Option<String>,
    pub air_date: Option<String>,
    pub aired: bool,
}

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

/// (season, episode, name, still_path, air_date, aired)
type NextRow = (
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
);

const TITLE_COLUMNS: &str = "t.id, t.kind, t.tmdb_id, t.name, t.overview, t.poster_path,
        t.backdrop_path, t.logo_path, t.release_date, t.last_air_date, t.tmdb_status, t.runtime,
        t.episode_count, t.season_count, t.genres_json, t.tmdb_vote, t.imdb_id,
        t.collection_id, t.accent_json";

/// Everything the home screen draws, in one request.
pub async fn home(_: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let pool = &state.pool;

    let watching = rows(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l
               JOIN title t ON t.id = l.title_id
              WHERE l.status = 'watching'
              ORDER BY (SELECT MAX(w.watched_at) FROM watch_event w WHERE w.title_id = t.id) DESC,
                       l.updated_at DESC
              LIMIT 24"
        ),
    )
    .await?;

    let mut in_flight = Vec::with_capacity(watching.len());
    for row in watching {
        let id = row.id;
        let item = store::build_item(pool, row).await?;
        let next = next_episode(pool, id).await?;
        in_flight.push(UpNext { item, next });
    }

    // A show whose next episode has not aired is not something to continue,
    // it is something to wait for. Saying so beats a play button pointing at
    // an episode that does not exist yet.
    let (waiting, continuing): (Vec<UpNext>, Vec<UpNext>) = in_flight
        .into_iter()
        .partition(|u| matches!(&u.next, Some(n) if !n.aired));

    let recent = items(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l JOIN title t ON t.id = l.title_id
              ORDER BY l.added_at DESC LIMIT 20"
        ),
    )
    .await?;

    let favourites = items(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l JOIN title t ON t.id = l.title_id
              WHERE l.favourite = 1
              ORDER BY l.updated_at DESC LIMIT 20"
        ),
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

    let this_week = airing_this_week(pool).await?;

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
async fn airing_this_week(pool: &SqlitePool) -> AppResult<Vec<Airing>> {
    let found: Vec<AiringRow> = sqlx::query_as(
        "SELECT e.title_id, e.season_number, e.episode_number, e.name, e.air_date
           FROM episode e
           JOIN library_entry l ON l.title_id = e.title_id
          WHERE l.status IN ('watching', 'planned', 'paused')
            AND e.air_date IS NOT NULL
            AND e.air_date >= date('now')
            AND e.air_date <= date('now', '+7 day')
          ORDER BY e.air_date, e.season_number, e.episode_number
          LIMIT 60",
    )
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
        let item = store::build_item(pool, row).await?;
        let air_date = episodes.first().and_then(|e| e.air_date.clone());
        out.push(Airing {
            item,
            episodes,
            air_date,
        });
    }

    Ok(out)
}

async fn rows(pool: &SqlitePool, sql: &str) -> AppResult<Vec<TitleRow>> {
    Ok(sqlx::query_as::<_, TitleRow>(sql).fetch_all(pool).await?)
}

async fn items(pool: &SqlitePool, sql: &str) -> AppResult<Vec<LibraryItem>> {
    let found = rows(pool, sql).await?;
    let mut out = Vec::with_capacity(found.len());
    for row in found {
        out.push(store::build_item(pool, row).await?);
    }
    Ok(out)
}

async fn next_episode(pool: &SqlitePool, title_id: i64) -> AppResult<Option<NextEpisode>> {
    let row: Option<NextRow> = sqlx::query_as(
        "SELECT e.season_number,
                e.episode_number,
                e.name,
                e.still_path,
                e.air_date,
                CASE WHEN e.air_date IS NULL OR e.air_date <= date('now') THEN 1 ELSE 0 END
           FROM episode e
          WHERE e.title_id = ?
            AND e.season_number > 0
            AND NOT EXISTS (SELECT 1 FROM watch_event w WHERE w.episode_id = e.id)
          ORDER BY e.season_number, e.episode_number
          LIMIT 1",
    )
    .bind(title_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(season_number, episode_number, name, still_path, air_date, aired)| NextEpisode {
            season_number,
            episode_number,
            name,
            still_path,
            air_date,
            aired: aired == 1,
        },
    ))
}
