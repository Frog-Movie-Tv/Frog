use axum::extract::{Path, State};
use axum::Json;
use chrono::Duration;
use serde::{Deserialize, Serialize};
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
    auth: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id)): Path<(String, i64)>,
) -> AppResult<Json<Value>> {
    check_kind(&kind)?;
    // An AniList address for something TMDB has (reached from a person,
    // a studio or a related work) is that TMDB title: no second copy is
    // made. One a sync already keeps, because its place is not certain,
    // stays what it is.
    if store::is_local(tmdb_id)
        && store::find_title_id(&state.pool, &kind, tmdb_id)
            .await?
            .is_none()
    {
        let mapped: Option<(String, i64)> = sqlx::query_as(
            "SELECT tmdb_kind, tmdb_id FROM anime_map
              WHERE anilist_id = ? AND tmdb_kind IN ('movie', 'tv') AND tmdb_id IS NOT NULL",
        )
        .bind(-tmdb_id)
        .fetch_optional(&state.pool)
        .await?;
        if let Some((kind, tmdb_id)) = mapped {
            return Ok(Json(
                json!({ "moved_to": { "kind": kind, "tmdb_id": tmdb_id } }),
            ));
        }
    }
    let (title_id, raw) = store::ensure_title(&state, &kind, tmdb_id).await?;
    let row = store::get_title(&state.pool, title_id).await?;
    let item = store::build_item(&state.pool, auth.user_id(), row).await?;

    let collection = match item.title.collection_id {
        Some(cid) => collection_parts(&state, auth.user_id(), cid).await.ok(),
        // AniList's related works, the franchise of a title TMDB lacks.
        None if store::is_local(tmdb_id) => {
            let mut parts = models::related(&raw, "related", 24);
            parts.sort_by_key(|p| p.year.unwrap_or(9999));
            crate::routes::search::annotate_library(&state, auth.user_id(), &mut parts).await?;
            (!parts.is_empty()).then(|| json!({ "id": 0, "name": "Related", "parts": parts }))
        }
        None => None,
    };

    // An AniList-only title TMDB has since found: the page goes there.
    let moved: Option<(String, i64)> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id FROM title f JOIN title t ON t.id = f.merged_into
          WHERE f.id = ?",
    )
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;

    let next_up = if kind == "tv" {
        next_unwatched(&state, auth.user_id(), title_id).await?
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
        "ratings": if kind == "tv" {
            serde_json::to_value(
                crate::release::rating_units(&state.pool, auth.user_id(), title_id).await?,
            )?
        } else {
            json!([])
        },
        "heatmap": heatmap(&state, auth.user_id(), title_id).await?,
        "moved_to": moved.map(|(kind, tmdb_id)| json!({ "kind": kind, "tmdb_id": tmdb_id })),
        // Kept from AniList alone: no TMDB entry, so no cast, seasons or links.
        "anilist_only": store::is_local(tmdb_id),
    })))
}

/// (season, episode, name, overview, still, air date, runtime)
type LocalEpisodeRow = (
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

/// A season of a title from AniList, as stored.
async fn local_episodes(
    state: &AppState,
    title_id: i64,
    season: i64,
) -> AppResult<Vec<EpisodeInfo>> {
    let rows: Vec<LocalEpisodeRow> = sqlx::query_as(
        "SELECT season_number, episode_number, name, overview, still_path, air_date, runtime
           FROM episode WHERE title_id = ? AND season_number = ?
          ORDER BY episode_number",
    )
    .bind(title_id)
    .bind(season)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| EpisodeInfo {
            season_number: r.0,
            episode_number: r.1,
            tmdb_episode_id: None,
            name: r.2,
            overview: r.3,
            still_path: r.4,
            air_date: r.5,
            runtime: r.6,
            tmdb_vote: None,
        })
        .collect())
}

/// Films in the same franchise, in release order: the watch order that is
/// actually defensible without hand-maintained lists.
async fn collection_parts(state: &AppState, user_id: i64, collection_id: i64) -> AppResult<Value> {
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
    crate::routes::search::annotate_library(state, user_id, &mut parts).await?;

    Ok(json!({
        "id": collection_id,
        "name": models::str_at(&raw, "name"),
        "overview": models::str_at(&raw, "overview"),
        "poster_path": models::str_at(&raw, "poster_path"),
        "backdrop_path": models::str_at(&raw, "backdrop_path"),
        "parts": parts,
    }))
}

async fn next_unwatched(
    state: &AppState,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<NextUp>> {
    // The next one out, or the next one with a date to wait for; never an
    // announced episode nobody has a date for.
    let row: Option<NextUpRow> = sqlx::query_as(concat!(
        "SELECT e.season_number, e.episode_number, e.name, e.still_path, e.air_date
           FROM episode e
          WHERE e.title_id = ?
            AND e.season_number > 0
            AND (e.air_date IS NOT NULL OR ",
        crate::released!(),
        ")
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ? AND w.episode_id = e.id)
          ORDER BY e.season_number, e.episode_number
          LIMIT 1"
    ))
    .bind(title_id)
    .bind(user_id)
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

/// (season, episode, my rating, TMDB score, watch count)
type HeatRow = (i64, i64, Option<f64>, Option<f64>, i64);

/// Compact grid data for the per-show episode heatmap.
///
/// Carries TMDB's own episode score alongside mine, so a show you have just
/// added already shows the shape of its seasons instead of an empty grid.
async fn heatmap(state: &AppState, user_id: i64, title_id: i64) -> AppResult<Value> {
    let rows: Vec<HeatRow> = sqlx::query_as(
        "SELECT e.season_number,
                e.episode_number,
                s.rating,
                e.tmdb_vote,
                (SELECT COUNT(*) FROM watch_event w
                  WHERE w.user_id = ?1 AND w.episode_id = e.id)
           FROM episode e
           LEFT JOIN episode_state s ON s.episode_id = e.id AND s.user_id = ?1
          WHERE e.title_id = ?2 AND e.season_number > 0
          ORDER BY e.season_number, e.episode_number",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(&state.pool)
    .await?;

    let cells: Vec<Value> = rows
        .into_iter()
        .map(|(s, e, rating, tmdb, watches)| {
            json!({
                "s": s,
                "e": e,
                "rating": rating,
                // TMDB scores out of ten; everything else here is out of 100.
                "tmdb": tmdb.map(|v| v * 10.0),
                "watched": watches > 0,
            })
        })
        .collect();

    Ok(Value::Array(cells))
}

/// Episodes of one season, merged with my watches, ratings and notes.
pub async fn season(
    auth: Auth,
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

    let (raw, episodes) = if store::is_local(tmdb_id) {
        // From AniList: its episodes came with its detail and are kept.
        (json!({}), local_episodes(&state, title_id, season).await?)
    } else {
        let raw = state.tmdb.season(tmdb_id, season).await?;
        let episodes = models::episodes_of_season(&raw);
        store::upsert_episodes(&state.pool, title_id, &episodes).await?;
        (raw, episodes)
    };

    let states: Vec<EpisodeStateRow> = sqlx::query_as(
        "SELECT e.season_number,
                e.episode_number,
                s.rating,
                s.notes,
                (SELECT COUNT(*) FROM watch_event w
                  WHERE w.user_id = ?1 AND w.episode_id = e.id)
           FROM episode e
           LEFT JOIN episode_state s ON s.episode_id = e.id AND s.user_id = ?1
          WHERE e.title_id = ?2 AND e.season_number = ?3",
    )
    .bind(auth.user_id())
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
    auth: Auth,
    State(state): State<AppState>,
    Path(person_id): Path<i64>,
) -> AppResult<Json<Value>> {
    // Someone AniList knows: the same page, from AniList.
    if person_id < 0 {
        let (mut header, list) = crate::anilist::media::person(&state, -person_id).await?;
        // One card per title, with every part, as for anyone from TMDB: a
        // show can be listed once per character voiced, and as staff too.
        let mut merged: Vec<(SearchItem, &'static str, Vec<String>)> = Vec::new();
        for c in list {
            let roles = c.role.into_iter();
            match merged
                .iter_mut()
                .find(|m| m.0.kind == c.item.kind && m.0.tmdb_id == c.item.tmdb_id)
            {
                Some(m) => {
                    for role in roles {
                        if !m.2.contains(&role) {
                            m.2.push(role);
                        }
                    }
                }
                None => merged.push((c.item, c.department, roles.collect())),
            }
        }
        let mut items: Vec<SearchItem> = merged.iter().map(|m| m.0.clone()).collect();
        crate::routes::search::annotate_library(&state, auth.user_id(), &mut items).await?;
        let mut credits: Vec<Value> = merged
            .into_iter()
            .zip(items)
            .map(|((_, department, roles), item)| {
                let role = (!roles.is_empty()).then(|| roles.join(" / "));
                json!({ "item": item, "department": department, "role": role,
                        "appearance": false })
            })
            .collect();
        credits.sort_by_key(|c| -c["item"]["year"].as_i64().unwrap_or(0));
        header["credits"] = json!(credits);
        return Ok(Json(header));
    }
    let raw = state.tmdb.person(person_id).await?;

    let mut credits_by_title: Vec<(Value, Vec<String>)> = Vec::new();
    let mut seen: std::collections::HashMap<(String, i64), usize> = Default::default();
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
            let role = models::str_at(c, "character")
                .or_else(|| models::str_at(c, "job"))
                .filter(|r| !r.trim().is_empty());
            // One card per title: an actor who also produced it, or a talk
            // show visited three times, reads as one entry with every role.
            let key = (item.kind.clone(), item.tmdb_id);
            if let Some(&at) = seen.get(&key) {
                let merged: &mut (Value, Vec<String>) = &mut credits_by_title[at];
                if let Some(role) = role {
                    if !merged.1.contains(&role) {
                        merged.1.push(role);
                    }
                }
                continue;
            }
            seen.insert(key, credits_by_title.len());
            credits_by_title.push((
                json!({ "item": item, "department": section }),
                role.into_iter().collect(),
            ));
        }
    }
    let mut credits: Vec<Value> = credits_by_title
        .into_iter()
        .map(|(mut credit, roles)| {
            // "Self", "Self - Guest", "Himself": a talk show or an awards
            // night rather than a part, which the page shows apart.
            let appearance = !roles.is_empty()
                && roles.iter().all(|r| {
                    let r = r.to_ascii_lowercase();
                    r.starts_with("self") || r.starts_with("himself") || r.starts_with("herself")
                });
            if let Some(obj) = credit.as_object_mut() {
                let role = (!roles.is_empty()).then(|| roles.join(" / "));
                obj.insert("role".into(), json!(role));
                obj.insert("appearance".into(), json!(appearance));
            }
            credit
        })
        .collect();

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
    crate::routes::search::annotate_library(&state, auth.user_id(), &mut items).await?;
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

#[derive(Deserialize)]
pub struct SeasonRatingBody {
    /// 0..100, or null to take the rating off.
    pub rating: Option<f64>,
    /// Where the part starts, when AniList splits the season; 1 otherwise.
    #[serde(default = "first")]
    pub first_episode: i64,
}

fn first() -> i64 {
    1
}

/// Rate a season (or one part of it). Only a season that has come out in
/// full can be rated: a rating is a verdict on the whole of it.
pub async fn rate_season(
    auth: Auth,
    State(state): State<AppState>,
    Path((kind, tmdb_id, season)): Path<(String, i64, i64)>,
    Json(body): Json<SeasonRatingBody>,
) -> AppResult<Json<Value>> {
    if kind != "tv" {
        return Err(AppError::BadRequest(
            "only shows have seasons to rate".into(),
        ));
    }
    let title_id = store::find_title_id(&state.pool, &kind, tmdb_id)
        .await?
        .ok_or_else(|| AppError::NotFound("no such title".into()))?;
    let units = crate::release::rating_units(&state.pool, auth.user_id(), title_id).await?;
    let Some(unit) = units
        .iter()
        .find(|u| u.season == season && u.first_episode == body.first_episode)
    else {
        return Err(AppError::NotFound("no such season".into()));
    };
    if body.rating.is_some() && !unit.rateable {
        return Err(AppError::BadRequest(
            "This season can be rated once all of it is out.".into(),
        ));
    }
    if let Some(r) = body.rating {
        if !(0.0..=100.0).contains(&r) {
            return Err(AppError::BadRequest("a rating is between 0 and 100".into()));
        }
    }
    crate::anilist::set_part_rating(
        &state.pool,
        auth.user_id(),
        title_id,
        season,
        body.first_episode,
        body.rating,
    )
    .await?;
    crate::anilist::touch(&state.pool, auth.user_id()).await;
    Ok(Json(json!({
        "ratings": crate::release::rating_units(&state.pool, auth.user_id(), title_id).await?
    })))
}
