//! A title TMDB does not have, drawn from AniList instead.
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
  Media(id: $id, type: ANIME) {
    id format status episodes duration countryOfOrigin
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
    externalLinks { id site url type icon }
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
/// AniList titles with everything else. Every anime is Animation on TMDB.
fn genres(kind: &str, list: &Value) -> Vec<Value> {
    let mut names: Vec<String> = vec!["Animation".into()];
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

/// A related or recommended work as a TMDB list item: the TMDB title when
/// the anime lists say which, else the AniList title Frog can show.
async fn as_item(state: &AppState, media: &Value) -> AppResult<Option<Value>> {
    if media["type"].as_str() != Some("ANIME") {
        return Ok(None);
    }
    let (Some(id), Some(name)) = (media["id"].as_i64(), name_of(media)) else {
        return Ok(None);
    };
    let mapped: Option<(String, i64)> = sqlx::query_as(
        "SELECT tmdb_kind, tmdb_id FROM anime_map
          WHERE anilist_id = ? AND tmdb_kind IN ('movie', 'tv') AND tmdb_id IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let (kind, item_id, source) = match mapped {
        Some((kind, tmdb_id)) => (kind, tmdb_id, "tmdb"),
        None => (
            if media["format"].as_str() == Some("MOVIE") {
                "movie".to_string()
            } else {
                "tv".to_string()
            },
            id,
            "anilist",
        ),
    };
    let date = date_of(&media["startDate"])
        .or_else(|| media["startDate"]["year"].as_i64().map(|y| format!("{y}")));
    let mut item = json!({
        "id": item_id,
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

/// An anime by its AniList id as a list item: the TMDB title when the anime
/// lists know it, else AniList's own. For an import that names it that way.
pub async fn search_item(state: &AppState, media_id: i64) -> AppResult<SearchItem> {
    let mut media = fetch(state, media_id).await?;
    // The detail query does not ask for the type; it only reads anime.
    media["type"] = json!("ANIME");
    let kind = if media["format"].as_str() == Some("MOVIE") {
        "movie"
    } else {
        "tv"
    };
    as_item(state, &media)
        .await?
        .and_then(|v| SearchItem::from_value(&v, Some(kind)))
        .ok_or_else(|| AppError::NotFound("AniList has no such anime".into()))
}

/// One anime from AniList, cached like TMDB's answers: half a day while it
/// is airing or yet to air, a week once it has finished.
pub async fn fetch(state: &AppState, media_id: i64) -> AppResult<Value> {
    let key = format!("anilist:media:1:{media_id}");
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
        return Err(AppError::NotFound("AniList has no such anime".into()));
    }
    let mut schedule: Vec<Value> = Vec::new();
    for page in 1..=MAX_SCHEDULE_PAGES {
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

/// The anime in the shape of a TMDB detail answer, with `kind` the way Frog
/// files it (a film, or a show of one season).
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
        if base == "Original Creator" || base == "Original Story" {
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

    let studios: Vec<Value> = media["studios"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            Some(
                json!({ "id": -s["id"].as_i64()?, "name": s["name"].as_str()?, "logo_path": null }),
            )
        })
        .collect();

    let videos: Vec<Value> = match (
        media["trailer"]["site"].as_str(),
        media["trailer"]["id"].as_str(),
    ) {
        (Some("youtube"), Some(key)) => vec![json!({
            "key": key, "site": "YouTube", "type": "Trailer", "name": "Trailer", "official": true
        })],
        _ => Vec::new(),
    };

    // Where it streams, linked straight to the page AniList knows. AniList
    // does not say in which countries, so they are listed for every one.
    let streaming: Vec<Value> = media["externalLinks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["type"].as_str() == Some("STREAMING"))
        .filter_map(|l| {
            Some(json!({
                "provider_id": -l["id"].as_i64()?,
                "provider_name": l["site"].as_str()?,
                "logo_path": image_path(l["icon"].as_str()),
                "url": l["url"].as_str()?,
            }))
        })
        .collect();

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
    let kind = if item.get("title").is_some() {
        "movie"
    } else {
        "tv"
    };
    Ok(SearchItem::from_value(&item, Some(kind)))
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
          staffMedia(sort: START_DATE_DESC, perPage: 50, type: ANIME) {{
            edges {{ staffRole node {{ {WORK} }} }} }}
        }} }}"
    );
    let data = cached(
        state,
        &format!("anilist:staff:1:{staff_id}"),
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

/// A studio AniList knows, with its most popular work.
pub async fn studio(state: &AppState, studio_id: i64) -> AppResult<(Value, Vec<SearchItem>)> {
    let query = format!(
        "query ($id: Int) {{ Studio(id: $id) {{ id name siteUrl
          media(sort: POPULARITY_DESC, isMain: true, perPage: 40) {{ nodes {{ {WORK} }} }}
        }} }}"
    );
    let data = cached(
        state,
        &format!("anilist:studio:1:{studio_id}"),
        &query,
        json!({ "id": studio_id }),
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
    Ok((header, items))
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
