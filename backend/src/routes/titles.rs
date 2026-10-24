use axum::extract::{Path, State};
use axum::Json;
use chrono::Duration;
use serde::Serialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store;
use crate::tmdb::models::{self, EpisodeInfo, SearchItem};

/// (season, episode, name, still_path, air_date)
type NextUpRow = (i64, i64, Option<String>, Option<String>, Option<String>);

/// (season, episode, my rating, my notes, number of watches)
type EpisodeStateRow = (i64, i64, Option<f64>, Option<String>, i64);

fn check_kind(kind: &str) -> AppResult<()> {
    if kind == "movie" || kind == "tv" {
        Ok(())
    } else {
        Err(AppError::BadRequest("kind must be movie or tv".into()))
    }
}

#[derive(Serialize)]
pub struct EpisodeView {
    #[serde(flatten)]
    pub info: EpisodeInfo,
    pub watched: bool,
    pub watch_count: i64,
    pub rating: Option<f64>,
    pub notes: Option<String>,
}

#[derive(Serialize)]
pub struct NextUp {
    pub season_number: i64,
    pub episode_number: i64,
    pub name: Option<String>,
    pub still_path: Option<String>,
    pub air_date: Option<String>,
}

/// One request per title page. Everything the page draws above the fold comes
/// back here; seasons are fetched lazily as they are opened.
pub async fn detail(
    _: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id)): Path<(String, i64)>,
) -> AppResult<Json<Value>> {
    check_kind(&kind)?;
    let (title_id, raw) = store::ensure_title(&state, &kind, tmdb_id).await?;
    let row = store::get_title(&state.pool, title_id).await?;
    let item = store::build_item(&state.pool, row).await?;

    let collection = match item.title.collection_id {
        Some(cid) => collection_parts(&state, cid).await.ok(),
        None => None,
    };

    let next_up = if kind == "tv" {
        next_unwatched(&state, title_id).await?
    } else {
        None
    };

    Ok(Json(json!({
        "item": item,
        "tagline": models::str_at(&raw, "tagline"),
        "homepage": models::str_at(&raw, "homepage"),
        "cast": models::cast(&raw, 24),
        "crew": models::key_crew(&raw),
        "trailers": models::trailers(&raw),
        "providers": models::providers(&raw, state.tmdb.region()),
        "provider_region": state.tmdb.region(),
        "seasons": models::seasons(&raw),
        "recommendations": models::related(&raw, "recommendations", 18),
        "similar": models::related(&raw, "similar", 18),
        "collection": collection,
        "next_up": next_up,
        "heatmap": heatmap(&state, title_id).await?,
    })))
}

/// Films in the same franchise, in release order: the watch order that is
/// actually defensible without hand-maintained lists.
async fn collection_parts(state: &AppState, collection_id: i64) -> AppResult<Value> {
    let raw = state
        .tmdb
        .get(
            &format!("/collection/{collection_id}"),
            &[],
            Duration::hours(24 * 7),
        )
        .await?;

    let mut parts: Vec<SearchItem> = raw
        .get("parts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| SearchItem::from_value(p, Some("movie")))
                .collect()
        })
        .unwrap_or_default();

    parts.sort_by_key(|p| p.year.unwrap_or(9999));
    crate::routes::search::annotate_library(state, &mut parts).await?;

    Ok(json!({
        "id": collection_id,
        "name": models::str_at(&raw, "name"),
        "overview": models::str_at(&raw, "overview"),
        "poster_path": models::str_at(&raw, "poster_path"),
        "backdrop_path": models::str_at(&raw, "backdrop_path"),
        "parts": parts,
    }))
}

async fn next_unwatched(state: &AppState, title_id: i64) -> AppResult<Option<NextUp>> {
    let row: Option<NextUpRow> = sqlx::query_as(
        "SELECT e.season_number, e.episode_number, e.name, e.still_path, e.air_date
           FROM episode e
          WHERE e.title_id = ?
            AND e.season_number > 0
            AND NOT EXISTS (SELECT 1 FROM watch_event w WHERE w.episode_id = e.id)
          ORDER BY e.season_number, e.episode_number
          LIMIT 1",
    )
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(row.map(|(s, e, name, still_path, air_date)| NextUp {
        season_number: s,
        episode_number: e,
        name,
        still_path,
        air_date,
    }))
}

/// Compact grid data for the per-show episode heatmap. Empty until the show has
/// been added (that is when every season gets pulled down).
async fn heatmap(state: &AppState, title_id: i64) -> AppResult<Value> {
    let rows: Vec<(i64, i64, Option<f64>, i64)> = sqlx::query_as(
        "SELECT e.season_number,
                e.episode_number,
                s.rating,
                (SELECT COUNT(*) FROM watch_event w WHERE w.episode_id = e.id)
           FROM episode e
           LEFT JOIN episode_state s ON s.episode_id = e.id
          WHERE e.title_id = ? AND e.season_number > 0
          ORDER BY e.season_number, e.episode_number",
    )
    .bind(title_id)
    .fetch_all(&state.pool)
    .await?;

    let cells: Vec<Value> = rows
        .into_iter()
        .map(|(s, e, rating, watches)| {
            json!({ "s": s, "e": e, "rating": rating, "watched": watches > 0 })
        })
        .collect();

    Ok(Value::Array(cells))
}

/// Episodes of one season, merged with my watches, ratings and notes.
pub async fn season(
    _: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id, season)): Path<(String, i64, i64)>,
) -> AppResult<Json<Value>> {
    if kind != "tv" {
        return Err(AppError::BadRequest("only shows have seasons".into()));
    }

    let title_id = match store::find_title_id(&state.pool, &kind, tmdb_id).await? {
        Some(id) => id,
        None => store::ensure_title(&state, &kind, tmdb_id).await?.0,
    };

    let raw = state.tmdb.season(tmdb_id, season).await?;
    let episodes = models::episodes_of_season(&raw);
    store::upsert_episodes(&state.pool, title_id, &episodes).await?;

    let states: Vec<EpisodeStateRow> = sqlx::query_as(
        "SELECT e.season_number,
                e.episode_number,
                s.rating,
                s.notes,
                (SELECT COUNT(*) FROM watch_event w WHERE w.episode_id = e.id)
           FROM episode e
           LEFT JOIN episode_state s ON s.episode_id = e.id
          WHERE e.title_id = ? AND e.season_number = ?",
    )
    .bind(title_id)
    .bind(season)
    .fetch_all(&state.pool)
    .await?;

    let views: Vec<EpisodeView> = episodes
        .into_iter()
        .map(|info| {
            let found = states
                .iter()
                .find(|(s, e, _, _, _)| *s == info.season_number && *e == info.episode_number);
            let (rating, notes, count) = match found {
                Some((_, _, r, n, c)) => (*r, n.clone(), *c),
                None => (None, None, 0),
            };
            EpisodeView {
                info,
                watched: count > 0,
                watch_count: count,
                rating,
                notes,
            }
        })
        .collect();

    Ok(Json(json!({
        "season_number": season,
        "name": models::str_at(&raw, "name"),
        "overview": models::str_at(&raw, "overview"),
        "poster_path": models::str_at(&raw, "poster_path"),
        "air_date": models::str_at(&raw, "air_date"),
        "episodes": views,
    })))
}

/// A person, with their filmography marked up by what I have seen.
pub async fn person(
    _: Auth,
    State(state): State<AppState>,
    Path(person_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let raw = state.tmdb.person(person_id).await?;

    let mut credits: Vec<Value> = Vec::new();
    for section in ["cast", "crew"] {
        let Some(list) = raw
            .get("combined_credits")
            .and_then(|c| c.get(section))
            .and_then(Value::as_array)
        else {
            continue;
        };

        for c in list {
            let kind = match models::str_at(c, "media_type") {
                Some(k) if k == "movie" || k == "tv" => k,
                _ => continue,
            };
            let Some(mut item) = SearchItem::from_value(c, Some(&kind)) else {
                continue;
            };
            // Bit parts and uncredited crew drown the list; keep what is known.
            if item.vote.is_none() && models::i64_at(c, "vote_count").unwrap_or(0) < 5 {
                continue;
            }
            item.overview = None;
            credits.push(json!({
                "item": item,
                "role": models::str_at(c, "character").or_else(|| models::str_at(c, "job")),
                "department": section,
            }));
        }
    }

    // Newest first; undated credits (announced projects) sink to the bottom.
    credits.sort_by_key(|c| {
        -c.get("item")
            .and_then(|i| i.get("year"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    });
    credits.truncate(120);

    let mut items: Vec<SearchItem> = credits
        .iter()
        .filter_map(|c| serde_json::from_value(c.get("item").cloned()?).ok())
        .collect();
    crate::routes::search::annotate_library(&state, &mut items).await?;
    for (c, item) in credits.iter_mut().zip(items) {
        if let Some(obj) = c.as_object_mut() {
            obj.insert("item".into(), serde_json::to_value(item)?);
        }
    }

    Ok(Json(json!({
        "id": person_id,
        "name": models::str_at(&raw, "name"),
        "biography": models::str_at(&raw, "biography"),
        "profile_path": models::str_at(&raw, "profile_path"),
        "birthday": models::str_at(&raw, "birthday"),
        "deathday": models::str_at(&raw, "deathday"),
        "place_of_birth": models::str_at(&raw, "place_of_birth"),
        "known_for_department": models::str_at(&raw, "known_for_department"),
        "credits": credits,
    })))
}
