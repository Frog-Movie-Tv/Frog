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
#[derive(Debug, Serialize)]
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
///
/// Four small queries beat four round trips from a phone on a tailnet, and
/// the rows are tiny.
pub async fn home(_: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let pool = &state.pool;

    // Things in flight, most recently watched first. A show you have never
    // touched is not "continue watching", so require at least one watch.
    let watching = rows(
        pool,
        &format!(
            "SELECT {TITLE_COLUMNS}
               FROM library_entry l
               JOIN title t ON t.id = l.title_id
              WHERE l.status = 'watching'
              ORDER BY (SELECT MAX(w.watched_at) FROM watch_event w WHERE w.title_id = t.id) DESC,
                       l.updated_at DESC
              LIMIT 20"
        ),
    )
    .await?;

    let mut continuing = Vec::with_capacity(watching.len());
    for row in watching {
        let id = row.id;
        let item = store::build_item(pool, row).await?;
        let next = next_episode(pool, id).await?;
        continuing.push(UpNext { item, next });
    }

    // Anything I follow that airs in the next week.
    let airing: Vec<AiringRow> = sqlx::query_as(
        "SELECT e.title_id, e.season_number, e.episode_number, e.name, e.air_date
           FROM episode e
           JOIN library_entry l ON l.title_id = e.title_id
          WHERE l.status IN ('watching', 'planned', 'paused')
            AND e.air_date IS NOT NULL
            AND e.air_date >= date('now')
            AND e.air_date <= date('now', '+7 day')
          ORDER BY e.air_date, e.season_number, e.episode_number
          LIMIT 30",
    )
    .fetch_all(pool)
    .await?;

    let mut this_week = Vec::with_capacity(airing.len());
    for (title_id, season, episode, name, air_date) in airing {
        let row = store::get_title(pool, title_id).await?;
        let item = store::build_item(pool, row).await?;
        this_week.push(json!({
            "item": item,
            "episode": {
                "season_number": season,
                "episode_number": episode,
                "name": name,
                "air_date": air_date,
            }
        }));
    }

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

    // The hero is whatever I am most actively in the middle of.
    let hero = continuing.first().map(|u| &u.item).or(recent.first());

    Ok(Json(json!({
        "hero": hero,
        "continue": continuing,
        "this_week": this_week,
        "recent": recent,
        "favourites": favourites,
    })))
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
