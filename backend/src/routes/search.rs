use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::catalog::ExternalId;
use crate::error::{AppError, AppResult};
use crate::routes::shelves::{self, Rules};
use crate::state::AppState;
use crate::text::{self, Closeness};
use crate::tmdb::models::SearchItem;

/// TMDB's genre id for Animation, shared by films and shows.
const ANIMATION_GENRE: i64 = 16;

/// TMDB's genres: the name Frog stores, and its id for films and for shows.
/// TMDB keeps two lists and gives a few genres different ids in each.
pub const GENRES: [(&str, Option<i64>, Option<i64>); 25] = [
    ("Action", Some(28), Some(10759)),
    ("Action & Adventure", None, Some(10759)),
    ("Adventure", Some(12), Some(10759)),
    ("Animation", Some(16), Some(16)),
    ("Comedy", Some(35), Some(35)),
    ("Crime", Some(80), Some(80)),
    ("Documentary", Some(99), Some(99)),
    ("Drama", Some(18), Some(18)),
    ("Family", Some(10751), Some(10751)),
    ("Fantasy", Some(14), Some(10765)),
    ("History", Some(36), None),
    ("Horror", Some(27), None),
    ("Kids", None, Some(10762)),
    ("Music", Some(10402), None),
    ("Mystery", Some(9648), Some(9648)),
    ("News", None, Some(10763)),
    ("Reality", None, Some(10764)),
    ("Romance", Some(10749), None),
    ("Sci-Fi & Fantasy", None, Some(10765)),
    ("Science Fiction", Some(878), Some(10765)),
    ("Soap", None, Some(10766)),
    ("Talk", None, Some(10767)),
    ("Thriller", Some(53), None),
    ("War", Some(10752), Some(10768)),
    ("Western", Some(37), Some(37)),
];

fn genre_ids(name: &str) -> Vec<i64> {
    GENRES
        .iter()
        .find(|g| g.0 == name)
        .map(|g| [g.1, g.2].into_iter().flatten().collect())
        .unwrap_or_default()
}

/// The genres to offer, without TMDB's two names for one thing.
pub async fn genres(_: Auth) -> Json<Value> {
    let names: Vec<&str> = GENRES
        .iter()
        .map(|g| g.0)
        .filter(|g| !matches!(*g, "Action & Adventure" | "Sci-Fi & Fantasy"))
        .collect();
    Json(json!({ "genres": names }))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "first_page")]
    pub page: u32,
    /// Leave animation out of the results.
    #[serde(default)]
    pub hide_animation: bool,
    /// movie | tv
    pub kind: Option<String>,
    /// Any of these, comma separated.
    pub genres: Option<String>,
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
    /// What the title's own provider scores it, 0-100.
    pub min_score: Option<f64>,
    /// Any of these library statuses, comma separated; `none` is not in the
    /// library.
    pub statuses: Option<String>,
    /// popular | rating | newest | oldest; best match when absent.
    pub sort: Option<String>,
}

fn first_page() -> u32 {
    1
}

/// A search's filters, the same ones a smart shelf has where they mean the
/// same thing.
#[derive(Debug, Default, PartialEq, Clone)]
struct Filters {
    kind: Option<String>,
    genres: Vec<String>,
    from_year: Option<i32>,
    to_year: Option<i32>,
    min_score: Option<f64>,
    statuses: Vec<String>,
    sort: Option<String>,
}

fn list(raw: &Option<String>) -> Vec<String> {
    raw.as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

impl Filters {
    fn from_query(q: &SearchQuery) -> AppResult<Self> {
        let kind = q.kind.clone().filter(|k| !k.is_empty());
        if kind.as_deref().is_some_and(|k| k != "movie" && k != "tv") {
            return Err(AppError::BadRequest("That isn't a film or a show.".into()));
        }
        let sort = q.sort.clone().filter(|s| !s.is_empty());
        if sort
            .as_deref()
            .is_some_and(|s| !matches!(s, "popular" | "rating" | "newest" | "oldest"))
        {
            return Err(AppError::BadRequest("Frog can't sort by that.".into()));
        }
        let statuses = list(&q.statuses);
        if statuses
            .iter()
            .any(|s| s != "none" && !crate::store::STATUSES.contains(&s.as_str()))
        {
            return Err(AppError::BadRequest("That isn't a status.".into()));
        }
        Ok(Self {
            kind,
            genres: list(&q.genres),
            from_year: q.from_year,
            to_year: q.to_year,
            min_score: q.min_score.filter(|s| *s > 0.0),
            statuses,
            sort,
        })
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether only what is in the library can match.
    fn library_only(&self) -> bool {
        !self.statuses.is_empty() && !self.statuses.iter().any(|s| s == "none")
    }

    fn matches(&self, item: &SearchItem) -> bool {
        if self.kind.as_deref().is_some_and(|k| k != item.kind) {
            return false;
        }
        if self.from_year.is_some() || self.to_year.is_some() {
            let Some(year) = item.year else { return false };
            if self.from_year.is_some_and(|y| year < y) || self.to_year.is_some_and(|y| year > y) {
                return false;
            }
        }
        if let Some(min) = self.min_score {
            if item.vote.unwrap_or(0.0) * 10.0 < min {
                return false;
            }
        }
        self.genres.is_empty()
            || self
                .genres
                .iter()
                .any(|g| genre_ids(g).iter().any(|id| item.genre_ids.contains(id)))
    }

    /// Whether films (or shows) can match the chosen genres at all. Talk is
    /// only a show's genre and Horror only a film's; asking for Talk must
    /// leave films out, not bring them back unfiltered.
    fn fits_kind(&self, kind: &str) -> bool {
        self.genres.is_empty()
            || self.genres.iter().any(|g| {
                GENRES
                    .iter()
                    .find(|x| x.0 == g)
                    .is_some_and(|x| if kind == "movie" { x.1 } else { x.2 }.is_some())
            })
    }

    /// Both "not in my library" and some library status: the library part is
    /// read from the library, since TMDB's pages hold only a little of it.
    fn library_and_beyond(&self) -> bool {
        self.statuses.iter().any(|s| s == "none") && self.statuses.iter().any(|s| s != "none")
    }

    /// After `annotate_library`: which results the status filter keeps.
    fn matches_status(&self, item: &SearchItem) -> bool {
        self.statuses.is_empty()
            || match &item.in_library {
                Some(status) => self.statuses.contains(status),
                None => self.statuses.iter().any(|s| s == "none"),
            }
    }

    fn sorted(&self, items: &mut [SearchItem]) {
        let date = |i: &SearchItem| i.year.unwrap_or(0);
        match self.sort.as_deref() {
            Some("popular") => items.sort_by(|a, b| {
                b.popularity
                    .unwrap_or(0.0)
                    .total_cmp(&a.popularity.unwrap_or(0.0))
            }),
            Some("rating") => {
                items.sort_by(|a, b| b.vote.unwrap_or(0.0).total_cmp(&a.vote.unwrap_or(0.0)))
            }
            Some("newest") => items.sort_by_key(|i| std::cmp::Reverse(date(i))),
            Some("oldest") => items.sort_by_key(|i| {
                let year = date(i);
                if year == 0 {
                    i32::MAX
                } else {
                    year
                }
            }),
            _ => {}
        }
    }

    /// TMDB's own parameters for these filters.
    fn discover_params(&self, kind: &str, hide_animation: bool) -> Vec<(&'static str, String)> {
        let dated = if kind == "movie" {
            "primary_release_date"
        } else {
            "first_air_date"
        };
        let mut params: Vec<(&'static str, String)> = Vec::new();
        let (gte, lte) = if kind == "movie" {
            ("primary_release_date.gte", "primary_release_date.lte")
        } else {
            ("first_air_date.gte", "first_air_date.lte")
        };
        if let Some(y) = self.from_year {
            params.push((gte, format!("{y}-01-01")));
        }
        if let Some(y) = self.to_year {
            params.push((lte, format!("{y}-12-31")));
        }
        if let Some(min) = self.min_score {
            params.push(("vote_average.gte", format!("{:.1}", min / 10.0)));
        }
        let ids: Vec<String> = self
            .genres
            .iter()
            .filter_map(|g| GENRES.iter().find(|x| x.0 == g))
            .filter_map(|g| if kind == "movie" { g.1 } else { g.2 })
            .map(|id| id.to_string())
            .collect();
        if !ids.is_empty() {
            // A pipe is any of them, as on a shelf.
            params.push(("with_genres", ids.join("|")));
        }
        if hide_animation {
            params.push(("without_genres", ANIMATION_GENRE.to_string()));
        }
        let sort = match self.sort.as_deref() {
            Some("rating") => "vote_average.desc".to_string(),
            Some("newest") => format!("{dated}.desc"),
            Some("oldest") => format!("{dated}.asc"),
            _ => "popularity.desc".to_string(),
        };
        // Top rated means rated by enough people to say so; and the very
        // oldest and newest entries are mostly empty ones.
        let votes = if self.sort.is_some() { 50 } else { 5 };
        params.push(("vote_count.gte", votes.to_string()));
        params.push(("sort_by", sort));
        params
    }
}

/// How many of TMDB's pages one filtered search reads. TMDB can only filter
/// when there is no name, so a name and filters are matched here.
const FILTERED_PAGES: u32 = 3;

pub async fn search(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let term = q.q.trim();
    let filters = Filters::from_query(&q)?;
    if term.is_empty() && filters.is_empty() {
        return Err(AppError::BadRequest(
            "give me something to search for".into(),
        ));
    }
    if filters.library_only() {
        let items = from_library(&state, auth.user_id(), term, &filters).await?;
        return Ok(Json(
            json!({ "page": 1, "next_page": null, "results": items }),
        ));
    }

    let page = q.page.clamp(1, 500);
    let (mut items, next_page) = if term.is_empty() {
        discovered(&state, &filters, q.hide_animation, page).await?
    } else {
        named(&state, term, &filters, q.hide_animation, page).await?
    };
    if page == 1 && !term.is_empty() {
        let known = known_titles(&state, auth.user_id(), term, q.hide_animation).await?;
        let known = known.into_iter().filter(|i| filters.matches(i)).collect();
        items = merge_known(items, known, term);
    }
    annotate_library(&state, auth.user_id(), &mut items).await?;
    items.retain(|i| filters.matches_status(i));
    if page == 1 && filters.library_and_beyond() {
        let mine = Filters {
            statuses: filters
                .statuses
                .iter()
                .filter(|s| *s != "none")
                .cloned()
                .collect(),
            sort: filters.sort.clone(),
            ..filters.clone()
        };
        let mut owned = from_library(&state, auth.user_id(), term, &mine).await?;
        items.retain(|i| i.id.is_none() || !owned.iter().any(|o| o.id == i.id));
        owned.append(&mut items);
        items = owned;
    }
    filters.sorted(&mut items);

    let person_first = page == 1 && !term.is_empty() && names_a_person(&state, term).await;
    Ok(Json(json!({
        "page": page,
        "next_page": next_page,
        "person_first": person_first,
        "results": items,
    })))
}

/// Whether someone typed a person's whole name and no title's. TMDB's mixed
/// search already says, so the page knows the order of its sections before
/// the people themselves arrive, and nothing moves when they do.
async fn names_a_person(state: &AppState, term: &str) -> bool {
    let Ok(raw) = state.tmdb.search_multi(term, 1).await else {
        return false;
    };
    let typed = text::words(term);
    let mut person = false;
    for r in raw["results"].as_array().into_iter().flatten() {
        let name = r["title"].as_str().or(r["name"].as_str()).unwrap_or("");
        if text::closeness(name, &typed) == Closeness::Exact {
            if r["media_type"].as_str() != Some("person") {
                return false;
            }
            person = true;
        }
    }
    person
}

/// Titles by name from TMDB, ranked. With filters, as many of TMDB's pages
/// as it takes to have something to show, each matched here.
async fn named(
    state: &AppState,
    term: &str,
    filters: &Filters,
    hide_animation: bool,
    page: u32,
) -> AppResult<(Vec<SearchItem>, Option<u32>)> {
    let pages = if filters.is_empty() {
        1
    } else {
        FILTERED_PAGES
    };
    let mut items = Vec::new();
    let mut last = page;
    let mut total = page;
    for n in page..page + pages {
        let raw = state.tmdb.search_multi(term, n).await?;
        total = raw["total_pages"].as_i64().unwrap_or(1) as u32;
        items.extend(collect(&raw, None));
        last = n;
        if n >= total {
            break;
        }
    }
    items.retain(|i| filters.matches(i));
    rank(&mut items, term, hide_animation);
    Ok((items, (last < total).then_some(last + 1)))
}

/// Titles by filters alone, from TMDB's browse. Films and shows together are
/// two lists; they are told apart by their badges and merged by popularity.
async fn discovered(
    state: &AppState,
    filters: &Filters,
    hide_animation: bool,
    page: u32,
) -> AppResult<(Vec<SearchItem>, Option<u32>)> {
    let kinds: &[&str] = match filters.kind.as_deref() {
        Some("movie") => &["movie"],
        Some(_) => &["tv"],
        None => &["movie", "tv"],
    };
    let mut items = Vec::new();
    let mut more = false;
    for kind in kinds.iter().filter(|k| filters.fits_kind(k)) {
        let params = filters.discover_params(kind, hide_animation);
        let raw = state.tmdb.discover(kind, &params, page).await?;
        more |= raw["total_pages"].as_i64().unwrap_or(1) > i64::from(page);
        items.extend(collect(&raw, Some(kind)));
    }
    if filters.sort.is_none() {
        items.sort_by(|a, b| {
            b.popularity
                .unwrap_or(0.0)
                .total_cmp(&a.popularity.unwrap_or(0.0))
        });
    }
    Ok((items, more.then_some(page + 1)))
}

/// The library, through the same rules a smart shelf uses.
async fn from_library(
    state: &AppState,
    user_id: i64,
    term: &str,
    filters: &Filters,
) -> AppResult<Vec<SearchItem>> {
    let rules = Rules {
        kind: filters.kind.clone(),
        statuses: filters.statuses.clone(),
        genres: filters.genres.clone(),
        from_year: filters.from_year,
        to_year: filters.to_year,
        sort: Some(
            match filters.sort.as_deref() {
                Some("newest") | Some("oldest") => "release",
                Some("rating") => "rating",
                _ => "updated",
            }
            .to_string(),
        ),
        order: Some(if filters.sort.as_deref() == Some("oldest") {
            "asc".to_string()
        } else {
            "desc".to_string()
        }),
        ..Rules::default()
    };
    let typed = text::words(term);
    let rows = shelves::evaluate(&state.pool, user_id, &rules).await?;
    // Every provider id and status of this library, once.
    let known: Vec<(i64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT l.title_id, l.status, x.provider, x.external_id
           FROM library_entry l
           LEFT JOIN title_external x ON x.title_id = l.title_id AND x.is_primary = 1
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    let known: HashMap<i64, (String, Option<ExternalId>)> = known
        .into_iter()
        .map(|(id, status, provider, key)| {
            let source = provider
                .zip(key)
                .and_then(|(p, k)| ExternalId::parse(&p, &k));
            (id, (status, source))
        })
        .collect();

    let mut items: Vec<SearchItem> = rows
        .into_iter()
        .filter(|r| typed.is_empty() || text::closeness(&r.name, &typed) != Closeness::Loose)
        .filter(|r| {
            filters
                .min_score
                .is_none_or(|min| r.tmdb_vote.unwrap_or(0.0) * 10.0 >= min)
        })
        .map(|r| {
            let (status, source) = known.get(&r.id).cloned().unwrap_or_default();
            SearchItem {
                kind: r.kind,
                tmdb_id: source.as_ref().and_then(ExternalId::tmdb_id),
                anilist_id: match source {
                    Some(ExternalId::AniList(a)) => Some(a),
                    _ => None,
                },
                id: Some(r.id),
                name: r.name,
                poster_path: r.poster_path,
                backdrop_path: r.backdrop_path,
                year: r
                    .release_date
                    .as_deref()
                    .and_then(|d| d.get(..4))
                    .and_then(|y| y.parse().ok()),
                overview: None,
                vote: r.tmdb_vote,
                popularity: None,
                vote_count: None,
                genre_ids: Vec::new(),
                in_library: Some(status).filter(|s| !s.is_empty()),
            }
        })
        .collect();
    if !typed.is_empty() {
        items.sort_by_key(|i| text::closeness(&i.name, &typed));
    }
    Ok(items)
}

pub async fn trending(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let raw = state.tmdb.trending().await?;
    let mut items = collect(&raw, None);
    annotate_library(&state, auth.user_id(), &mut items).await?;
    Ok(Json(serde_json::json!({ "results": items })))
}

/// (id, kind, primary provider, its id, name, poster, backdrop, release date,
///  vote, genres)
type KnownRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<f64>,
    String,
);

/// Titles this Frog already knows whose name starts with what was typed, or
/// has a word that does. TMDB matches whole words only, so "inter" never
/// finds Interstellar there; typing a few letters of something you track
/// should. Your own library first. Names are compared folded, so "pokemon"
/// finds Pokémon, and "&" finds "and".
async fn known_titles(
    state: &AppState,
    user_id: i64,
    term: &str,
    hide_animation: bool,
) -> AppResult<Vec<SearchItem>> {
    if term.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let clean = |s: &str| s.replace(['\\', '%', '_'], "");
    let pattern = clean(&text::fold(term));
    let alternative = clean(&text::fold(&term.replace('&', "and")));
    let rows: Vec<KnownRow> = sqlx::query_as(
        "SELECT t.id, t.kind, x.provider, x.external_id, t.name, t.poster_path,
                t.backdrop_path, t.release_date, t.tmdb_vote, t.genres_json
           FROM title t
           LEFT JOIN title_external x ON x.title_id = t.id AND x.is_primary = 1
          WHERE t.merged_into IS NULL
            AND (t.search_name LIKE ?1 || '%' OR t.search_name LIKE '% ' || ?1 || '%'
                 OR t.search_name LIKE '%' || char(10) || ?1 || '%'
                 OR t.search_name LIKE ?3 || '%' OR t.search_name LIKE '% ' || ?3 || '%')
          ORDER BY EXISTS (SELECT 1 FROM library_entry l
                            WHERE l.title_id = t.id AND l.user_id = ?2) DESC,
                   t.tmdb_vote DESC
          LIMIT 8",
    )
    .bind(&pattern)
    .bind(user_id)
    .bind(&alternative)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter(|r| !(hide_animation && r.9.contains("Animation")))
        .map(
            |(id, kind, provider, key, name, poster_path, backdrop_path, date, vote, stored)| {
                let source = provider
                    .zip(key)
                    .and_then(|(p, k)| ExternalId::parse(&p, &k));
                let names: Vec<String> = serde_json::from_str(&stored).unwrap_or_default();
                SearchItem {
                    kind,
                    tmdb_id: source.as_ref().and_then(ExternalId::tmdb_id),
                    anilist_id: match source {
                        Some(ExternalId::AniList(a)) => Some(a),
                        _ => None,
                    },
                    id: Some(id),
                    name,
                    poster_path,
                    backdrop_path,
                    year: date
                        .as_deref()
                        .and_then(|d| d.get(..4))
                        .and_then(|y| y.parse().ok()),
                    overview: None,
                    vote,
                    popularity: None,
                    vote_count: None,
                    genre_ids: names.iter().flat_map(|n| genre_ids(n)).collect(),
                    in_library: None,
                }
            },
        )
        .collect())
}

/// Exact names first, wherever they came from; then what this Frog knows;
/// then the rest of TMDB's results, each title once.
fn merge_known(tmdb: Vec<SearchItem>, known: Vec<SearchItem>, term: &str) -> Vec<SearchItem> {
    let typed = text::words(term);
    let exact = |i: &SearchItem| text::closeness(&i.name, &typed) == Closeness::Exact;
    let mut out: Vec<SearchItem> = Vec::new();
    let push = |item: SearchItem, out: &mut Vec<SearchItem>| {
        if !out.iter().any(|o| o.same_as(&item)) {
            out.push(item);
        }
    };
    let (exact_known, other_known): (Vec<_>, Vec<_>) = known.into_iter().partition(|i| exact(i));
    let (exact_tmdb, other_tmdb): (Vec<_>, Vec<_>) = tmdb.into_iter().partition(|i| exact(i));
    for item in exact_tmdb
        .into_iter()
        .chain(exact_known)
        .chain(other_known)
        .chain(other_tmdb)
    {
        push(item, &mut out);
    }
    out
}

/// Put the thing you almost certainly meant at the top.
///
/// TMDB's own relevance ordering surfaces a lot of one-minute shorts and
/// unreleased fan projects that happen to share a word with the query, and
/// its popularity swings with the week. How closely the name answers comes
/// first (`text::closeness`); within that, popularity, damped by how few
/// people have rated it so obscure entries sink.
fn rank(items: &mut Vec<SearchItem>, term: &str, hide_animation: bool) {
    if hide_animation {
        items.retain(|i| !i.genre_ids.contains(&ANIMATION_GENRE));
    }
    let typed = text::words(term);
    items.sort_by(|a, b| score(b, &typed).total_cmp(&score(a, &typed)));
}

fn score(item: &SearchItem, typed: &[String]) -> f64 {
    let close = match text::closeness(&item.name, typed) {
        Closeness::Exact => 1000.0,
        Closeness::Starts => 300.0,
        Closeness::AllWords => 120.0,
        Closeness::Loose => 0.0,
    };

    let votes = item.vote_count.unwrap_or(0) as f64;
    // Fifty ratings is roughly where a title stops being a curiosity.
    let confidence = (votes / (votes + 50.0)).clamp(0.05, 1.0);
    let popularity = item.popularity.unwrap_or(0.0).min(500.0);

    // A result with no artwork at all is rarely the one meant, and a wall
    // that opens on empty cards looks broken; it goes below the rest.
    let art = if item.poster_path.is_some() {
        0.0
    } else if item.backdrop_path.is_some() {
        -150.0
    } else {
        -400.0
    };

    // Two titles with the same name: the one people have heard of. Ratings
    // say that more steadily than this week's popularity.
    close + 100.0 * confidence + popularity * confidence + art
}

fn collect(raw: &Value, force_kind: Option<&str>) -> Vec<SearchItem> {
    raw.get("results")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| SearchItem::from_value(v, force_kind))
                .collect()
        })
        .unwrap_or_default()
}

/// Mark results that are already tracked, so the grid can show their status
/// instead of offering to add them again.
pub async fn annotate_library(
    state: &AppState,
    user_id: i64,
    items: &mut [SearchItem],
) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }

    // Every provider id of every title in the library, with its status.
    let rows: Vec<(String, String, i64, String)> = sqlx::query_as(
        "SELECT x.provider, x.external_id, l.title_id, l.status
           FROM library_entry l
           JOIN title_external x ON x.title_id = l.title_id
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    for item in items.iter_mut() {
        let Some(source) = item.external() else {
            continue;
        };
        let (provider, key) = (source.provider(), source.key());
        if let Some((_, _, title_id, status)) =
            rows.iter().find(|(p, k, _, _)| p == provider && *k == key)
        {
            item.in_library = Some(status.clone());
            item.id = Some(*title_id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, name: &str, votes: i64, popularity: f64) -> SearchItem {
        SearchItem {
            kind: kind.into(),
            tmdb_id: Some(popularity as i64 + votes),
            anilist_id: None,
            id: None,
            name: name.into(),
            poster_path: Some("/p.jpg".into()),
            backdrop_path: None,
            year: Some(2021),
            overview: None,
            vote: Some(7.0),
            popularity: Some(popularity),
            vote_count: Some(votes),
            genre_ids: vec![35, 16],
            in_library: None,
        }
    }

    #[test]
    fn the_title_you_named_comes_before_what_is_merely_popular() {
        // TMDB's own answer to "Tom & Jerry", in TMDB's order.
        let mut found = vec![
            item("tv", "Tom & Jerry Kids Show", 139, 52.4),
            item("tv", "The Tom and Jerry Show", 132, 69.9),
            item("movie", "Tom & Jerry", 2640, 10.1),
            item("tv", "Tom and Jerry", 7, 3.0),
            item("tv", "Tom & Jerry", 1, 9.2),
            item("tv", "Tom and Jerry Tales", 155, 18.4),
        ];
        rank(&mut found, "Tom & Jerry", false);
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|i| (i.kind.as_str(), i.name.as_str()))
            .collect();
        assert_eq!(names[0], ("movie", "Tom & Jerry"));
        assert!(names[..3]
            .iter()
            .all(|(_, n)| n.replace('&', "and") == "Tom and Jerry"));
        assert_eq!(names[3].1, "Tom & Jerry Kids Show");
        assert_eq!(names[5].1, "The Tom and Jerry Show");
    }

    #[test]
    fn filters_combine_and_each_one_counts() {
        let filters = Filters {
            kind: Some("movie".into()),
            genres: vec!["Comedy".into(), "Horror".into()],
            from_year: Some(2020),
            to_year: Some(2022),
            min_score: Some(70.0),
            ..Filters::default()
        };
        let film = item("movie", "A film", 10, 1.0);
        assert!(filters.matches(&film));
        assert!(!filters.matches(&item("tv", "A show", 10, 1.0)));
        assert!(!filters.matches(&SearchItem {
            year: Some(2019),
            ..film.clone()
        }));
        assert!(!filters.matches(&SearchItem {
            year: None,
            ..film.clone()
        }));
        assert!(!filters.matches(&SearchItem {
            vote: Some(6.9),
            ..film.clone()
        }));
        assert!(!filters.matches(&SearchItem {
            genre_ids: vec![18],
            ..film
        }));
    }

    #[test]
    fn genres_use_the_id_each_list_gives_them() {
        let filters = Filters {
            genres: vec!["Science Fiction".into()],
            ..Filters::default()
        };
        let genres = |kind: &str| {
            filters
                .discover_params(kind, false)
                .into_iter()
                .find(|p| p.0 == "with_genres")
                .map(|p| p.1)
        };
        assert_eq!(genres("movie").as_deref(), Some("878"));
        assert_eq!(genres("tv").as_deref(), Some("10765"));
        assert!(filters
            .discover_params("tv", true)
            .contains(&("without_genres", "16".to_string())));
    }

    #[test]
    fn a_genre_only_one_side_has_leaves_the_other_side_out() {
        let talk = Filters {
            genres: vec!["Talk".into()],
            ..Filters::default()
        };
        assert!(talk.fits_kind("tv"));
        assert!(!talk.fits_kind("movie"));
        let horror_or_talk = Filters {
            genres: vec!["Horror".into(), "Talk".into()],
            ..Filters::default()
        };
        assert!(horror_or_talk.fits_kind("movie"));
        assert!(horror_or_talk.fits_kind("tv"));
        assert!(Filters::default().fits_kind("movie"));
    }

    #[test]
    fn a_status_filter_decides_where_the_search_looks() {
        let only = Filters {
            statuses: vec!["watching".into()],
            ..Filters::default()
        };
        assert!(only.library_only());
        let open = Filters {
            statuses: vec!["watching".into(), "none".into()],
            ..Filters::default()
        };
        assert!(!open.library_only());
        let mut result = item("tv", "Show", 1, 1.0);
        assert!(open.matches_status(&result));
        assert!(!only.matches_status(&result));
        result.in_library = Some("watching".into());
        assert!(only.matches_status(&result));
    }
}
