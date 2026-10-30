use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::auth::Auth;
use crate::catalog::ExternalId;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::models::SearchItem;

/// TMDB's genre id for Animation, shared by films and shows.
const ANIMATION_GENRE: i64 = 16;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "first_page")]
    pub page: u32,
    /// Leave animation out of the results.
    #[serde(default)]
    pub hide_animation: bool,
}

fn first_page() -> u32 {
    1
}

pub async fn search(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let term = q.q.trim();
    if term.is_empty() {
        return Err(AppError::BadRequest(
            "give me something to search for".into(),
        ));
    }

    let page = q.page.clamp(1, 500);
    let raw = state.tmdb.search_multi(term, page).await?;
    let mut items = collect(&raw, None);
    rank(&mut items, term, q.hide_animation);
    if page == 1 {
        let known = known_titles(&state, auth.user_id(), term, q.hide_animation).await?;
        items = merge_known(items, known, term);
    }
    annotate_library(&state, auth.user_id(), &mut items).await?;

    Ok(Json(serde_json::json!({
        "page": raw.get("page").and_then(Value::as_i64).unwrap_or(1),
        "total_pages": raw.get("total_pages").and_then(Value::as_i64).unwrap_or(1),
        "results": items,
    })))
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
/// should. Your own library first.
async fn known_titles(
    state: &AppState,
    user_id: i64,
    term: &str,
    hide_animation: bool,
) -> AppResult<Vec<SearchItem>> {
    if term.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let pattern = term.replace(['\\', '%', '_'], "");
    let rows: Vec<KnownRow> = sqlx::query_as(
        "SELECT t.id, t.kind, x.provider, x.external_id, t.name, t.poster_path,
                t.backdrop_path, t.release_date, t.tmdb_vote, t.genres_json
           FROM title t
           LEFT JOIN title_external x ON x.title_id = t.id AND x.is_primary = 1
          WHERE t.merged_into IS NULL
            AND (t.name LIKE ?1 || '%' OR t.name LIKE '% ' || ?1 || '%'
                 OR t.original_name LIKE ?1 || '%')
          ORDER BY EXISTS (SELECT 1 FROM library_entry l
                            WHERE l.title_id = t.id AND l.user_id = ?2) DESC,
                   t.tmdb_vote DESC
          LIMIT 8",
    )
    .bind(&pattern)
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter(|r| !(hide_animation && r.9.contains("Animation")))
        .map(
            |(id, kind, provider, key, name, poster_path, backdrop_path, date, vote, _)| {
                let source = provider
                    .zip(key)
                    .and_then(|(p, k)| ExternalId::parse(&p, &k));
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
                    genre_ids: Vec::new(),
                    in_library: None,
                }
            },
        )
        .collect())
}

/// Exact names first, wherever they came from; then what this Frog knows;
/// then the rest of TMDB's results, each title once.
fn merge_known(tmdb: Vec<SearchItem>, known: Vec<SearchItem>, term: &str) -> Vec<SearchItem> {
    let needle = term.trim().to_lowercase();
    let exact = |i: &SearchItem| i.name.to_lowercase() == needle;
    let mut out: Vec<SearchItem> = Vec::new();
    let push = |item: SearchItem, out: &mut Vec<SearchItem>| {
        if !out.iter().any(|o| o.same_as(&item)) {
            out.push(item);
        }
    };
    let (exact_known, other_known): (Vec<_>, Vec<_>) = known.into_iter().partition(|i| exact(i));
    let (exact_tmdb, other_tmdb): (Vec<_>, Vec<_>) = tmdb.into_iter().partition(|i| exact(i));
    for item in exact_known
        .into_iter()
        .chain(exact_tmdb)
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
/// unreleased fan projects that happen to share a word with the query. An
/// exact title match wins outright; after that it is popularity, damped by how
/// few people have rated it so obscure entries sink.
fn rank(items: &mut Vec<SearchItem>, term: &str, hide_animation: bool) {
    if hide_animation {
        items.retain(|i| !i.genre_ids.contains(&ANIMATION_GENRE));
    }

    let needle = term.trim().to_lowercase();

    items.sort_by(|a, b| score(b, &needle).total_cmp(&score(a, &needle)));
}

fn score(item: &SearchItem, needle: &str) -> f64 {
    let name = item.name.to_lowercase();
    let exact = if name == needle { 1000.0 } else { 0.0 };
    let starts = if name.starts_with(needle) { 250.0 } else { 0.0 };

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

    exact + starts + popularity * confidence + art
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
