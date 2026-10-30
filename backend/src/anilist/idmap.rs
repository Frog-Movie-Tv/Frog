//! Which TMDB, TVDB and IMDb entry an AniList id is, from two community id
//! lists kept in `anime_map`, and what Frog remembers of each AniList id.

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::config::AniListClient;
use crate::error::AppResult;
use crate::state::AppState;

/// Both id lists change slowly; a week is plenty.
const MAP_DAYS: i64 = 7;

/// One entry of Fribb's anime-lists; everything else in it is left unread.
#[derive(Deserialize)]
struct ListEntry {
    anilist_id: Option<i64>,
    /// `{ "tv": 1429 }` or `{ "movie": 129 }`.
    themoviedb_id: Option<Value>,
    /// `{ "tvdb": 2, "tmdb": 1 }`.
    season: Option<Value>,
    /// `{ "tvdb": 0, "tmdb": 28 }`.
    episode_offset: Option<Value>,
    #[serde(rename = "type")]
    format: Option<String>,
}

async fn download<T: serde::de::DeserializeOwned>(state: &AppState, url: &str) -> Option<T> {
    match state.http.get(url).send().await {
        Ok(r) if r.status().is_success() => match r.json::<T>().await {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(%url, error = %e, "an anime id list was not the expected JSON");
                None
            }
        },
        Ok(r) => {
            tracing::warn!(%url, status = %r.status(), "could not download an anime id list");
            None
        }
        Err(e) => {
            tracing::warn!(%url, error = %e, "could not download an anime id list");
            None
        }
    }
}

/// Make sure both id lists are loaded and not older than a week: Anime-IDs
/// (TVDB and IMDb ids) and Fribb's anime-lists (TMDB ids and TMDB seasons).
/// Each list replaces only its own columns, so one failed download keeps
/// whatever that list said last time.
pub async fn ensure_map(state: &AppState, client: &AniListClient) -> AppResult<()> {
    let row: Option<(String,)> = sqlx::query_as("SELECT fetched_at FROM anime_map_state")
        .fetch_optional(&state.pool)
        .await?;
    let fresh = row
        .and_then(|(at,)| chrono::DateTime::parse_from_rfc3339(&at).ok())
        .is_some_and(|at| Utc::now() - at.with_timezone(&Utc) < chrono::Duration::days(MAP_DAYS));
    if fresh {
        return Ok(());
    }

    let ids: Option<serde_json::Map<String, Value>> = download(state, &client.anime_ids_url).await;
    let lists: Option<Vec<ListEntry>> = download(state, &client.anime_lists_url).await;
    if ids.is_none() && lists.is_none() {
        return Ok(());
    }

    let mut tx = state.pool.begin().await?;
    let mut from_ids = 0;
    if let Some(entries) = &ids {
        sqlx::query(
            "UPDATE anime_map
                SET tvdb_id = NULL, tvdb_season = NULL, tvdb_epoffset = 0, imdb_id = NULL",
        )
        .execute(&mut *tx)
        .await?;
        for v in entries.values() {
            let Some(anilist_id) = v["anilist_id"].as_i64() else {
                continue;
            };
            let imdb = match &v["imdb_id"] {
                Value::String(s) => s.split(',').next().map(|s| s.trim().to_string()),
                _ => None,
            }
            .filter(|s| s.starts_with("tt"));
            sqlx::query(
                "INSERT INTO anime_map (anilist_id, tvdb_id, tvdb_season, tvdb_epoffset, imdb_id)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (anilist_id) DO UPDATE SET
                    tvdb_id = excluded.tvdb_id, tvdb_season = excluded.tvdb_season,
                    tvdb_epoffset = excluded.tvdb_epoffset, imdb_id = excluded.imdb_id",
            )
            .bind(anilist_id)
            .bind(v["tvdb_id"].as_i64())
            .bind(v["tvdb_season"].as_i64())
            .bind(v["tvdb_epoffset"].as_i64().unwrap_or(0))
            .bind(imdb)
            .execute(&mut *tx)
            .await?;
            from_ids += 1;
        }
    }
    let mut from_lists = 0;
    if let Some(entries) = &lists {
        sqlx::query(
            "UPDATE anime_map
                SET tmdb_kind = NULL, tmdb_id = NULL, tmdb_season = NULL, tmdb_offset = NULL",
        )
        .execute(&mut *tx)
        .await?;
        for e in entries {
            let (Some(anilist_id), Some(tmdb)) = (e.anilist_id, e.themoviedb_id.as_ref()) else {
                continue;
            };
            let film = e.format.as_deref() == Some("MOVIE");
            let (kind, tmdb_id) = match (tmdb["movie"].as_i64(), tmdb["tv"].as_i64()) {
                (Some(m), _) if film => ("movie", m),
                (_, Some(t)) => ("tv", t),
                (Some(m), None) => ("movie", m),
                (None, None) => continue,
            };
            let season = e.season.as_ref().and_then(|s| s["tmdb"].as_i64());
            let offset = e.episode_offset.as_ref().and_then(|o| o["tmdb"].as_i64());
            sqlx::query(
                "INSERT INTO anime_map (anilist_id, tmdb_kind, tmdb_id, tmdb_season, tmdb_offset)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (anilist_id) DO UPDATE SET
                    tmdb_kind = excluded.tmdb_kind, tmdb_id = excluded.tmdb_id,
                    tmdb_season = excluded.tmdb_season, tmdb_offset = excluded.tmdb_offset",
            )
            .bind(anilist_id)
            .bind(kind)
            .bind(tmdb_id)
            .bind(season)
            .bind(offset)
            .execute(&mut *tx)
            .await?;
            from_lists += 1;
        }
    }
    sqlx::query(
        "INSERT INTO anime_map_state (id, fetched_at) VALUES (1, ?)
         ON CONFLICT (id) DO UPDATE SET fetched_at = excluded.fetched_at",
    )
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(from_ids, from_lists, "loaded the anime id lists");
    Ok(())
}

/// What AniList said about a media id last time it was read, so placing
/// can count episodes without asking again.
pub(super) async fn remember_media(
    pool: &SqlitePool,
    id: i64,
    episodes: Option<i64>,
    format: Option<&str>,
    name: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO anilist_media (id, episodes, format, name, fetched_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET episodes = excluded.episodes, format = excluded.format,
                                        name = excluded.name, fetched_at = excluded.fetched_at",
    )
    .bind(id)
    .bind(episodes)
    .bind(format)
    .bind(name)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}
