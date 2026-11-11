//! Searching for more than films and shows: people, characters, studios and
//! manga, and a page for each studio. Members are searched in the browser from the profiles list,
//! which already holds only the people who chose to be seen.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::timeout;

use crate::anilist;
use crate::auth::Auth;
use crate::catalog::ExternalId;
use crate::error::{AppError, AppResult};
use crate::routes::search::annotate_library;
use crate::state::AppState;
use crate::studios;
use crate::text::{self, Closeness};
use crate::tmdb::models::{self, SearchItem};

#[derive(Deserialize)]
pub struct MoreQuery {
    pub q: String,
}

/// How long a search waits for AniList's manga and characters, asked in one
/// call. AniList is asked a couple of seconds apart, so this covers one turn
/// in the queue behind a sync, and no more.
const ANILIST_WAIT: Duration = Duration::from_millis(3500);

/// People, characters, studios and manga for one search, each list on its own
/// so a slow or failing source leaves the others standing.
pub async fn more(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<MoreQuery>,
) -> AppResult<Json<Value>> {
    let term = q.q.trim();
    if term.is_empty() {
        return Err(AppError::BadRequest(
            "give me something to search for".into(),
        ));
    }
    let (people, studios, named) = tokio::join!(
        people(&state, term),
        studios(&state, term),
        by_name(&state, term)
    );
    let named = named.unwrap_or(Value::Null);
    let characters = characters(&state, term, &named).await;
    let manga = manga(&state, auth.user_id(), term, &named).await;
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
        "manga": quiet(manga, "manga"),
    })))
}

/// AniList's manga and characters for a name, in one call, or nothing when
/// AniList is not set up, too slow or failing: the rest of the search stands.
async fn by_name(state: &AppState, term: &str) -> Option<Value> {
    if term.chars().count() < 2 || state.config.anilist.is_none() {
        return None;
    }
    match timeout(ANILIST_WAIT, anilist::media::search_by_name(state, term)).await {
        Ok(Ok(data)) => Some(data),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "AniList name search failed");
            None
        }
        Err(_) => {
            tracing::debug!("AniList name search skipped: too slow");
            None
        }
    }
}

/// Manga by name from AniList, marked with what is in the library already.
async fn manga(state: &AppState, user_id: i64, term: &str, named: &Value) -> AppResult<Vec<Value>> {
    if named.is_null() {
        return Ok(Vec::new());
    }
    let mut found = anilist::media::manga_found(state, named, term).await?;
    annotate_library(state, user_id, &mut found).await?;
    Ok(found
        .into_iter()
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
        .collect())
}

/// TMDB finds people by every name they have ever gone by, so "Tom & Jerry"
/// answers with two actors once credited that way. When the name is plainly
/// a title's, and a person's own name does not answer it, they go.
async fn names_a_title(state: &AppState, typed: &[String]) -> bool {
    let term = typed.join(" ");
    let Ok(raw) = state.tmdb.search_multi(&term, 1).await else {
        return false;
    };
    raw["results"].as_array().into_iter().flatten().any(|r| {
        models::str_at(r, "title")
            .or_else(|| models::str_at(r, "name"))
            .filter(|_| r["media_type"].as_str() != Some("person"))
            .is_some_and(|name| text::closeness(&name, typed) <= Closeness::Starts)
    })
}

async fn people(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let raw = state.tmdb.search_person(term).await?;
    let typed = text::words(term);
    let mut found: Vec<(Closeness, usize, Value)> = raw["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(20)
        .enumerate()
        .filter_map(|(i, p)| {
            let known: Vec<String> = p["known_for"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| models::str_at(t, "title").or_else(|| models::str_at(t, "name")))
                .take(3)
                .collect();
            let name = models::str_at(p, "name")?;
            // Known for nothing: a page with no credits to show.
            if known.is_empty() {
                return None;
            }
            Some((
                text::closeness(&name, &typed),
                i,
                json!({
                    "id": p["id"].as_i64()?,
                    "name": name,
                    "profile_path": models::str_at(p, "profile_path"),
                    "department": models::str_at(p, "known_for_department"),
                    "known_for": known,
                }),
            ))
        })
        .collect();
    let answered = found.iter().any(|f| f.0 < Closeness::Loose);
    if !answered && !found.is_empty() && names_a_title(state, &typed).await {
        found.clear();
    }
    if answered {
        // Someone by this name exists; aliases of others are noise.
        found.retain(|f| f.0 < Closeness::Loose);
    }
    found.sort_by_key(|f| (f.0, f.1));
    Ok(found.into_iter().map(|f| f.2).collect())
}

/// One company's row, ready for ranking.
struct Studio {
    rank: (Closeness, bool),
    plain: String,
    order: usize,
    value: Value,
}

/// Studios from TMDB's company search and from the titles this Frog holds.
///
/// TMDB lists every shell company that ever shared a word with the query, and
/// several entries for one studio, usually only one of them with a logo: that
/// is why so many came up blank. Entries with the same name become one, the
/// one with a logo, and a logo-less one only stays if its name answers what
/// was typed. A studio with no logo anywhere keeps its initial; nothing is
/// guessed.
async fn studios(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let typed = text::words(term);
    let raw = state.tmdb.search_company(term).await?;
    let mut found: Vec<Studio> = Vec::new();
    let mut add = |id: i64, name: String, logo: Option<String>, country: Option<String>| {
        let close = text::closeness(&name, &typed);
        if logo.is_none() && close > Closeness::Starts {
            return;
        }
        let plain = anilist::plain(&name);
        if let Some(known) = found.iter_mut().find(|s| s.plain == plain) {
            if known.value["logo_path"].is_null() && logo.is_some() {
                known.value["logo_path"] = json!(logo);
                known.value["id"] = json!(id);
                known.rank.1 = false;
            }
            return;
        }
        let order = found.len();
        found.push(Studio {
            rank: (close, logo.is_none()),
            plain,
            order,
            value: json!({ "id": id, "name": name, "logo_path": logo, "country": country }),
        });
    };
    for c in raw["results"].as_array().into_iter().flatten() {
        let (Some(id), Some(name)) = (c["id"].as_i64(), models::str_at(c, "name")) else {
            continue;
        };
        add(
            id,
            name,
            models::str_at(c, "logo_path"),
            models::str_at(c, "origin_country").filter(|s| !s.is_empty()),
        );
    }
    let pattern = term.replace(['\\', '%', '_'], "");
    let known = studios::known(&state.pool, &pattern).await?;
    for (id, name, logo, _) in known {
        add(id, name, logo, None);
    }
    // A studio by this name exists: "A25" is not what "A24" was looking for.
    if found.iter().any(|s| s.rank.0 < Closeness::Loose) {
        found.retain(|s| s.rank.0 < Closeness::Loose);
    }
    found.sort_by_key(|s| (s.rank, s.order));
    found.truncate(20);

    // A studio with no logo is often a shell with nothing to its name, whose
    // page would be empty: those go. One whose titles this Frog holds, or
    // that has a logo, is kept without asking.
    let ids: Vec<i64> = found
        .iter()
        .filter_map(|s| s.value["id"].as_i64())
        .collect();
    let held = studios::held(&state.pool, &ids).await?;
    let checks = found.iter().map(|s| {
        let id = s.value["id"].as_i64().unwrap_or(0);
        let ask = s.value["logo_path"].is_null() && !held.contains(&id);
        async move { !ask || has_titles(state, id).await }
    });
    let keep = futures_util::future::join_all(checks).await;
    Ok(found
        .into_iter()
        .zip(keep)
        .filter_map(|(s, keep)| keep.then_some(s.value))
        .collect())
}

/// Whether TMDB has any film or show under a company. A failed answer keeps
/// the company: better a page with less than a search with gaps.
async fn has_titles(state: &AppState, id: i64) -> bool {
    for kind in ["movie", "tv"] {
        match state.tmdb.by_company(kind, id, 1).await {
            Ok(r) if r["total_results"].as_i64().unwrap_or(0) > 0 => return true,
            Ok(_) => {}
            Err(_) => return true,
        }
    }
    false
}

/// Characters from two places: the casts of every film and show this Frog
/// holds, and AniList's character search for anime, whose casts TMDB mostly
/// lists by voice actor alone.
async fn characters(state: &AppState, term: &str, named: &Value) -> AppResult<Vec<Value>> {
    let mut out = cast_characters(state, term).await?;
    if !named.is_null() {
        out.extend(anime_characters(state, term, named).await?);
    }
    Ok(out)
}

/// A character as search shows one: the character's own name and the title
/// they are from. Their picture is the title's poster, never the actor's
/// photo: a face belongs to a person, and People is where people are.
async fn cast_characters(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let typed = text::words(term);
    if typed.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = typed.join(" ").replace(['\\', '%', '_'], "");
    let rows = crate::catalog::characters_named(&state.pool, &pattern).await?;

    let mut out: Vec<(Closeness, Value)> = Vec::new();
    for (id, kind, provider, key, title, poster, character, actor, actor_id) in rows {
        let source = provider
            .zip(key)
            .and_then(|(p, k)| ExternalId::parse(&p, &k));
        let same = out.iter().any(|(_, o)| {
            o["name"].as_str() == Some(character.as_str()) && o["title"]["id"].as_i64() == Some(id)
        });
        if same {
            continue;
        }
        out.push((
            text::closeness(&character, &typed),
            json!({
                "name": character,
                "image": poster.as_ref().map(|p| json!({ "source": "tmdb", "path": p })),
                "actor": actor.map(|name| json!({ "id": actor_id, "name": name })),
                "title": {
                    "id": id,
                    "kind": kind,
                    "tmdb_id": source.as_ref().and_then(ExternalId::tmdb_id),
                    "name": title,
                    "poster_path": poster,
                },
            }),
        ));
    }
    out.sort_by_key(|(close, _)| *close);
    Ok(out.into_iter().take(20).map(|(_, v)| v).collect())
}

/// AniList's own CDN path for an image, which the image route fetches from
/// AniList and keeps, the way it does TMDB's.
fn anilist_image(url: &str) -> Option<String> {
    url.strip_prefix("https://s4.anilist.co/file/anilistcdn/")
        .map(str::to_string)
}

/// The characters in an AniList name search (`anilist::media::search_by_name`).
async fn anime_characters(state: &AppState, term: &str, data: &Value) -> AppResult<Vec<Value>> {
    // AniList matches loosely ("nolan" finds "Norland"); keep the characters
    // whose name, in any form, really holds what was typed.
    let needle = anilist::plain(term);
    let mut out = Vec::new();
    for c in data["characters"]["characters"]
        .as_array()
        .into_iter()
        .flatten()
    {
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
        // A character from no anime has nothing to lead to.
        if media.is_null() {
            continue;
        }
        let show = media["title"]["english"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| media["title"]["romaji"].as_str());
        // Which TMDB title that is, if Fribb's list knows it.
        let placed = match media["id"].as_i64() {
            Some(id) => anilist::tmdb_of(&state.pool, id).await?,
            None => None,
        };
        let title = match placed {
            Some((kind, tmdb_id)) => {
                let poster =
                    crate::catalog::poster_of(&state.pool, &ExternalId::tmdb(&kind, tmdb_id))
                        .await?;
                json!({ "kind": kind, "tmdb_id": tmdb_id, "name": show,
                        "poster_path": poster })
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

#[derive(Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    page: Option<u32>,
}

/// A studio, and what it made, films and shows together, most popular first,
/// a page at a time: everything it made, not only the first page.
pub async fn studio(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    let page = q.page.unwrap_or(1).clamp(1, 500);
    // A studio AniList knows: the same page, from AniList.
    if id < 0 {
        let (mut header, mut items, more) = anilist::media::studio(&state, -id, page).await?;
        annotate_library(&state, auth.user_id(), &mut items).await?;
        if let Some(name) = header["name"].as_str().map(str::to_string) {
            header["logo_path"] = json!(studios::logo(&state, &name).await);
        }
        header["titles"] = json!(items);
        header["next_page"] = json!(more.then_some(page + 1));
        return Ok(Json(header));
    }
    let raw = state.tmdb.company(id).await?;
    let (films, shows) = tokio::join!(
        state.tmdb.by_company("movie", id, page),
        state.tmdb.by_company("tv", id, page)
    );
    let mut items: Vec<SearchItem> = Vec::new();
    let mut more = false;
    for (kind, list) in [("movie", films), ("tv", shows)] {
        let Ok(list) = list else { continue };
        let total = list["total_pages"].as_u64().unwrap_or(0) as u32;
        more |= page < total.min(500);
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
        "next_page": more.then_some(page + 1),
    })))
}
