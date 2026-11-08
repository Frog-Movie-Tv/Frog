//! A title TMDB does not have, drawn from AniList instead: an anime TMDB
//! lacks, and every manga, which only AniList has.
//!
//! AniList knows most of what TMDB does about an anime: the cover, a banner,
//! the synopsis, genres, a score, episode length, studios, staff, the voice
//! cast, a trailer, where it streams, what it leads on to, and when every
//! episode aired. This reads all of that and writes it in TMDB's shape, so
//! the title page, the library, Home, the calendar and Stats treat such a
//! title exactly like any other. Nothing is made up to fill a gap: a field
//! AniList leaves empty stays empty.
//!
//! Ids: an AniList title, person or studio is its AniList id made negative,
//! so it can never be mistaken for TMDB's (always positive). Pictures are
//! paths under `/anilist/` (AniList's CDN) or `/crunchyroll/` (episode
//! thumbnails AniList links to), which the image route fetches and keeps.

use chrono::{DateTime, FixedOffset, Utc};
use serde_json::{json, Value};

use super::client::Api;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::models::{EpisodeInfo, SearchItem};

const MEDIA_QUERY: &str = "query ($id: Int) {
  Media(id: $id) {
    id type format status episodes chapters volumes duration countryOfOrigin
    title { english romaji native }
    description(asHtml: false)
    startDate { year month day }
    endDate { year month day }
    coverImage { extraLarge color }
    bannerImage
    genres averageScore popularity
    trailer { id site }
    nextAiringEpisode { episode airingAt }
    studios(isMain: true) { nodes { id name } }
    characters(sort: [ROLE, RELEVANCE, ID], perPage: 24) {
      edges { role node { id name { full } }
              voiceActors(language: JAPANESE, sort: [RELEVANCE, ID]) { id name { full } image { large } } }
    }
    staff(sort: [RELEVANCE, ID], perPage: 25) {
      edges { role node { id name { full } image { large } } }
    }
    relations { edges { relationType node {
      id type format title { english romaji } coverImage { extraLarge } bannerImage
      startDate { year month day } averageScore popularity } } }
    recommendations(sort: RATING_DESC, perPage: 18) { nodes { mediaRecommendation {
      id type format title { english romaji } coverImage { extraLarge } bannerImage
      startDate { year month day } averageScore popularity } } }
    externalLinks { id site url type icon color language }
    streamingEpisodes { title thumbnail }
  }
}";

const SCHEDULE_QUERY: &str = "query ($id: Int, $page: Int) {
  Page(page: $page, perPage: 50) {
    pageInfo { hasNextPage }
    airingSchedules(mediaId: $id, sort: EPISODE) { episode airingAt }
  }
}";

/// Schedules longer than this many pages are not read (2,500 episodes).
const MAX_SCHEDULE_PAGES: i64 = 50;

/// Anime air on Japanese dates; TMDB dates them the same way.
fn japan() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("a valid offset")
}

/// A picture on AniList's CDN as an image path Frog serves.
pub fn image_path(url: Option<&str>) -> Option<String> {
    url?.strip_prefix("https://s4.anilist.co/file/anilistcdn/")
        .map(|p| format!("/anilist/{p}"))
}

/// An episode thumbnail, kept only from a host the image route trusts.
fn thumbnail_path(url: Option<&str>) -> Option<String> {
    let url = url?;
    image_path(Some(url)).or_else(|| {
        url.strip_prefix("https://img1.ak.crunchyroll.com/")
            .map(|p| format!("/crunchyroll/{p}"))
    })
}

fn name_of(media: &Value) -> Option<String> {
    media["title"]["english"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| media["title"]["romaji"].as_str())
        .map(str::to_string)
}

fn date_of(d: &Value) -> Option<String> {
    super::client::full_date(d)
}

/// Where it streams, or for a manga where it is read, linked straight to the
/// page AniList knows: one link a service. AniList lists a service once for
/// each language it has the title in; the English one is kept, else the
/// first. It does not say in which countries, so they are listed for every
/// one.
fn where_to(links: &Value) -> Vec<Value> {
    let mut kept: Vec<&Value> = Vec::new();
    for link in links.as_array().into_iter().flatten() {
        if link["type"].as_str() != Some("STREAMING") || link["site"].as_str().is_none() {
            continue;
        }
        let english = matches!(link["language"].as_str(), None | Some("English"));
        match kept.iter().position(|k| k["site"] == link["site"]) {
            Some(at)
                if english
                    && kept[at]["language"]
                        .as_str()
                        .is_some_and(|l| l != "English") =>
            {
                kept[at] = link;
            }
            Some(_) => {}
            None => kept.push(link),
        }
    }
    kept.into_iter()
        .filter_map(|l| {
            Some(json!({
                "provider_id": -l["id"].as_i64()?,
                "provider_name": l["site"].as_str()?,
                "logo_path": image_path(l["icon"].as_str()),
                "url": l["url"].as_str()?,
                "color": l["color"].as_str(),
            }))
        })
        .collect()
}

/// AniList writes its synopses with a little HTML and a source line; TMDB's
/// are plain paragraphs.
pub fn plain_text(html: &str) -> String {
    let mut text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    // Whatever tags are left (<i>, <b>, <a …>) go; their text stays.
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for c in text.chars() {
        match c {
            '<' => inside = true,
            '>' if inside => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    text = out
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&mdash;", "—")
        .replace("&amp;", "&");
    text = unmark(&text);
    let paragraphs: Vec<&str> = text
        .split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        // "(Source: Crunchyroll)" and "Note: …" are about the text, not the show.
        .filter(|l| {
            let lower = l.to_ascii_lowercase();
            !(lower.starts_with("(source:") || lower.starts_with("note:"))
        })
        .collect();
    paragraphs.join("\n\n")
}

/// AniList's Markdown, read as plain text: `[label](address)` keeps its label,
/// emphasis loses its marks, a `~!spoiler!~` goes entirely, and a line that
/// was nothing but links (a row of "Website", "Twitter") goes too.
fn unmark(text: &str) -> String {
    // Spoilers first: they can run across lines.
    let mut rest = text;
    let mut open = String::with_capacity(text.len());
    while let Some(start) = rest.find("~!") {
        open.push_str(&rest[..start]);
        match rest[start..].find("!~") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    open.push_str(rest);

    let mut lines = Vec::new();
    for line in open.split('\n') {
        let mut kept = String::with_capacity(line.len());
        let mut only_links = true;
        let mut links = 0;
        let mut i = 0;
        let bytes = line.as_bytes();
        while i < line.len() {
            if bytes[i] == b'[' {
                if let Some(close) = line[i..].find("](") {
                    if let Some(end) = line[i + close..].find(')') {
                        kept.push_str(&line[i + 1..i + close]);
                        links += 1;
                        i += close + end + 1;
                        continue;
                    }
                }
            }
            let c = line[i..].chars().next().unwrap_or(' ');
            if !(c.is_whitespace() || matches!(c, '|' | '/' | '-' | '·' | '•' | ',')) {
                only_links = false;
            }
            kept.push(c);
            i += c.len_utf8();
        }
        if links > 0 && only_links {
            continue;
        }
        let kept = kept.replace("**", "").replace("__", "").replace("~~~", "");
        lines.push(kept.trim_start_matches('#').trim().to_string());
    }
    lines.join("\n")
}

/// TMDB's genre names, where AniList's differ, so filters and Stats group
/// AniList titles with everything else. Every anime is Animation on TMDB; a
/// manga keeps AniList's own words.
fn genres(kind: &str, list: &Value) -> Vec<Value> {
    let mut names: Vec<String> = if kind == "manga" {
        Vec::new()
    } else {
        vec!["Animation".into()]
    };
    for g in list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let mapped = match (kind, g) {
            ("tv", "Action" | "Adventure") => "Action & Adventure",
            ("tv", "Sci-Fi" | "Fantasy") => "Sci-Fi & Fantasy",
            ("movie", "Sci-Fi") => "Science Fiction",
            (_, other) => other,
        };
        if !names.iter().any(|n| n == mapped) {
            names.push(mapped.to_string());
        }
    }
    names
        .into_iter()
        .enumerate()
        .map(|(i, name)| json!({ "id": i, "name": name }))
        .collect()
}

/// How Frog files an AniList media: a manga, a film, or a show. A media with
/// no type is an anime, which is all AniList was read for before manga.
pub fn kind_of(media: &Value) -> &'static str {
    match (media["type"].as_str(), media["format"].as_str()) {
        (Some("MANGA"), _) => "manga",
        (_, Some("MOVIE")) => "movie",
        _ => "tv",
    }
}

/// A related or recommended work as a TMDB list item: the TMDB title when
/// the anime lists say which, else the AniList title Frog can show. A manga
/// is always AniList's.
async fn as_item(state: &AppState, media: &Value) -> AppResult<Option<Value>> {
    if !matches!(media["type"].as_str(), Some("ANIME" | "MANGA")) {
        return Ok(None);
    }
    let (Some(id), Some(name)) = (media["id"].as_i64(), name_of(media)) else {
        return Ok(None);
    };
    let mapped: Option<(String, i64)> = if kind_of(media) == "manga" {
        None
    } else {
        sqlx::query_as(
            "SELECT tmdb_kind, tmdb_id FROM anime_map
              WHERE anilist_id = ? AND tmdb_kind IN ('movie', 'tv') AND tmdb_id IS NOT NULL",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
    };
    let (kind, item_id, source) = match mapped {
        Some((kind, tmdb_id)) => (kind, tmdb_id, "tmdb"),
        None => (kind_of(media).to_string(), id, "anilist"),
    };
    let date = date_of(&media["startDate"])
        .or_else(|| media["startDate"]["year"].as_i64().map(|y| format!("{y}")));
    let mut item = json!({
        "id": item_id,
        "media_type": kind,
        "source": source,
        "poster_path": image_path(media["coverImage"]["extraLarge"].as_str()),
        "backdrop_path": image_path(media["bannerImage"].as_str()),
        "vote_average": media["averageScore"].as_f64().map(|s| s / 10.0),
        "popularity": media["popularity"].as_f64(),
    });
    if kind == "movie" {
        item["title"] = json!(name);
        item["release_date"] = json!(date);
    } else {
        item["name"] = json!(name);
        item["first_air_date"] = json!(date);
    }
    Ok(Some(item))
}

/// An anime or manga by its AniList id as a list item: for an anime, the
/// TMDB title when the anime lists know it, else AniList's own. For an
/// import that names it that way.
pub async fn search_item(state: &AppState, media_id: i64) -> AppResult<SearchItem> {
    let mut media = fetch(state, media_id).await?;
    // An answer kept from before the query asked for the type is an anime.
    if media["type"].is_null() {
        media["type"] = json!("ANIME");
    }
    let kind = kind_of(&media);
    as_item(state, &media)
        .await?
        .and_then(|v| SearchItem::from_value(&v, Some(kind)))
        .ok_or_else(|| AppError::NotFound("AniList has no such title".into()))
}

/// One anime or manga from AniList, cached like TMDB's answers: half a day
/// while it is airing or yet to air, a week once it has finished.
pub async fn fetch(state: &AppState, media_id: i64) -> AppResult<Value> {
    // 2: the answer names its type and its links' colours and languages.
    let key = format!("anilist:media:2:{media_id}");
    let cached: Option<(String,)> =
        sqlx::query_as("SELECT body FROM tmdb_cache WHERE cache_key = ? AND expires_at > ?")
            .bind(&key)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&state.pool)
            .await?;
    if let Some(body) = cached.and_then(|(b,)| serde_json::from_str::<Value>(&b).ok()) {
        return Ok(body);
    }

    let api = Api::new(state)?;
    // Public data: no account's token is needed or used.
    let data = api
        .graphql("", MEDIA_QUERY, json!({ "id": media_id }))
        .await?;
    let mut media = data["Media"].clone();
    if media["id"].as_i64() != Some(media_id) {
        return Err(AppError::NotFound("AniList has no such title".into()));
    }
    let mut schedule: Vec<Value> = Vec::new();
    // A manga has chapters, not an airing schedule.
    let pages = if kind_of(&media) == "manga" {
        0
    } else {
        MAX_SCHEDULE_PAGES
    };
    for page in 1..=pages {
        let data = api
            .graphql("", SCHEDULE_QUERY, json!({ "id": media_id, "page": page }))
            .await?;
        schedule.extend(
            data["Page"]["airingSchedules"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        if data["Page"]["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break;
        }
    }
    media["schedule"] = json!(schedule);

    let finished = media["status"].as_str() == Some("FINISHED");
    let now = Utc::now();
    let keep = if finished {
        chrono::Duration::days(7)
    } else {
        chrono::Duration::hours(12)
    };
    sqlx::query(
        "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (cache_key) DO UPDATE SET
            body = excluded.body, fetched_at = excluded.fetched_at,
            expires_at = excluded.expires_at",
    )
    .bind(&key)
    .bind(media.to_string())
    .bind(now.to_rfc3339())
    .bind((now + keep).to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(media)
}

/// The episodes AniList can vouch for, numbered as AniList numbers them, as
/// season 1. Dates come from its airing schedule; without one, only the first
/// episode (the start date) and, once it has finished, the last (the end
/// date) are dated, and those between are out because a later one is. No
/// episode is listed that AniList does not say exists.
pub fn episodes(media: &Value) -> Vec<EpisodeInfo> {
    let mut dated: std::collections::BTreeMap<i64, Option<String>> = Default::default();
    for s in media["schedule"].as_array().into_iter().flatten() {
        let (Some(n), Some(at)) = (s["episode"].as_i64(), s["airingAt"].as_i64()) else {
            continue;
        };
        let date = DateTime::from_timestamp(at, 0)
            .map(|t| t.with_timezone(&japan()).format("%Y-%m-%d").to_string());
        dated.insert(n, date);
    }
    let count = media["episodes"].as_i64().filter(|n| *n > 0);
    let last = count
        .or_else(|| dated.keys().max().copied())
        .or_else(|| {
            media["nextAiringEpisode"]["episode"]
                .as_i64()
                .map(|n| n - 1)
                .filter(|n| *n > 0)
        })
        .unwrap_or(0);
    for n in 1..=last {
        dated.entry(n).or_insert(None);
    }
    if let Some(first) = dated.get_mut(&1) {
        if first.is_none() {
            *first = date_of(&media["startDate"]);
        }
    }
    if media["status"].as_str() == Some("FINISHED") {
        if let (Some(n), Some(end)) = (count, date_of(&media["endDate"])) {
            if let Some(slot) = dated.get_mut(&n) {
                if slot.is_none() {
                    *slot = Some(end);
                }
            }
        }
    }

    // Episode names and pictures from the streaming sites AniList links,
    // matched by the number in their title, never by their order.
    let mut named: std::collections::HashMap<i64, (Option<String>, Option<String>)> =
        Default::default();
    for e in media["streamingEpisodes"].as_array().into_iter().flatten() {
        let Some(title) = e["title"].as_str() else {
            continue;
        };
        let Some(rest) = title.strip_prefix("Episode ") else {
            continue;
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(n) = digits.parse::<i64>() else {
            continue;
        };
        let name = rest[digits.len()..]
            .trim_start_matches([' ', '-', ':', '–'])
            .trim();
        named.entry(n).or_insert((
            (!name.is_empty()).then(|| name.to_string()),
            thumbnail_path(e["thumbnail"].as_str()),
        ));
    }

    // A site numbering past the end of the show cuts it differently (Angels
    // of Death is 12 episodes on AniList and 16 on Crunchyroll), so none of
    // its names can be trusted to line up.
    if named.keys().any(|n| *n > last) {
        named.clear();
    }

    let runtime = media["duration"].as_i64();
    dated
        .into_iter()
        .map(|(n, air_date)| {
            let (name, still) = named.remove(&n).unwrap_or((None, None));
            EpisodeInfo {
                season_number: 1,
                episode_number: n,
                tmdb_episode_id: None,
                name,
                overview: None,
                still_path: still,
                air_date,
                runtime,
                tmdb_vote: None,
            }
        })
        .collect()
}

/// The anime or manga in the shape of a TMDB detail answer, with `kind` the
/// way Frog files it: a film, a show of one season, or a manga, whose
/// chapters and volumes stand where a show's episodes and seasons do.
pub async fn as_tmdb(state: &AppState, kind: &str, media: &Value) -> AppResult<Value> {
    let media_id = media["id"].as_i64().unwrap_or_default();
    let name = name_of(media).unwrap_or_else(|| "Untitled".into());
    let original = media["title"]["native"]
        .as_str()
        .or_else(|| media["title"]["romaji"].as_str());
    let start = date_of(&media["startDate"]);
    let end = date_of(&media["endDate"]);
    let status = super::local::local_status(media["status"].as_str());
    let duration = media["duration"].as_i64();
    let eps = episodes(media);

    // The voice cast, as TMDB lists actors: the person, and who they voice.
    let mut cast = Vec::new();
    for (order, edge) in media["characters"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(actor) = edge["voiceActors"].as_array().and_then(|a| a.first()) else {
            continue;
        };
        let (Some(id), Some(person)) = (actor["id"].as_i64(), actor["name"]["full"].as_str())
        else {
            continue;
        };
        cast.push(json!({
            "id": -id,
            "name": person,
            "character": edge["node"]["name"]["full"].as_str(),
            "profile_path": image_path(actor["image"]["large"].as_str()),
            "order": order,
        }));
    }

    // Staff, in TMDB's words for the jobs Frog shows.
    let mut crew = Vec::new();
    let mut created_by = Vec::new();
    for edge in media["staff"]["edges"].as_array().into_iter().flatten() {
        let (Some(id), Some(person)) = (
            edge["node"]["id"].as_i64(),
            edge["node"]["name"]["full"].as_str(),
        ) else {
            continue;
        };
        let role = edge["role"].as_str().unwrap_or("");
        let picture = image_path(edge["node"]["image"]["large"].as_str());
        let base = role.split(" (").next().unwrap_or(role).trim();
        let author = kind == "manga" && matches!(base, "Story & Art" | "Story" | "Art");
        if author || base == "Original Creator" || base == "Original Story" {
            created_by.push(json!({ "id": -id, "name": person, "profile_path": picture }));
            continue;
        }
        let job = match base {
            "Director" => "Director",
            "Series Composition" | "Script" | "Screenplay" => "Writer",
            _ => continue,
        };
        crew.push(json!({ "id": -id, "name": person, "job": job, "profile_path": picture }));
    }

    let mut studios: Vec<Value> = Vec::new();
    for s in media["studios"]["nodes"].as_array().into_iter().flatten() {
        let (Some(id), Some(name)) = (s["id"].as_i64(), s["name"].as_str()) else {
            continue;
        };
        let logo = crate::routes::browse::studio_logo(state, name).await;
        studios.push(json!({ "id": -id, "name": name, "logo_path": logo }));
    }

    let videos: Vec<Value> = match (
        media["trailer"]["site"].as_str(),
        media["trailer"]["id"].as_str(),
    ) {
        (Some("youtube"), Some(key)) => vec![json!({
            "key": key, "site": "YouTube", "type": "Trailer", "name": "Trailer", "official": true
        })],
        _ => Vec::new(),
    };

    let streaming = where_to(&media["externalLinks"]);

    let mut recommendations = Vec::new();
    for node in media["recommendations"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(item) = as_item(state, &node["mediaRecommendation"]).await? {
            recommendations.push(item);
        }
    }
    // Sequels, prequels, side stories, films: everything in its franchise.
    let mut related = Vec::new();
    for edge in media["relations"]["edges"].as_array().into_iter().flatten() {
        if matches!(edge["relationType"].as_str(), Some("CHARACTER" | "OTHER")) {
            continue;
        }
        if let Some(item) = as_item(state, &edge["node"]).await? {
            related.push(item);
        }
    }

    let poster = image_path(media["coverImage"]["extraLarge"].as_str());
    let mut raw = json!({
        "id": media_id,
        "source": "anilist",
        "original_name": original,
        "overview": media["description"].as_str().map(plain_text),
        "poster_path": poster,
        "backdrop_path": image_path(media["bannerImage"].as_str()),
        "status": status,
        "genres": genres(kind, &media["genres"]),
        "vote_average": media["averageScore"].as_f64().map(|s| s / 10.0),
        "credits": { "cast": cast, "crew": crew },
        "created_by": created_by,
        "production_companies": studios,
        "videos": { "results": videos },
        "watch/providers": { "results": { state.tmdb.region(): { "flatrate": streaming } } },
        "recommendations": { "results": recommendations },
        "related": { "results": related },
    });
    if kind == "movie" {
        raw["title"] = json!(name);
        raw["original_title"] = json!(original);
        raw["release_date"] = json!(start);
        raw["runtime"] = json!(duration);
    } else if kind == "manga" {
        raw["name"] = json!(name);
        raw["first_air_date"] = json!(start);
        raw["last_air_date"] = json!(end);
        raw["number_of_episodes"] = json!(media["chapters"].as_i64().filter(|n| *n > 0));
        raw["number_of_seasons"] = json!(media["volumes"].as_i64().filter(|n| *n > 0));
        raw["seasons"] = json!([]);
    } else {
        raw["name"] = json!(name);
        raw["first_air_date"] = json!(start);
        raw["last_air_date"] = json!(end.or_else(|| eps
            .iter()
            .rev()
            .filter_map(|e| e.air_date.clone())
            .find(|d| d.as_str() <= Utc::now().format("%Y-%m-%d").to_string().as_str())));
        raw["episode_run_time"] = json!(duration.into_iter().collect::<Vec<_>>());
        raw["number_of_episodes"] = json!(media["episodes"].as_i64());
        raw["number_of_seasons"] = json!(1);
        raw["seasons"] = json!(if eps.is_empty() {
            Vec::new()
        } else {
            vec![json!({
                "season_number": 1, "name": "Season 1", "episode_count": eps.len(),
                "air_date": start, "poster_path": poster,
            })]
        });
        if let (Some(n), Some(at)) = (
            media["nextAiringEpisode"]["episode"].as_i64(),
            media["nextAiringEpisode"]["airingAt"].as_i64(),
        ) {
            raw["next_episode_to_air"] = json!({
                "season_number": 1,
                "episode_number": n,
                "air_date": DateTime::from_timestamp(at, 0)
                    .map(|t| t.with_timezone(&japan()).format("%Y-%m-%d").to_string()),
            });
        }
    }
    Ok(raw)
}

// --- people and studios ---------------------------------------------------------

const WORK: &str = "id type format title { english romaji } coverImage { extraLarge } bannerImage
      startDate { year month day } averageScore popularity";

/// One AniList answer, kept for a day like a TMDB one.
async fn cached(state: &AppState, key: &str, query: &str, vars: Value) -> AppResult<Value> {
    let cached: Option<(String,)> =
        sqlx::query_as("SELECT body FROM tmdb_cache WHERE cache_key = ? AND expires_at > ?")
            .bind(key)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&state.pool)
            .await?;
    if let Some(body) = cached.and_then(|(b,)| serde_json::from_str::<Value>(&b).ok()) {
        return Ok(body);
    }
    let data = Api::new(state)?.graphql("", query, vars).await?;
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (cache_key) DO UPDATE SET
            body = excluded.body, fetched_at = excluded.fetched_at,
            expires_at = excluded.expires_at",
    )
    .bind(key)
    .bind(data.to_string())
    .bind(now.to_rfc3339())
    .bind((now + chrono::Duration::days(1)).to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(data)
}

/// A work as the list item a person or studio page shows.
async fn work(state: &AppState, media: &Value) -> AppResult<Option<SearchItem>> {
    let Some(item) = as_item(state, media).await? else {
        return Ok(None);
    };
    Ok(SearchItem::from_value(&item, None))
}

/// Manga by name, best match first, as list items. AniList is the only
/// place Frog finds manga; adult titles stay out, as TMDB's do, and so do
/// light novels, which AniList files with manga. AniList's own search also
/// answers by synonym and description, so what it gives is ranked here
/// (`rank_manga`) and only what a name of it answers is kept.
pub async fn search_manga(state: &AppState, term: &str) -> AppResult<Vec<SearchItem>> {
    let query = "query ($search: String) {
  Page(perPage: 30) {
    media(search: $search, type: MANGA, format_in: [MANGA, ONE_SHOT], isAdult: false,
          sort: [SEARCH_MATCH, POPULARITY_DESC]) {
      id type format title { english romaji native } synonyms
      coverImage { extraLarge } bannerImage startDate { year month day }
      averageScore popularity
    }
  }
}";
    // 2: novels left out, and every name read for ranking.
    let key = format!("anilist:manga-search:2:{}", term.trim().to_lowercase());
    let data = cached(state, &key, query, json!({ "search": term.trim() })).await?;
    let found: Vec<Value> = data["Page"]["media"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for media in rank_manga(found, &crate::text::words(term)) {
        if let Some(item) = work(state, &media).await? {
            out.push(item);
        }
    }
    Ok(out)
}

/// A manga by its MyAnimeList id, as AniList knows it, for an import from
/// MyAnimeList's manga list.
pub async fn manga_by_mal(state: &AppState, mal_id: i64) -> AppResult<SearchItem> {
    let query = format!("query ($id: Int) {{ Media(idMal: $id, type: MANGA) {{ {WORK} }} }}");
    let data = cached(
        state,
        &format!("anilist:manga-mal:1:{mal_id}"),
        &query,
        json!({ "id": mal_id }),
    )
    .await?;
    work(state, &data["Media"])
        .await?
        .ok_or_else(|| AppError::NotFound("AniList has no such manga".into()))
}

/// AniList's answers to a manga search, closest first, and only those a name
/// of theirs answers. Its English, romaji and native titles come first; a
/// manga found by a synonym alone comes after all of them. Within the same
/// closeness, AniList's own order (best match, then most popular) stands. A
/// synonym only counts when it is exactly what was typed.
pub fn rank_manga(found: Vec<Value>, typed: &[String]) -> Vec<Value> {
    use crate::text::{closeness, Closeness};
    if typed.is_empty() {
        return Vec::new();
    }
    let best = |names: Vec<&str>| {
        names
            .into_iter()
            .map(|n| closeness(n, typed))
            .filter(|c| *c != Closeness::Loose)
            .min()
    };
    let mut ranked: Vec<(u8, usize, Value)> = found
        .into_iter()
        .enumerate()
        .filter_map(|(i, media)| {
            let titles: Vec<&str> = ["english", "romaji", "native"]
                .iter()
                .filter_map(|k| media["title"][*k].as_str())
                .collect();
            let synonyms: Vec<&str> = media["synonyms"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            // A synonym is often a loose translation: it counts only as
            // exactly what was typed.
            let rank = match (best(titles), best(synonyms)) {
                (Some(Closeness::Exact), _) => 0,
                (Some(Closeness::Starts), _) => 1,
                (Some(_), _) => 2,
                (None, Some(Closeness::Exact)) => 3,
                _ => return None,
            };
            Some((rank, i, media))
        })
        .collect();
    ranked.sort_by_key(|(rank, i, _)| (*rank, *i));
    ranked.into_iter().map(|(_, _, media)| media).collect()
}

/// One credit on a person page: the work, cast or crew, and the part.
pub struct Credit {
    pub item: SearchItem,
    pub department: &'static str,
    pub role: Option<String>,
}

/// A person AniList knows (a voice actor, a director), in the fields the
/// person page shows for anyone from TMDB.
pub async fn person(state: &AppState, staff_id: i64) -> AppResult<(Value, Vec<Credit>)> {
    let query = format!(
        "query ($id: Int) {{ Staff(id: $id) {{
          id name {{ full }} description(asHtml: false) image {{ large }}
          dateOfBirth {{ year month day }} dateOfDeath {{ year month day }}
          homeTown primaryOccupations
          characterMedia(sort: START_DATE_DESC, perPage: 50) {{
            edges {{ characters {{ name {{ full }} }} node {{ {WORK} }} }} }}
          staffMedia(sort: START_DATE_DESC, perPage: 50) {{
            edges {{ staffRole node {{ {WORK} }} }} }}
        }} }}"
    );
    let data = cached(
        state,
        // 2: their manga too, not only their anime.
        &format!("anilist:staff:2:{staff_id}"),
        &query,
        json!({ "id": staff_id }),
    )
    .await?;
    let staff = &data["Staff"];
    if staff["id"].as_i64() != Some(staff_id) {
        return Err(AppError::NotFound("no such person".into()));
    }
    let mut credits = Vec::new();
    for edge in staff["characterMedia"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(item) = work(state, &edge["node"]).await? {
            let parts: Vec<&str> = edge["characters"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["name"]["full"].as_str())
                .collect();
            credits.push(Credit {
                item,
                department: "cast",
                role: (!parts.is_empty()).then(|| parts.join(" / ")),
            });
        }
    }
    for edge in staff["staffMedia"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(item) = work(state, &edge["node"]).await? {
            credits.push(Credit {
                item,
                department: "crew",
                role: edge["staffRole"].as_str().map(str::to_string),
            });
        }
    }
    let occupation = staff["primaryOccupations"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .map(|o| if o == "Voice Actor" { "Acting" } else { o });
    let header = json!({
        "id": -staff_id,
        "name": staff["name"]["full"].as_str(),
        "biography": staff["description"].as_str().map(plain_text).filter(|b| !b.is_empty()),
        "profile_path": image_path(staff["image"]["large"].as_str()),
        "birthday": date_of(&staff["dateOfBirth"]),
        "deathday": date_of(&staff["dateOfDeath"]),
        "place_of_birth": staff["homeTown"].as_str(),
        "known_for_department": occupation,
    });
    Ok((header, credits))
}

/// A studio AniList knows, with its work, most popular first, a page at a
/// time. The last value says whether there is another page.
pub async fn studio(
    state: &AppState,
    studio_id: i64,
    page: u32,
) -> AppResult<(Value, Vec<SearchItem>, bool)> {
    let query = format!(
        "query ($id: Int, $page: Int) {{ Studio(id: $id) {{ id name siteUrl
          media(sort: POPULARITY_DESC, isMain: true, page: $page, perPage: 40) {{
            pageInfo {{ hasNextPage }} nodes {{ {WORK} }} }}
        }} }}"
    );
    let data = cached(
        state,
        &format!("anilist:studio:2:{studio_id}:{page}"),
        &query,
        json!({ "id": studio_id, "page": page }),
    )
    .await?;
    let studio = &data["Studio"];
    if studio["id"].as_i64() != Some(studio_id) {
        return Err(AppError::NotFound("no such studio".into()));
    }
    let mut items = Vec::new();
    for node in studio["media"]["nodes"].as_array().into_iter().flatten() {
        if let Some(item) = work(state, node).await? {
            items.push(item);
        }
    }
    let header = json!({
        "id": -studio_id,
        "name": studio["name"].as_str(),
        "logo_path": null,
        "description": null,
        "headquarters": null,
        "country": null,
        "homepage": studio["siteUrl"].as_str(),
    });
    let more = studio["media"]["pageInfo"]["hasNextPage"].as_bool() == Some(true);
    Ok((header, items, more))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synopses_lose_their_markup_and_source_line() {
        let html = "A boy &amp; his <i>dog</i>.<br><br>They &quot;run&quot;.<br>\n<br>(Source: Crunchyroll)";
        assert_eq!(plain_text(html), "A boy & his dog.\n\nThey \"run\".");
    }

    #[test]
    fn biographies_lose_their_markdown_links_and_spoilers() {
        let bio = "[Website](https://a.example/) [Blog](https://b.example/)\n\n\
                   **Height:** 156 cm\n\nShe voices [Rachel](https://anilist.co/character/1). \
                   ~!She is the killer.!~ Fans love her.";
        assert_eq!(
            plain_text(bio),
            "Height: 156 cm\n\nShe voices Rachel.  Fans love her."
        );
    }

    #[test]
    fn episodes_come_from_the_schedule_and_never_beyond_it() {
        let media = json!({
            "status": "RELEASING", "episodes": null, "duration": 24,
            "startDate": { "year": 2026, "month": 10, "day": 1 },
            "nextAiringEpisode": { "episode": 3, "airingAt": 1_793_000_000 },
            "schedule": [
                { "episode": 1, "airingAt": 1_791_790_000 },
                { "episode": 2, "airingAt": 1_792_395_000 },
                { "episode": 3, "airingAt": 1_793_000_000 }
            ],
            "streamingEpisodes": [
                { "title": "Episode 2 - The Pond", "thumbnail": "https://img1.ak.crunchyroll.com/i/a.jpg" },
                { "title": "Pond special", "thumbnail": "https://example.com/b.jpg" }
            ]
        });
        let eps = episodes(&media);
        assert_eq!(eps.len(), 3, "only what AniList lists");
        assert!(eps.iter().all(|e| e.air_date.is_some()));
        assert_eq!(eps[1].name.as_deref(), Some("The Pond"));
        assert_eq!(eps[1].still_path.as_deref(), Some("/crunchyroll/i/a.jpg"));
        assert_eq!(eps[0].name, None);
    }

    #[test]
    fn a_finished_show_without_a_schedule_dates_only_what_is_known() {
        let media = json!({
            "status": "FINISHED", "episodes": 4,
            "startDate": { "year": 2018, "month": 7, "day": 6 },
            "endDate": { "year": 2018, "month": 7, "day": 27 }
        });
        let dates: Vec<Option<String>> = episodes(&media).into_iter().map(|e| e.air_date).collect();
        assert_eq!(
            dates,
            vec![
                Some("2018-07-06".into()),
                None,
                None,
                Some("2018-07-27".into())
            ]
        );
        // A site that numbers past the end lends no names at all.
        let split = json!({
            "status": "FINISHED", "episodes": 2,
            "streamingEpisodes": [
                { "title": "Episode 1 - One", "thumbnail": null },
                { "title": "Episode 3 - Three", "thumbnail": null }
            ]
        });
        assert!(episodes(&split).iter().all(|e| e.name.is_none()));
        // Not out yet and nothing known: no episodes at all.
        assert!(episodes(&json!({ "status": "NOT_YET_RELEASED", "episodes": null })).is_empty());
    }

    #[test]
    fn manga_search_keeps_what_a_name_answers_closest_first() {
        let m = |id: i64, english: Option<&str>, romaji: &str, synonyms: &[&str]| {
            json!({ "id": id, "title": { "english": english, "romaji": romaji },
                    "synonyms": synonyms })
        };
        let found = vec![
            m(1, Some("Tough Love at the Office"), "Shanai Jijou", &[]),
            m(2, None, "Tong Lei", &["Monster"]),
            m(
                3,
                Some("Monster Musume"),
                "Monster Musume no Iru Nichijou",
                &[],
            ),
            m(4, Some("Monster"), "Monster", &[]),
            m(
                5,
                Some("A Business Proposal"),
                "Sanaegeul",
                &["The Office Proposal"],
            ),
        ];
        let ids = |list: Vec<Value>| {
            list.iter()
                .map(|v| v["id"].as_i64().unwrap())
                .collect::<Vec<_>>()
        };
        let typed = crate::text::words("monster");
        assert_eq!(ids(rank_manga(found.clone(), &typed)), vec![4, 3, 2]);
        // A name searched for a film finds no manga that does not carry it.
        let typed = crate::text::words("interstellar");
        assert!(rank_manga(found.clone(), &typed).is_empty());
        // A synonym that only starts with it is a loose translation.
        let typed = crate::text::words("the office");
        assert_eq!(ids(rank_manga(found, &typed)), vec![1]);
    }

    #[test]
    fn a_service_listed_in_many_languages_is_one_link() {
        let links = json!([
            { "id": 1, "site": "MANGA Plus", "type": "STREAMING", "language": "Spanish",
              "url": "https://mangaplus.example/es", "icon": null, "color": "#E40C1D" },
            { "id": 2, "site": "MANGA Plus", "type": "STREAMING", "language": "English",
              "url": "https://mangaplus.example/en", "icon": null, "color": "#E40C1D" },
            { "id": 3, "site": "MANGA Plus", "type": "STREAMING", "language": "Thai",
              "url": "https://mangaplus.example/th" },
            { "id": 4, "site": "Twitter", "type": "SOCIAL", "url": "https://x.example" },
            { "id": 5, "site": "Shonen Jump", "type": "STREAMING", "language": null,
              "url": "https://sj.example" }
        ]);
        let kept = where_to(&links);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(kept[0]["url"], json!("https://mangaplus.example/en"));
        assert_eq!(kept[0]["color"], json!("#E40C1D"));
        assert_eq!(kept[1]["provider_name"], json!("Shonen Jump"));
    }

    #[test]
    fn genres_read_as_tmdb_names_them() {
        let names: Vec<String> = genres("tv", &json!(["Action", "Adventure", "Drama", "Sci-Fi"]))
            .into_iter()
            .map(|g| g["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "Animation",
                "Action & Adventure",
                "Drama",
                "Sci-Fi & Fantasy"
            ]
        );
    }

    #[test]
    fn only_trusted_image_hosts_become_paths() {
        assert_eq!(
            image_path(Some(
                "https://s4.anilist.co/file/anilistcdn/media/anime/cover/large/bx1.jpg"
            ))
            .as_deref(),
            Some("/anilist/media/anime/cover/large/bx1.jpg")
        );
        assert_eq!(thumbnail_path(Some("https://evil.example/x.jpg")), None);
    }
}
