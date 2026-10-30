//! Curated views over TMDB's JSON.
//!
//! The raw `append_to_response` payload for a show is a few hundred kilobytes.
//! Rather than ship that to a phone, every endpoint narrows it to the fields a
//! screen actually draws. Keeping the mapping in one place also means a change
//! in TMDB's shape breaks here, loudly, instead of silently in a component.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn str_at(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn i64_at(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

pub fn f64_at(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn year_of(date: Option<&str>) -> Option<i32> {
    date.and_then(|d| d.get(0..4)).and_then(|y| y.parse().ok())
}

/// A film or show as a provider lists it: a search result, a credit, a
/// recommendation. Not necessarily in Frog's catalog yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchItem {
    pub kind: String,
    /// TMDB's id; none for an anime only AniList has.
    pub tmdb_id: Option<i64>,
    /// AniList's id, for an anime TMDB lacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anilist_id: Option<i64>,
    /// Frog's id, when the title is in this account's library.
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub vote: Option<f64>,
    /// TMDB's own popularity score, used to rank results.
    pub popularity: Option<f64>,
    pub vote_count: Option<i64>,
    pub genre_ids: Vec<i64>,
    /// Populated by the route when the title is already in the library.
    pub in_library: Option<String>,
}

impl SearchItem {
    /// `media_type` is absent on /movie and /tv list endpoints, so callers can
    /// force it.
    pub fn from_value(v: &Value, force_kind: Option<&str>) -> Option<Self> {
        let kind = force_kind
            .map(str::to_string)
            .or_else(|| str_at(v, "media_type"))?;
        if kind != "movie" && kind != "tv" {
            return None;
        }
        let name = str_at(v, "title").or_else(|| str_at(v, "name"))?;
        let date = str_at(v, "release_date").or_else(|| str_at(v, "first_air_date"));
        // AniList's answers come in TMDB's shape, marked as AniList's.
        let raw_id = i64_at(v, "id")?;
        let (tmdb_id, anilist_id) = if str_at(v, "source").as_deref() == Some("anilist") {
            (None, Some(raw_id))
        } else {
            (Some(raw_id), None)
        };

        Some(Self {
            kind,
            tmdb_id,
            anilist_id,
            id: None,
            name,
            poster_path: str_at(v, "poster_path"),
            backdrop_path: str_at(v, "backdrop_path"),
            year: year_of(date.as_deref()),
            overview: str_at(v, "overview"),
            vote: f64_at(v, "vote_average").filter(|n| *n > 0.0),
            popularity: f64_at(v, "popularity"),
            vote_count: i64_at(v, "vote_count"),
            genre_ids: v
                .get("genre_ids")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default(),
            in_library: None,
        })
    }

    /// The provider id this item stands for.
    pub fn external(&self) -> Option<crate::catalog::ExternalId> {
        match (self.tmdb_id, self.anilist_id) {
            (Some(id), _) => Some(crate::catalog::ExternalId::tmdb(&self.kind, id)),
            (None, Some(id)) => Some(crate::catalog::ExternalId::AniList(id)),
            (None, None) => None,
        }
    }

    /// Whether two items are the same title as their providers name it.
    pub fn same_as(&self, other: &SearchItem) -> bool {
        self.kind == other.kind
            && match (self.tmdb_id, other.tmdb_id) {
                (Some(a), Some(b)) => a == b,
                (None, None) => self.anilist_id.is_some() && self.anilist_id == other.anilist_id,
                _ => false,
            }
    }
}

/// The subset we denormalise into the `title` table. Which provider it came
/// from is the caller's to say (`catalog::ensure`).
#[derive(Debug, Clone)]
pub struct TitleCore {
    pub kind: String,
    pub name: String,
    pub original_name: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub release_date: Option<String>,
    pub last_air_date: Option<String>,
    pub tmdb_status: Option<String>,
    pub runtime: Option<i64>,
    pub episode_count: Option<i64>,
    pub season_count: Option<i64>,
    pub genres_json: String,
    pub tmdb_vote: Option<f64>,
    pub imdb_id: Option<String>,
    pub collection_id: Option<i64>,
}

/// Empty, but with genres as the empty list SQL's `json_each` can read: an
/// empty string there fails every query that ranks by genre.
impl Default for TitleCore {
    fn default() -> Self {
        Self {
            kind: String::new(),
            name: String::new(),
            original_name: None,
            overview: None,
            poster_path: None,
            backdrop_path: None,
            logo_path: None,
            release_date: None,
            last_air_date: None,
            tmdb_status: None,
            runtime: None,
            episode_count: None,
            season_count: None,
            genres_json: "[]".to_string(),
            tmdb_vote: None,
            imdb_id: None,
            collection_id: None,
        }
    }
}

impl TitleCore {
    pub fn from_detail(kind: &str, v: &Value) -> Option<Self> {
        let is_tv = kind == "tv";
        let genres: Vec<String> = v
            .get("genres")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|g| str_at(g, "name")).collect())
            .unwrap_or_default();

        let imdb_id = str_at(v, "imdb_id")
            .or_else(|| v.get("external_ids").and_then(|e| str_at(e, "imdb_id")));

        Some(Self {
            kind: kind.to_string(),
            name: str_at(v, "title").or_else(|| str_at(v, "name"))?,
            original_name: str_at(v, "original_title").or_else(|| str_at(v, "original_name")),
            overview: str_at(v, "overview"),
            poster_path: str_at(v, "poster_path"),
            backdrop_path: str_at(v, "backdrop_path"),
            logo_path: pick_logo(v),
            release_date: str_at(v, "release_date").or_else(|| str_at(v, "first_air_date")),
            last_air_date: if is_tv {
                str_at(v, "last_air_date")
            } else {
                None
            },
            tmdb_status: str_at(v, "status"),
            runtime: i64_at(v, "runtime").or_else(|| {
                v.get("episode_run_time")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(Value::as_i64)
            }),
            episode_count: i64_at(v, "number_of_episodes"),
            season_count: i64_at(v, "number_of_seasons"),
            genres_json: serde_json::to_string(&genres).unwrap_or_else(|_| "[]".into()),
            tmdb_vote: f64_at(v, "vote_average").filter(|n| *n > 0.0),
            imdb_id,
            collection_id: v.get("belongs_to_collection").and_then(|c| i64_at(c, "id")),
        })
    }
}

/// Prefer a wide PNG logo: those are the transparent title treatments that make
/// the hero look like a streaming app rather than a database.
fn pick_logo(v: &Value) -> Option<String> {
    let logos = v.get("images")?.get("logos")?.as_array()?;
    let mut best: Option<(f64, String)> = None;
    for l in logos {
        let path = str_at(l, "file_path")?;
        let ratio = f64_at(l, "aspect_ratio").unwrap_or(1.0);
        let votes = f64_at(l, "vote_average").unwrap_or(0.0);
        let png_bonus = if path.ends_with(".png") { 2.0 } else { 0.0 };
        let wide_bonus = if ratio > 1.6 { 1.0 } else { 0.0 };
        // Many anime only have a Japanese logo. Better than none, but never
        // ahead of one in the reader's own language.
        let foreign = if str_at(l, "iso_639_1").as_deref() == Some("ja") {
            -20.0
        } else {
            0.0
        };
        let score = votes + png_bonus + wide_bonus + foreign;
        if best.as_ref().map(|(b, _)| score > *b).unwrap_or(true) {
            best = Some((score, path));
        }
    }
    best.map(|(_, p)| p)
}

/// A title's age rating for a region: what TMDB lists for that country, or
/// for the US when it has none there. Films give a certificate per release;
/// the theatrical one is the one people know.
pub fn age_rating(v: &Value, region: &str) -> Option<String> {
    let for_country = |code: &str| -> Option<String> {
        if let Some(list) = v
            .pointer("/content_ratings/results")
            .and_then(Value::as_array)
        {
            let entry = list
                .iter()
                .find(|c| c["iso_3166_1"].as_str() == Some(code))?;
            return str_at(entry, "rating");
        }
        let list = v.pointer("/release_dates/results")?.as_array()?;
        let entry = list
            .iter()
            .find(|c| c["iso_3166_1"].as_str() == Some(code))?;
        let mut dates: Vec<&Value> = entry["release_dates"].as_array()?.iter().collect();
        // 3 is a theatrical release, 2 a limited one; the rest come after.
        dates.sort_by_key(|d| match d["type"].as_i64() {
            Some(3) => 0,
            Some(2) => 1,
            _ => 2,
        });
        dates.iter().find_map(|d| str_at(d, "certification"))
    };
    for_country(region).or_else(|| for_country("US"))
}

/// A cast member as a character someone might look for.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterCredit {
    pub name: String,
    pub actor: Option<String>,
    pub actor_id: Option<i64>,
    pub ord: i64,
}

/// How many characters of one title search keeps.
const SEARCH_CAST: usize = 30;

/// The character a credit names, without what casting adds to it, or none
/// when the credit is a role rather than a character: "Self", "Host", "Guest".
pub fn character_name(credit: &str) -> Option<String> {
    let mut name = credit.trim();
    while let Some(start) = name.rfind('(').filter(|_| name.ends_with(')')) {
        name = name[..start].trim_end();
    }
    let name = name.trim_matches(|c: char| c == '-' || c == '/' || c.is_whitespace());
    let lower = name.to_lowercase();
    const NOT_CHARACTERS: [&str; 16] = [
        "self",
        "himself",
        "herself",
        "themselves",
        "themself",
        "host",
        "co-host",
        "narrator",
        "presenter",
        "guest",
        "various",
        "extra",
        "additional voices",
        "archive footage",
        "contestant",
        "interviewee",
    ];
    let role = NOT_CHARACTERS.iter().any(|r| {
        lower == *r || lower.starts_with(&format!("{r} ")) || lower.starts_with(&format!("{r}-"))
    });
    (!name.is_empty() && !role).then(|| name.to_string())
}

/// The characters of a title's cast, top billing first.
pub fn characters(v: &Value) -> Vec<CharacterCredit> {
    let mut cast: Vec<&Value> = v
        .pointer("/credits/cast")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    cast.sort_by_key(|c| i64_at(c, "order").unwrap_or(999));
    cast.into_iter()
        .filter_map(|c| {
            Some((
                character_name(c["character"].as_str()?)?,
                str_at(c, "name"),
                i64_at(c, "id"),
            ))
        })
        .take(SEARCH_CAST)
        .enumerate()
        .map(|(i, (name, actor, actor_id))| CharacterCredit {
            name,
            actor,
            actor_id,
            ord: i as i64,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct CastMember {
    pub id: i64,
    pub name: String,
    pub character: Option<String>,
    pub profile_path: Option<String>,
    pub order: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrewMember {
    pub id: i64,
    pub name: String,
    pub job: String,
    pub profile_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Trailer {
    pub key: String,
    pub name: String,
    pub site: String,
    pub kind: String,
    pub official: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderGroup {
    pub flatrate: Vec<Provider>,
    pub rent: Vec<Provider>,
    pub buy: Vec<Provider>,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Provider {
    pub id: i64,
    pub name: String,
    pub logo_path: Option<String>,
    /// The title's own page on the service, when the source gives one
    /// (AniList does; TMDB only has its own watch page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeasonSummary {
    pub season_number: i64,
    pub name: String,
    pub episode_count: i64,
    pub poster_path: Option<String>,
    pub air_date: Option<String>,
    pub overview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EpisodeInfo {
    pub season_number: i64,
    pub episode_number: i64,
    pub tmdb_episode_id: Option<i64>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub still_path: Option<String>,
    pub air_date: Option<String>,
    pub runtime: Option<i64>,
    pub tmdb_vote: Option<f64>,
}

pub fn cast(v: &Value, limit: usize) -> Vec<CastMember> {
    v.get("credits")
        .and_then(|c| c.get("cast"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    Some(CastMember {
                        id: i64_at(c, "id")?,
                        name: str_at(c, "name")?,
                        character: str_at(c, "character"),
                        profile_path: str_at(c, "profile_path"),
                        order: i64_at(c, "order").unwrap_or(999),
                    })
                })
                .take(limit)
                .collect()
        })
        .unwrap_or_default()
}

/// Directors and writers for films; creators for shows.
pub fn key_crew(v: &Value) -> Vec<CrewMember> {
    let mut out: Vec<CrewMember> = v
        .get("created_by")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    Some(CrewMember {
                        id: i64_at(c, "id")?,
                        name: str_at(c, "name")?,
                        job: "Creator".into(),
                        profile_path: str_at(c, "profile_path"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    const WANTED: [&str; 4] = ["Director", "Screenplay", "Writer", "Story"];
    if let Some(crew) = v
        .get("credits")
        .and_then(|c| c.get("crew"))
        .and_then(Value::as_array)
    {
        for c in crew {
            let job = match str_at(c, "job") {
                Some(j) if WANTED.contains(&j.as_str()) => j,
                _ => continue,
            };
            let (Some(id), Some(name)) = (i64_at(c, "id"), str_at(c, "name")) else {
                continue;
            };
            if out.iter().any(|e| e.id == id && e.job == job) {
                continue;
            }
            out.push(CrewMember {
                id,
                name,
                job,
                profile_path: str_at(c, "profile_path"),
            });
        }
    }
    out.truncate(8);
    out
}

/// One row of `title_person`: who stats can credit a title to.
#[derive(Debug, Clone, PartialEq)]
pub struct StatCredit {
    pub role: &'static str,
    pub tmdb_id: i64,
    pub name: String,
    pub profile_path: Option<String>,
    pub ord: i64,
}

/// Billing beyond this is mostly one-scene parts, which would let a prolific
/// bit-part actor outrank the leads you actually watched.
const STAT_CAST: usize = 12;

/// Top-billed cast, directors (films), creators (shows) and production
/// companies -- what the stats page ranks.
pub fn stat_credits(v: &Value) -> Vec<StatCredit> {
    let mut out = Vec::new();
    let mut push = |role: &'static str, item: &Value, image_key: &str, ord: i64| {
        let (Some(tmdb_id), Some(name)) = (i64_at(item, "id"), str_at(item, "name")) else {
            return;
        };
        if out
            .iter()
            .any(|c: &StatCredit| c.role == role && c.tmdb_id == tmdb_id)
        {
            return;
        }
        out.push(StatCredit {
            role,
            tmdb_id,
            name,
            profile_path: str_at(item, image_key),
            ord,
        });
    };

    let credits = v.get("credits");
    let list = |parent: Option<&Value>, key: &str| -> Vec<Value> {
        parent
            .and_then(|p| p.get(key))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };

    let mut cast = list(credits, "cast");
    cast.sort_by_key(|c| i64_at(c, "order").unwrap_or(999));
    for (i, c) in cast.iter().take(STAT_CAST).enumerate() {
        push("cast", c, "profile_path", i as i64);
    }
    for (i, c) in list(credits, "crew")
        .iter()
        .filter(|c| str_at(c, "job").as_deref() == Some("Director"))
        .enumerate()
    {
        push("director", c, "profile_path", i as i64);
    }
    for (i, c) in list(Some(v), "created_by").iter().enumerate() {
        push("creator", c, "profile_path", i as i64);
    }
    for (i, c) in list(Some(v), "production_companies").iter().enumerate() {
        push("studio", c, "logo_path", i as i64);
    }
    out
}

pub fn trailers(v: &Value) -> Vec<Trailer> {
    let mut out: Vec<Trailer> = v
        .get("videos")
        .and_then(|x| x.get("results"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    let site = str_at(t, "site")?;
                    if site != "YouTube" {
                        return None;
                    }
                    let kind = str_at(t, "type")?;
                    if kind != "Trailer" && kind != "Teaser" {
                        return None;
                    }
                    Some(Trailer {
                        key: str_at(t, "key")?,
                        name: str_at(t, "name").unwrap_or_else(|| kind.clone()),
                        site,
                        kind,
                        official: t.get("official").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Official trailers first, teasers last.
    out.sort_by_key(|t| (!t.official, t.kind != "Trailer"));
    out.truncate(6);
    out
}

pub fn providers(v: &Value, region: &str) -> ProviderGroup {
    let node = v
        .get("watch/providers")
        .and_then(|p| p.get("results"))
        .and_then(|r| r.get(region));

    let list = |key: &str| -> Vec<Provider> {
        node.and_then(|n| n.get(key))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some(Provider {
                            id: i64_at(p, "provider_id")?,
                            name: str_at(p, "provider_name")?,
                            logo_path: str_at(p, "logo_path"),
                            url: str_at(p, "url"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    ProviderGroup {
        flatrate: list("flatrate"),
        rent: list("rent"),
        buy: list("buy"),
        link: node.and_then(|n| str_at(n, "link")),
    }
}

pub fn seasons(v: &Value) -> Vec<SeasonSummary> {
    v.get("seasons")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    Some(SeasonSummary {
                        season_number: i64_at(s, "season_number")?,
                        name: str_at(s, "name").unwrap_or_else(|| "Season".into()),
                        episode_count: i64_at(s, "episode_count").unwrap_or(0),
                        poster_path: str_at(s, "poster_path"),
                        air_date: str_at(s, "air_date"),
                        overview: str_at(s, "overview"),
                    })
                })
                .filter(|s| s.episode_count > 0)
                .collect()
        })
        .unwrap_or_default()
}

pub fn episodes_of_season(v: &Value) -> Vec<EpisodeInfo> {
    let season_number = i64_at(v, "season_number").unwrap_or(0);
    v.get("episodes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    Some(EpisodeInfo {
                        season_number: i64_at(e, "season_number").unwrap_or(season_number),
                        episode_number: i64_at(e, "episode_number")?,
                        tmdb_episode_id: i64_at(e, "id"),
                        name: str_at(e, "name"),
                        overview: str_at(e, "overview"),
                        still_path: str_at(e, "still_path"),
                        air_date: str_at(e, "air_date"),
                        runtime: i64_at(e, "runtime"),
                        tmdb_vote: f64_at(e, "vote_average").filter(|n| *n > 0.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn related(v: &Value, key: &str, limit: usize) -> Vec<SearchItem> {
    v.get(key)
        .and_then(|r| r.get("results"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|item| {
                    let forced = if item.get("title").is_some() {
                        "movie"
                    } else {
                        "tv"
                    };
                    SearchItem::from_value(item, Some(forced))
                })
                .take(limit)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn search_item_handles_movies_and_shows() {
        let movie = json!({
            "media_type": "movie", "id": 1, "title": "Heat",
            "release_date": "1995-12-15", "vote_average": 7.9
        });
        let m = SearchItem::from_value(&movie, None).unwrap();
        assert_eq!(m.kind, "movie");
        assert_eq!(m.year, Some(1995));

        let show = json!({
            "media_type": "tv", "id": 2, "name": "Severance",
            "first_air_date": "2022-02-18"
        });
        let s = SearchItem::from_value(&show, None).unwrap();
        assert_eq!(s.name, "Severance");
        assert_eq!(s.year, Some(2022));

        let person = json!({ "media_type": "person", "id": 3, "name": "Mann" });
        assert!(SearchItem::from_value(&person, None).is_none());
    }

    #[test]
    fn a_japanese_logo_is_only_a_last_resort() {
        let v = json!({ "images": { "logos": [
            { "file_path": "/ja.png", "aspect_ratio": 3.0, "vote_average": 9.0, "iso_639_1": "ja" },
            { "file_path": "/en.png", "aspect_ratio": 3.0, "vote_average": 1.0, "iso_639_1": "en" }
        ] } });
        assert_eq!(pick_logo(&v).as_deref(), Some("/en.png"));
        let v = json!({ "images": { "logos": [
            { "file_path": "/ja.png", "aspect_ratio": 3.0, "iso_639_1": "ja" }
        ] } });
        assert_eq!(pick_logo(&v).as_deref(), Some("/ja.png"));
    }

    #[test]
    fn the_age_rating_is_the_regions_then_the_us() {
        let tv = json!({ "content_ratings": { "results": [
            { "iso_3166_1": "US", "rating": "TV-MA" },
            { "iso_3166_1": "FR", "rating": "16" }
        ] } });
        assert_eq!(age_rating(&tv, "FR").as_deref(), Some("16"));
        assert_eq!(age_rating(&tv, "BE").as_deref(), Some("TV-MA"));
        let film = json!({ "release_dates": { "results": [
            { "iso_3166_1": "US", "release_dates": [
                { "certification": "", "type": 5 },
                { "certification": "PG", "type": 4 },
                { "certification": "R", "type": 3 }
            ] }
        ] } });
        assert_eq!(age_rating(&film, "BE").as_deref(), Some("R"));
        assert_eq!(age_rating(&json!({}), "BE"), None);
    }

    #[test]
    fn roles_are_not_characters() {
        assert_eq!(character_name("Tom (voice)").as_deref(), Some("Tom"));
        assert_eq!(
            character_name("Jerry Mouse (uncredited) (voice)").as_deref(),
            Some("Jerry Mouse")
        );
        assert_eq!(character_name("Self"), None);
        assert_eq!(character_name("Self - Host"), None);
        assert_eq!(character_name("Himself (archive footage)"), None);
        assert_eq!(character_name("Narrator (voice)"), None);
        assert_eq!(character_name(""), None);
        assert_eq!(character_name("Selfridge").as_deref(), Some("Selfridge"));
    }

    #[test]
    fn logo_picker_prefers_wide_png() {
        let v = json!({ "images": { "logos": [
            { "file_path": "/narrow.svg", "aspect_ratio": 1.1, "vote_average": 5.0 },
            { "file_path": "/wide.png", "aspect_ratio": 3.2, "vote_average": 4.0 }
        ]}});
        assert_eq!(pick_logo(&v).as_deref(), Some("/wide.png"));
    }

    #[test]
    fn trailers_put_official_first_and_drop_non_youtube() {
        let v = json!({ "videos": { "results": [
            { "site": "Vimeo", "type": "Trailer", "key": "nope", "name": "v" },
            { "site": "YouTube", "type": "Teaser", "key": "t1", "name": "Teaser", "official": true },
            { "site": "YouTube", "type": "Trailer", "key": "t2", "name": "Trailer", "official": true }
        ]}});
        let t = trailers(&v);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].key, "t2");
    }

    #[test]
    fn providers_read_the_requested_region_only() {
        let v = json!({ "watch/providers": { "results": {
            "BE": { "link": "https://x", "flatrate": [
                { "provider_id": 8, "provider_name": "Netflix", "logo_path": "/n.jpg" }
            ]},
            "US": { "flatrate": [ { "provider_id": 9, "provider_name": "Hulu" } ] }
        }}});
        let p = providers(&v, "BE");
        assert_eq!(p.flatrate.len(), 1);
        assert_eq!(p.flatrate[0].name, "Netflix");
        assert!(providers(&v, "NL").flatrate.is_empty());
    }

    #[test]
    fn title_core_reads_both_shapes() {
        let tv = json!({
            "id": 10, "name": "Show", "first_air_date": "2020-01-01",
            "number_of_seasons": 3, "number_of_episodes": 30,
            "genres": [{ "id": 1, "name": "Drama" }],
            "external_ids": { "imdb_id": "tt123" }
        });
        let c = TitleCore::from_detail("tv", &tv).unwrap();
        assert_eq!(c.season_count, Some(3));
        assert_eq!(c.imdb_id.as_deref(), Some("tt123"));
        assert_eq!(c.genres_json, "[\"Drama\"]");
    }

    #[test]
    fn stat_credits_keep_leads_directors_creators_and_studios() {
        let mut cast: Vec<Value> = (0..20)
            .map(|i| json!({ "id": 100 + i, "name": format!("Actor {i}"), "order": i }))
            .collect();
        // TMDB does not promise billing order in the array itself.
        cast.reverse();
        let v = json!({
            "credits": {
                "cast": cast,
                "crew": [
                    { "id": 7, "name": "Dir", "job": "Director", "profile_path": "/d.jpg" },
                    { "id": 7, "name": "Dir", "job": "Director" },
                    { "id": 8, "name": "Editor", "job": "Editor" }
                ]
            },
            "created_by": [{ "id": 9, "name": "Maker" }],
            "production_companies": [{ "id": 3, "name": "Studio", "logo_path": "/s.png" }]
        });

        let c = stat_credits(&v);
        let of = |role: &str| c.iter().filter(|x| x.role == role).collect::<Vec<_>>();
        let cast = of("cast");
        assert_eq!(cast.len(), STAT_CAST);
        assert_eq!(cast[0].name, "Actor 0", "billing order, not array order");
        assert_eq!(
            of("director").len(),
            1,
            "duplicates collapse, editors are skipped"
        );
        assert_eq!(of("director")[0].profile_path.as_deref(), Some("/d.jpg"));
        assert_eq!(of("creator")[0].name, "Maker");
        assert_eq!(of("studio")[0].profile_path.as_deref(), Some("/s.png"));
        assert!(stat_credits(&json!({})).is_empty());
    }
}
