use axum::extract::{Path, State};
use axum::Json;
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::catalog::{self, ExternalId};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store;
use crate::tmdb::models::{self, EpisodeInfo, SearchItem};

#[derive(Serialize)]
pub struct EpisodeView {
    #[serde(flatten)]
    pub info: EpisodeInfo,
    pub watched: bool,
    pub watch_count: i64,
    pub rating: Option<f64>,
    pub notes: Option<String>,
    /// When the latest watch was, so taking back an untick restores it.
    pub watched_at: Option<String>,
    pub undated: bool,
    /// The moment it comes out, when AniList or TVmaze knows the time.
    pub air_at: Option<String>,
}

/// A title as a provider's address names it: `movie/550` and `tv/1399` are
/// TMDB's, `anilist/21` AniList's. `tv/-21` is how an AniList-only title was
/// addressed before Frog had ids of its own, and still finds it.
pub fn provider_address(kind: &str, id: i64) -> AppResult<ExternalId> {
    match kind {
        "movie" | "tv" if id > 0 => Ok(ExternalId::tmdb(kind, id)),
        "movie" | "tv" if id < 0 => Ok(ExternalId::AniList(-id)),
        "anilist" if id > 0 => Ok(ExternalId::AniList(id)),
        _ => Err(AppError::NotFound("no such title".into())),
    }
}

/// The title a provider's address names, put in the catalog if it is new.
/// An AniList address for something TMDB has (reached from a person, a
/// studio or a related work) is that TMDB title: no second copy is made. One
/// a sync already keeps, because its place is not certain, stays what it is.
pub async fn by_address(state: &AppState, kind: &str, id: i64) -> AppResult<i64> {
    let address = provider_address(kind, id)?;
    if let Some(title_id) = catalog::find(&state.pool, &address).await? {
        return Ok(title_id);
    }
    let address = match address {
        ExternalId::AniList(media_id) => {
            let mapped = crate::anilist::tmdb_of(&state.pool, media_id).await?;
            match mapped {
                Some((kind, tmdb_id)) => ExternalId::tmdb(&kind, tmdb_id),
                None => address,
            }
        }
        other => other,
    };
    Ok(catalog::ensure(state, &address).await?.0)
}

/// A title page reached by a provider's address. The answer carries Frog's
/// id, which the page then uses as its own address.
pub async fn detail(
    auth: Auth,
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, i64)>,
) -> AppResult<Json<Value>> {
    let title_id = by_address(&state, &kind, id).await?;
    detail_of(&state, auth.user_id(), title_id).await
}

/// One request per title page. Everything the page draws above the fold comes
/// back here; seasons are fetched lazily as they are opened.
pub async fn detail_by_id(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
) -> AppResult<Json<Value>> {
    detail_of(&state, auth.user_id(), title_id).await
}

/// What other sites score a title, and its age rating. Apart from the page
/// itself, since a first look may ask another site.
pub async fn scores(
    _: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
) -> AppResult<Json<Value>> {
    catalog::get_title(&state.pool, title_id).await?;
    Ok(Json(crate::ratings::for_page(&state, title_id).await?))
}

async fn detail_of(state: &AppState, user_id: i64, title_id: i64) -> AppResult<Json<Value>> {
    let (mut row, raw) = catalog::load(state, title_id).await?;
    if row.logo_path.is_none() {
        row.logo_path = crate::fanart::first_look(state, title_id).await;
    }
    let kind = row.kind.clone();
    let external = catalog::externals(&state.pool, title_id).await?;
    let anilist_only = external.primary == Some("anilist");
    let item = store::build_item(&state.pool, user_id, row).await?;

    let collection = match item.title.collection_id {
        Some(cid) => collection_parts(state, user_id, cid).await.ok(),
        // A show TMDB keeps no collection for may still be where a film
        // collection began; never long, since the page waits on it.
        None if kind == "tv" && !anilist_only => {
            let (owned, name) = (state.clone(), item.title.name.clone());
            let look =
                tokio::spawn(
                    async move { crate::franchise::collection_of_show(&owned, &name).await },
                );
            match tokio::time::timeout(FRANCHISE_WAIT, look).await {
                Ok(Ok(Some(cid))) => collection_parts(state, user_id, cid).await.ok(),
                _ => None,
            }
        }
        // AniList's related works, the franchise of a title TMDB lacks.
        None if anilist_only => {
            let mut parts = models::related(&raw, "related", 24);
            parts.sort_by_key(|p| p.year.unwrap_or(9999));
            crate::routes::search::annotate_library(state, user_id, &mut parts).await?;
            (!parts.is_empty()).then(|| json!({ "id": 0, "name": "Related", "parts": parts }))
        }
        None => None,
    };

    // An AniList-only title TMDB has since found: the page goes there.
    let moved = catalog::merged_into(&state.pool, title_id).await?;

    let next_up = if kind == "tv" {
        // A show kept with no episodes listed gets them behind the page, which
        // never waits for TMDB. The worker does the same for those not opened.
        if item.entry.is_some() {
            let (state, detail) = (state.clone(), raw.clone());
            tokio::spawn(async move {
                if let Err(e) = catalog::ensure_episodes(&state, title_id, Some(&detail)).await {
                    tracing::debug!(title_id, error = %e, "episodes not listed");
                }
            });
        }
        store::next_episode(&state.pool, user_id, title_id).await?
    } else {
        None
    };

    let history = store::history(&state.pool, user_id, title_id, &kind).await?;
    let others = store::others_with(&state.pool, user_id, title_id).await?;

    Ok(Json(json!({
        "item": item,
        "history": history,
        "others": others,
        "external": external,
        "tagline": models::str_at(&raw, "tagline"),
        "age_rating": models::age_rating(&raw, state.tmdb.region()),
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
                crate::release::rating_units(&state.pool, user_id, title_id).await?,
            )?
        } else {
            json!([])
        },
        "moved_to": moved.map(|id| json!({ "id": id })),
        // Kept from AniList alone: no TMDB entry, so no cast, seasons or links.
        "anilist_only": anilist_only,
    })))
}

/// How long a title page waits on TMDB to name a franchise's shows. The
/// lookup runs on without the page when it takes longer, so its answer is
/// cached for the next visit instead of being thrown away and asked again.
const FRANCHISE_WAIT: std::time::Duration = std::time::Duration::from_millis(700);

/// Films in the same franchise, and the shows by its name, in release order:
/// the watch order that is defensible without hand-maintained lists.
async fn collection_parts(state: &AppState, user_id: i64, collection_id: i64) -> AppResult<Value> {
    let raw = state
        .tmdb
        .get(
            &format!("/collection/{collection_id}"),
            &[],
            Duration::hours(24 * 7),
        )
        .await?;

    let parts: Vec<SearchItem> = raw
        .get("parts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| SearchItem::from_value(p, Some("movie")))
                .collect()
        })
        .unwrap_or_default();

    // The shows of the same franchise, which TMDB never puts in a collection.
    let name = models::str_at(&raw, "name").unwrap_or_default();
    let owned = state.clone();
    let look = tokio::spawn(async move { crate::franchise::shows_of(&owned, &name).await });
    let shows = match tokio::time::timeout(FRANCHISE_WAIT, look).await {
        Ok(Ok(shows)) => shows,
        _ => Vec::new(),
    };
    let mut parts = models::unique_items(parts.into_iter().chain(shows).collect());
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

/// Episodes of one season, merged with my watches, ratings and notes.
pub async fn season(
    auth: Auth,
    State(state): State<AppState>,
    Path((title_id, season)): Path<(i64, i64)>,
) -> AppResult<Json<Value>> {
    if catalog::get_title(&state.pool, title_id).await?.kind != "tv" {
        return Err(AppError::BadRequest("only shows have seasons".into()));
    }

    let (raw, episodes) = match catalog::primary(&state.pool, title_id).await? {
        Some(ExternalId::Tmdb { id: tmdb_id, .. }) => {
            let raw = state.tmdb.season(tmdb_id, season).await?;
            let episodes = models::episodes_of_season(&raw);
            catalog::upsert_episodes(&state.pool, title_id, &episodes).await?;
            (raw, episodes)
        }
        // From AniList, or kept without a provider: its episodes are stored.
        _ => (
            json!({}),
            catalog::stored_episodes(&state.pool, title_id, season).await?,
        ),
    };

    let states = store::season_states(&state.pool, auth.user_id(), title_id, season).await?;

    let views: Vec<EpisodeView> = episodes
        .into_iter()
        .map(|info| {
            let found = states
                .iter()
                .find(|row| row.0 == info.season_number && row.1 == info.episode_number);
            let (rating, notes, count, at, undated, air_at) = match found {
                Some((_, _, r, n, c, at, u, air_at)) => (
                    *r,
                    n.clone(),
                    *c,
                    at.clone(),
                    u.unwrap_or(false),
                    air_at.clone(),
                ),
                None => (None, None, 0, None, false, None),
            };
            EpisodeView {
                info,
                air_at,
                watched: count > 0,
                watch_count: count,
                watched_at: at,
                undated,
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
            match merged.iter_mut().find(|m| m.0.same_as(&c.item)) {
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
    let mut seen: std::collections::HashMap<(String, Option<i64>), usize> = Default::default();
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
            let genres: Vec<i64> = credit["item"]["genre_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
            let appearance = is_appearance(credit["department"].as_str(), &roles, &genres);
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

/// TMDB's genres for talk shows, news and reality television.
const NOT_A_PART: [i64; 3] = [10767, 10763, 10764];
/// TMDB's documentary genre.
const DOCUMENTARY: i64 = 99;

/// Whether a credit is someone appearing as themselves rather than playing a
/// part or making the thing: "Self", "Himself", a talk show, a reality show,
/// or being in a documentary without a character. The person page shows
/// these apart, after the work, so a filmography leads with the work.
fn is_appearance(department: Option<&str>, roles: &[String], genres: &[i64]) -> bool {
    let as_themselves = !roles.is_empty()
        && roles.iter().all(|r| {
            let r = r.to_ascii_lowercase();
            r.starts_with("self") || r.starts_with("himself") || r.starts_with("herself")
        });
    if as_themselves {
        return true;
    }
    if department != Some("cast") {
        return false;
    }
    genres.iter().any(|g| NOT_A_PART.contains(g))
        || (genres.contains(&DOCUMENTARY) && roles.is_empty())
}

#[derive(Deserialize)]
pub struct RewatchBody {
    /// When it was watched again; now when absent.
    pub watched_at: Option<String>,
    #[serde(default)]
    pub undated: bool,
}

/// Watched it again: a film gets another watch, a show another time through.
pub async fn rewatch(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
    Json(body): Json<RewatchBody>,
) -> AppResult<Json<Value>> {
    let row = catalog::get_title(&state.pool, title_id).await?;
    if store::get_entry(&state.pool, auth.user_id(), title_id)
        .await?
        .is_none()
    {
        return Err(AppError::BadRequest("Add it to your library first.".into()));
    }
    let at = body
        .watched_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    store::add_rewatch(
        &state.pool,
        auth.user_id(),
        title_id,
        &row.kind,
        &at,
        body.undated,
    )
    .await?;
    let history = store::history(&state.pool, auth.user_id(), title_id, &row.kind).await?;
    Ok(Json(json!({ "history": history })))
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
    Path((title_id, season)): Path<(i64, i64)>,
    Json(body): Json<SeasonRatingBody>,
) -> AppResult<Json<Value>> {
    if catalog::get_title(&state.pool, title_id).await?.kind != "tv" {
        return Err(AppError::BadRequest(
            "only shows have seasons to rate".into(),
        ));
    }
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
    Ok(Json(json!({
        "ratings": crate::release::rating_units(&state.pool, auth.user_id(), title_id).await?
    })))
}

#[cfg(test)]
mod tests {
    use super::is_appearance;

    #[test]
    fn talk_shows_and_playing_yourself_are_appearances_and_parts_are_not() {
        let r = |s: &str| vec![s.to_string()];
        assert!(is_appearance(Some("cast"), &r("Self - Guest"), &[35]));
        assert!(
            is_appearance(Some("cast"), &r("Guest"), &[10767]),
            "a talk show"
        );
        assert!(
            is_appearance(Some("cast"), &[], &[99]),
            "a documentary, no part"
        );
        assert!(
            !is_appearance(Some("cast"), &r("Narrator"), &[99]),
            "a documentary's narrator"
        );
        assert!(!is_appearance(Some("cast"), &r("Walter White"), &[18]));
        assert!(
            !is_appearance(Some("crew"), &r("Producer"), &[10767]),
            "making one is work"
        );
    }
}
