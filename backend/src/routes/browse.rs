//! Searching for more than titles: people, characters and studios, and a page
//! for each studio. Members are searched in the browser from the profiles list,
//! which already holds only the people who chose to be seen.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::timeout;

use crate::anilist::{self, Api};
use crate::auth::Auth;
use crate::catalog::ExternalId;
use crate::error::{AppError, AppResult};
use crate::routes::search::annotate_library;
use crate::state::AppState;
use crate::text::{self, Closeness};
use crate::tmdb::models::{self, SearchItem};

#[derive(Deserialize)]
pub struct MoreQuery {
    pub q: String,
}

/// How long a search waits for AniList's characters. It paces every request,
/// and a search should not wait behind a sync.
const ANIME_CHARACTERS_WAIT: Duration = Duration::from_millis(1200);

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

/// (id, name, logo, titles)
type KnownStudioRow = (i64, String, Option<String>, i64);

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
    let known: Vec<KnownStudioRow> = sqlx::query_as(
        "SELECT tmdb_id, name, MAX(profile_path), COUNT(*)
           FROM title_person
          WHERE role = 'studio' AND (name LIKE ?1 || '%' OR name LIKE '% ' || ?1 || '%')
          GROUP BY tmdb_id
          ORDER BY COUNT(*) DESC
          LIMIT 8",
    )
    .bind(&pattern)
    .fetch_all(&state.pool)
    .await?;
    for (id, name, logo, _) in known {
        add(id, name, logo, None);
    }
    // A studio by this name exists: "A25" is not what "A24" was looking for.
    if found.iter().any(|s| s.rank.0 < Closeness::Loose) {
        found.retain(|s| s.rank.0 < Closeness::Loose);
    }
    found.sort_by_key(|s| (s.rank, s.order));
    Ok(found.into_iter().take(20).map(|s| s.value).collect())
}

/// The logo of the TMDB company with exactly this name, for a studio AniList
/// names. AniList has no pictures of studios. Only a company whose whole name
/// is the same counts, and a studio that has none is remembered as having none.
pub async fn studio_logo(state: &AppState, name: &str) -> Option<String> {
    const RECHECK_DAYS: i64 = 30;
    let key = anilist::plain(name);
    if key.is_empty() {
        return None;
    }
    let kept: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT logo_path, checked_at FROM studio_match WHERE name_key = ?")
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    let cutoff = (Utc::now() - chrono::Duration::days(RECHECK_DAYS)).to_rfc3339();
    if let Some((logo, _)) = kept.as_ref().filter(|(_, at)| *at > cutoff) {
        return logo.clone();
    }

    let raw = state.tmdb.search_company(name).await.ok()?;
    let same: Vec<&Value> = raw["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["name"].as_str().map(anilist::plain).as_deref() == Some(key.as_str()))
        .filter(|c| models::str_at(c, "logo_path").is_some())
        .collect();
    // AniList is anime, so a Japanese company of that name comes first.
    let logo = same
        .iter()
        .find(|c| c["origin_country"].as_str() == Some("JP"))
        .or(same.first())
        .and_then(|c| models::str_at(c, "logo_path"));
    let stored = sqlx::query(
        "INSERT INTO studio_match (name_key, logo_path, checked_at) VALUES (?, ?, ?)
         ON CONFLICT (name_key) DO UPDATE SET
            logo_path = excluded.logo_path, checked_at = excluded.checked_at",
    )
    .bind(&key)
    .bind(&logo)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await;
    if let Err(e) = stored {
        tracing::debug!(error = %e, "studio logo not remembered");
    }
    logo
}

/// Characters from two places: the casts of every film and show this Frog
/// holds, and AniList's character search for anime, whose casts TMDB mostly
/// lists by voice actor alone.
async fn characters(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let mut out = cast_characters(state, term).await?;
    if term.chars().count() >= 2 {
        match timeout(ANIME_CHARACTERS_WAIT, anime_characters(state, term)).await {
            Ok(Ok(found)) => out.extend(found),
            Ok(Err(e)) => tracing::warn!(error = %e, "AniList character search failed"),
            Err(_) => tracing::debug!("AniList character search skipped: too slow"),
        }
    }
    Ok(out)
}

/// (title id, kind, primary provider, its id, title, poster, character, actor, actor id)
type CastRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<i64>,
);

/// A character as search shows one: the character's own name and the title
/// they are from. Their picture is the title's poster, never the actor's
/// photo: a face belongs to a person, and People is where people are.
async fn cast_characters(state: &AppState, term: &str) -> AppResult<Vec<Value>> {
    let typed = text::words(term);
    if typed.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = typed.join(" ").replace(['\\', '%', '_'], "");
    let rows: Vec<CastRow> = sqlx::query_as(
        "SELECT t.id, t.kind, x.provider, x.external_id, t.name, t.poster_path,
                c.name, c.actor, c.actor_id
           FROM title_character c
           JOIN title t ON t.id = c.title_id
           LEFT JOIN title_external x ON x.title_id = t.id AND x.is_primary = 1
          WHERE t.merged_into IS NULL
            AND (c.search_name LIKE ?1 || '%' OR c.search_name LIKE '% ' || ?1 || '%')
          ORDER BY t.tmdb_vote IS NULL, c.ord, t.tmdb_vote DESC
          LIMIT 60",
    )
    .bind(&pattern)
    .fetch_all(&state.pool)
    .await?;

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
        if let Some(name) = header["name"].as_str().map(str::to_string) {
            header["logo_path"] = json!(studio_logo(&state, &name).await);
        }
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
