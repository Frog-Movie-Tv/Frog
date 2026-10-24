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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchItem {
    pub kind: String,
    pub tmdb_id: i64,
    pub name: String,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub vote: Option<f64>,
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

        Some(Self {
            kind,
            tmdb_id: i64_at(v, "id")?,
            name,
            poster_path: str_at(v, "poster_path"),
            backdrop_path: str_at(v, "backdrop_path"),
            year: year_of(date.as_deref()),
            overview: str_at(v, "overview"),
            vote: f64_at(v, "vote_average").filter(|n| *n > 0.0),
            in_library: None,
        })
    }
}

/// The subset we denormalise into the `title` table.
#[derive(Debug, Clone)]
pub struct TitleCore {
    pub kind: String,
    pub tmdb_id: i64,
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
            tmdb_id: i64_at(v, "id")?,
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
        let score = votes + png_bonus + wide_bonus;
        if best.as_ref().map(|(b, _)| score > *b).unwrap_or(true) {
            best = Some((score, path));
        }
    }
    best.map(|(_, p)| p)
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
}
