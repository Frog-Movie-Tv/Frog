//! Searching for more than titles: people, characters and studios, and a page
//! for each studio. Members are searched in the browser from the profiles list,
//! which already holds only the people who chose to be seen.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::anilist::{self, Api};
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::routes::search::annotate_library;
use crate::state::AppState;
use crate::tmdb::models::{self, SearchItem};

#[derive(Deserialize)]
pub struct MoreQuery {
    pub q: String,
}

/// People, characters and studios for one search, each list on its own so a
/// slow or failing source leaves the others standing.
pub async fn more(
    _: Auth,
    State(state): State<AppState>,
    Query(q): Query<MoreQuery>,
) -> AppResult<Json<Value>> {
    let term = q.q.trim();
    if term.is_empty() {
        return Err(AppError::BadRequest(
            "give me something to search for".into(),
        ));
    }
    let (people, studios, characters) = tokio::join!(
        people(&state, term),
        studios(&state, term),
        characters(&state, term)
    );
    let quiet = |r: AppResult<Vec<Value>>, what: &str| {
        r.unwrap_or_else(|e| {
            tracing::warn!(error = %e, what, "search source failed");
            Vec::new()
        })
    };
    Ok(Json(json!({
        "people": quiet(people, "people"),
        "studios": quiet(studios, "studios"),
        "characters": quiet(characters, "characters"),
    })))
}

async fn people(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let raw = state.tmdb.search_person(term).await?;
    Ok(raw["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(20)
        .filter_map(|p| {
            let known: Vec<String> = p["known_for"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| models::str_at(t, "title").or_else(|| models::str_at(t, "name")))
                .take(3)
                .collect();
            Some(json!({
                "id": p["id"].as_i64()?,
                "name": models::str_at(p, "name")?,
                "profile_path": models::str_at(p, "profile_path"),
                "department": models::str_at(p, "known_for_department"),
                "known_for": known,
            }))
        })
        .collect())
}

async fn studios(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let raw = state.tmdb.search_company(term).await?;
    let needle = anilist::plain(term);
    let mut found: Vec<(u8, usize, Value)> = raw["results"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = models::str_at(c, "name")?;
            let logo = models::str_at(c, "logo_path");
            // TMDB lists every shell company that ever shared a word with
            // the query; the one named exactly that, and those with a logo,
            // are the ones people mean.
            let rank = u8::from(anilist::plain(&name) != needle) * 2 + u8::from(logo.is_none());
            Some((
                rank,
                i,
                json!({
                    "id": c["id"].as_i64()?,
                    "name": name,
                    "logo_path": logo,
                    "country": models::str_at(c, "origin_country").filter(|s| !s.is_empty()),
                }),
            ))
        })
        .collect();
    found.sort_by_key(|(rank, i, _)| (*rank, *i));
    Ok(found.into_iter().take(20).map(|(_, _, v)| v).collect())
}

/// Characters from two places: the casts of every film and show this Frog
/// has already looked up, and AniList's character search for anime, whose
/// casts TMDB mostly lists by voice actor alone.
async fn characters(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let mut out = cast_characters(state, term).await?;
    if term.chars().count() >= 2 {
        match anime_characters(state, term).await {
            Ok(found) => out.extend(found),
            Err(e) => tracing::warn!(error = %e, "AniList character search failed"),
        }
    }
    Ok(out)
}

/// (kind, tmdb_id, title, poster, character, actor, actor id, actor photo)
type CastRow = (
    String,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
);

async fn cast_characters(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let rows: Vec<CastRow> = sqlx::query_as(
        "SELECT CASE WHEN c.cache_key LIKE '/movie/%' THEN 'movie' ELSE 'tv' END,
                json_extract(c.body, '$.id'),
                COALESCE(json_extract(c.body, '$.title'), json_extract(c.body, '$.name')),
                json_extract(c.body, '$.poster_path'),
                json_extract(j.value, '$.character'),
                json_extract(j.value, '$.name'),
                json_extract(j.value, '$.id'),
                json_extract(j.value, '$.profile_path')
           FROM tmdb_cache c, json_each(c.body, '$.credits.cast') j
          WHERE (c.cache_key LIKE '/movie/%' OR c.cache_key LIKE '/tv/%')
            AND c.cache_key NOT LIKE '%/season/%'
            AND instr(lower(c.body), lower(?1)) > 0
            AND instr(lower(json_extract(j.value, '$.character')), lower(?1)) > 0
          ORDER BY json_extract(c.body, '$.popularity') DESC, j.key
          LIMIT 40",
    )
    .bind(term)
    .fetch_all(&state.pool)
    .await?;

    let mut out: Vec<Value> = Vec::new();
    for (kind, tmdb_id, title, poster, character, actor, actor_id, photo) in rows {
        let Some(character) = character.filter(|c| !c.trim().is_empty()) else {
            continue;
        };
        // "Self" and "Narrator (voice)" are not characters anyone looks for.
        if character.to_lowercase().starts_with("self") {
            continue;
        }
        let same = out.iter().any(|o| {
            o["name"].as_str() == Some(character.as_str())
                && o["title"]["tmdb_id"].as_i64() == Some(tmdb_id)
        });
        if same {
            continue;
        }
        out.push(json!({
            "name": character,
            "image": photo.map(|p| json!({ "source": "tmdb", "path": p })),
            "actor": actor.map(|name| json!({ "id": actor_id, "name": name })),
            "title": { "kind": kind, "tmdb_id": tmdb_id, "name": title, "poster_path": poster },
        }));
        if out.len() >= 20 {
            break;
        }
    }
    Ok(out)
}

const CHARACTER_QUERY: &str = "query ($q: String) {
  Page(perPage: 12) {
    characters(search: $q, sort: [SEARCH_MATCH, FAVOURITES_DESC]) {
      id name { full native alternative } image { medium }
      media(perPage: 1, sort: POPULARITY_DESC, type: ANIME) {
        nodes { id format title { english romaji } coverImage { extraLarge } }
      }
    }
  }
}";

/// AniList's own CDN path for an image, which the image route fetches from
/// AniList and keeps, the way it does TMDB's.
fn anilist_image(url: &str) -> Option<String> {
    url.strip_prefix("https://s4.anilist.co/file/anilistcdn/")
        .map(str::to_string)
}

async fn anime_characters(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    if state.config.anilist.is_none() {
        return Ok(Vec::new());
    }
    let key = format!("anilist:characters:3:{}", term.to_lowercase());
    let cached: Option<(String,)> =
        sqlx::query_as("SELECT body FROM tmdb_cache WHERE cache_key = ? AND expires_at > ?")
            .bind(&key)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&state.pool)
            .await?;
    let data: Value = match cached.and_then(|(b,)| serde_json::from_str(&b).ok()) {
        Some(v) => v,
        None => {
            // Anonymous: searching AniList needs nobody's sign-in.
            let data = Api::new(state)?
                .graphql("", CHARACTER_QUERY, json!({ "q": term }))
                .await?;
            let now = Utc::now();
            sqlx::query(
                "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at) VALUES (?, ?, ?, ?)
                 ON CONFLICT (cache_key) DO UPDATE SET
                    body = excluded.body, fetched_at = excluded.fetched_at,
                    expires_at = excluded.expires_at",
            )
            .bind(&key)
            .bind(data.to_string())
            .bind(now.to_rfc3339())
            .bind((now + chrono::Duration::hours(12)).to_rfc3339())
            .execute(&state.pool)
            .await?;
            data
        }
    };

    // AniList matches loosely ("nolan" finds "Norland"); keep the characters
    // whose name, in any form, really holds what was typed.
    let needle = anilist::plain(term);
    let mut out = Vec::new();
    for c in data["Page"]["characters"].as_array().into_iter().flatten() {
        let Some(name) = c["name"]["full"].as_str() else {
            continue;
        };
        let names = [&c["name"]["full"], &c["name"]["native"]]
            .into_iter()
            .chain(c["name"]["alternative"].as_array().into_iter().flatten())
            .filter_map(Value::as_str);
        if !names.map(anilist::plain).any(|n| n.contains(&needle)) {
            continue;
        }
        let media = &c["media"]["nodes"][0];
        let show = media["title"]["english"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| media["title"]["romaji"].as_str());
        // Which TMDB title that is, if Fribb's list knows it.
        let placed: Option<(String, i64)> = match media["id"].as_i64() {
            Some(id) => {
                sqlx::query_as(
                    "SELECT tmdb_kind, tmdb_id FROM anime_map
                  WHERE anilist_id = ? AND tmdb_kind IS NOT NULL AND tmdb_id IS NOT NULL",
                )
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
            }
            None => None,
        };
        let title = match placed {
            Some((kind, tmdb_id)) => {
                let poster: Option<(Option<String>,)> = sqlx::query_as(
                    "SELECT t.poster_path FROM title_external x JOIN title t ON t.id = x.title_id
                      WHERE x.provider = 'tmdb' AND x.external_id = ?",
                )
                .bind(crate::catalog::ExternalId::tmdb(&kind, tmdb_id).key())
                .fetch_optional(&state.pool)
                .await?;
                json!({ "kind": kind, "tmdb_id": tmdb_id, "name": show,
                        "poster_path": poster.and_then(|p| p.0) })
            }
            // Not on TMDB: the page Frog makes from AniList.
            None => match media["id"].as_i64() {
                Some(id) => json!({
                    "kind": if media["format"].as_str() == Some("MOVIE") { "movie" } else { "tv" },
                    "tmdb_id": null,
                    "anilist_id": id,
                    "name": show,
                    "poster_path": anilist::media::image_path(media["coverImage"]["extraLarge"].as_str()),
                }),
                None => json!({ "kind": null, "tmdb_id": null, "name": show, "poster_path": null }),
            },
        };
        out.push(json!({
            "name": name,
            "image": c["image"]["medium"]
                .as_str()
                .and_then(anilist_image)
                .map(|p| json!({ "source": "anilist", "path": p })),
            "actor": null,
            "title": title,
        }));
    }
    Ok(out)
}

/// A studio, and the most popular of what it made, films and shows together.
pub async fn studio(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    // A studio AniList knows: the same page, from AniList.
    if id < 0 {
        let (mut header, mut items) = anilist::media::studio(&state, -id).await?;
        annotate_library(&state, auth.user_id(), &mut items).await?;
        header["titles"] = json!(items);
        return Ok(Json(header));
    }
    let raw = state.tmdb.company(id).await?;
    let (films, shows) = tokio::join!(
        state.tmdb.by_company("movie", id, 1),
        state.tmdb.by_company("tv", id, 1)
    );
    let mut items: Vec<SearchItem> = Vec::new();
    for (kind, list) in [("movie", films), ("tv", shows)] {
        let Ok(list) = list else { continue };
        items.extend(
            list["results"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| SearchItem::from_value(v, Some(kind))),
        );
    }
    items.sort_by(|a, b| {
        b.popularity
            .unwrap_or(0.0)
            .total_cmp(&a.popularity.unwrap_or(0.0))
    });
    for item in &mut items {
        item.overview = None;
    }
    annotate_library(&state, auth.user_id(), &mut items).await?;

    Ok(Json(json!({
        "id": id,
        "name": models::str_at(&raw, "name"),
        "logo_path": models::str_at(&raw, "logo_path"),
        "description": models::str_at(&raw, "description").filter(|s| !s.is_empty()),
        "headquarters": models::str_at(&raw, "headquarters").filter(|s| !s.is_empty()),
        "country": models::str_at(&raw, "origin_country").filter(|s| !s.is_empty()),
        "homepage": models::str_at(&raw, "homepage").filter(|s| !s.is_empty()),
        "titles": items,
    })))
}
