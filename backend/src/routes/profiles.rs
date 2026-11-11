//! Profiles: what another account on this instance is watching, for people who
//! chose to show it. Off by default, never public to the internet (every route
//! here needs a session), and a private profile answers exactly like a missing
//! one, so looking does not reveal who has an account.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::profiles::{self, CardRow};
use crate::routes::library::{to_map, ListQuery};
use crate::state::AppState;
use crate::stats::streaks;
use crate::store::{self, Page, SharedItem};
use crate::taste;

/// A page of someone's library, unless asked for less, and the most at once.
const SHARED_PAGE: i64 = 60;
const SHARED_MOST: i64 = 1000;

/// Accounts whose profile is on, plus your own.
pub async fn list(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let rows = profiles::listed(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({
        "profiles": rows.into_iter().map(|(username, display_name, me, avatar)| json!({
            "username": username, "display_name": display_name, "me": me, "avatar": avatar,
        })).collect::<Vec<_>>()
    })))
}

fn cards(rows: Vec<CardRow>) -> Vec<Value> {
    rows.into_iter()
        .map(|(kind, id, name, poster_path, rating)| {
            json!({ "id": id, "kind": kind, "name": name, "poster_path": poster_path, "rating": rating })
        })
        .collect()
}

/// What someone is watching, each show with where they are in it: how many
/// episodes of how many, and the one they are on. A manga they are reading
/// says how many chapters.
async fn watching_cards(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    rows: Vec<CardRow>,
) -> AppResult<Vec<Value>> {
    let mut out = Vec::with_capacity(rows.len());
    for (kind, id, name, poster_path, rating) in rows {
        let mut card = json!({ "id": id, "kind": kind, "name": name,
                               "poster_path": poster_path, "rating": rating });
        if kind == "tv" {
            let progress = store::progress(pool, user_id, id).await?;
            card["progress"] = json!({ "watched": progress.watched, "total": progress.total });
            if let Some(next) = store::next_episode(pool, user_id, id).await? {
                card["next"] = json!({ "season": next.season_number,
                                       "episode": next.episode_number,
                                       "name": next.name });
            }
        } else if kind == "manga" {
            let read = store::counted_progress(pool, user_id, id)
                .await?
                .unwrap_or(0);
            let total = profiles::chapter_count(pool, id).await?;
            card["progress"] = json!({ "watched": read, "total": total });
        }
        out.push(card);
    }
    Ok(out)
}

/// Everything someone has rated, best first, each show with its seasons'
/// ratings: what a profile's Ratings tab lists.
pub async fn ratings(
    auth: Auth,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> AppResult<Json<Value>> {
    let user_id = visible(&state, &auth, &username).await?.0;
    let pool = &state.pool;
    let (rows, seasons) = profiles::rated(pool, user_id).await?;
    let mut list: Vec<Value> = rows
        .into_iter()
        .map(|(kind, id, name, poster_path, status, rating)| {
            let parts: Vec<Value> = seasons
                .iter()
                .filter(|s| s.0 == id)
                .map(|s| json!({ "season": s.1, "first_episode": s.2, "rating": s.3 }))
                .collect();
            // A show rated only by its seasons stands at their average.
            let own = rating.or_else(|| {
                let r: Vec<f64> = seasons.iter().filter(|s| s.0 == id).map(|s| s.3).collect();
                (!r.is_empty()).then(|| r.iter().sum::<f64>() / r.len() as f64)
            });
            json!({ "id": id, "kind": kind, "name": name, "poster_path": poster_path,
                    "status": status, "rating": own, "seasons": parts })
        })
        .collect();
    list.sort_by(|a, b| {
        b["rating"]
            .as_f64()
            .unwrap_or(0.0)
            .total_cmp(&a["rating"].as_f64().unwrap_or(0.0))
            .then_with(|| {
                a["name"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(b["name"].as_str().unwrap_or_default())
            })
    });
    Ok(Json(json!({ "ratings": list })))
}

/// The viewer's library beside someone else's: what both have, with both
/// ratings, and what only one of them has.
pub async fn compare(
    auth: Auth,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> AppResult<Json<Value>> {
    let other = visible(&state, &auth, &username).await?.0;
    if other == auth.user_id() {
        return Err(AppError::BadRequest(
            "Compare with someone else's library.".into(),
        ));
    }
    let pool = &state.pool;
    let mine = taste::library_of(pool, auth.user_id()).await?;
    let theirs = taste::library_of(pool, other).await?;
    let compared = taste::compare(&mine, &theirs);

    let mut ids: Vec<i64> = mine.iter().chain(&theirs).map(|h| h.title_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let titles = profiles::titles_by_id(pool, &ids).await?;
    let card = |h: &taste::Held| {
        let (kind, name, poster) = titles.get(&h.title_id).cloned().unwrap_or_default();
        json!({ "id": h.title_id, "kind": kind, "name": name, "poster_path": poster,
                "status": h.status, "rating": h.rating })
    };
    Ok(Json(json!({
        "taste": taste::between(pool, auth.user_id(), other).await?,
        "both": compared.both.iter().map(|(m, t)| json!({
            "title": card(t), "mine": { "status": m.status, "rating": m.rating },
            "theirs": { "status": t.status, "rating": t.rating },
        })).collect::<Vec<_>>(),
        "only_theirs": compared.only_theirs.iter().map(card).collect::<Vec<_>>(),
        "only_mine": compared.only_mine.iter().map(card).collect::<Vec<_>>(),
    })))
}

/// Find an account whose profile this viewer may see.
async fn visible(state: &AppState, auth: &Auth, username: &str) -> AppResult<profiles::UserRow> {
    profiles::visible(&state.pool, auth.user_id(), username).await
}

pub async fn get(
    auth: Auth,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> AppResult<Json<Value>> {
    let (user_id, username, joined, public, display_name, bio, banner_backdrop, pins_json) =
        visible(&state, &auth, &username).await?;
    let pool = &state.pool;
    let facts = profiles::facts(pool, user_id, &pins_json).await?;
    let (films, episodes, shows, minutes) = facts.totals;
    let (in_library, completed, favourite_count, mean_rating) = facts.library;
    let (longest, current) = streaks(&facts.days, Utc::now().date_naive());
    let images = facts.images;
    let image_url = |kind: &str| {
        images
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, at)| store::picture_address(&username, kind, at))
    };

    // Past `visible`, so a private or switched-off account never gets this far,
    // and never of yourself: you would match perfectly and learn nothing.
    let taste = if user_id == auth.user_id() {
        None
    } else {
        taste::between(pool, auth.user_id(), user_id).await?
    };

    Ok(Json(json!({
        "username": username,
        "display_name": display_name,
        "bio": bio,
        "joined": joined,
        "public": public,
        "me": user_id == auth.user_id(),
        "taste": taste,
        "avatar": image_url("avatar"),
        "banner": image_url("banner"),
        "banner_backdrop": banner_backdrop,
        "banner_position": facts.banner_position,
        "stats": {
            "films": films,
            "episodes": episodes,
            "shows": shows,
            "chapters": facts.chapters,
            "minutes": minutes.round() as i64,
            "library": in_library,
            "completed": completed,
            "favourites": favourite_count,
            "mean_rating": mean_rating,
            "streak": current.days,
            "longest_streak": longest.days,
        },
        "genres": facts.genres.into_iter().map(|(name, titles)| json!({ "name": name, "titles": titles })).collect::<Vec<_>>(),
        "pinned": cards(facts.pinned),
        "watching": watching_cards(pool, user_id, facts.watching).await?,
        "favourites": cards(facts.favourites),
        "recent": cards(facts.recent),
    })))
}

/// Someone's whole library, read only, a page at a time: having the profile
/// on is the only switch, so `visible` answers for this too. It carries
/// `SharedItem`s, which have no notes to give away.
pub async fn library(
    auth: Auth,
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let owner = visible(&state, &auth, &username).await?;
    let (user_id, username, display_name) = (owner.0, owner.1, owner.4);
    let pool = &state.pool;
    let filter = q.filter()?;
    let page = Page {
        sort: q.sort.as_deref(),
        order: q.order.as_deref(),
        limit: q.limit.unwrap_or(SHARED_PAGE).clamp(1, SHARED_MOST),
        offset: q.offset.unwrap_or(0).max(0),
    };

    let rows = store::library_titles(pool, user_id, &filter, &page).await?;
    let items: Vec<SharedItem> = store::build_items(pool, user_id, rows)
        .await?
        .into_iter()
        .map(SharedItem::from)
        .collect();
    let counts = store::library_counts(pool, user_id, &filter).await?;
    let total: i64 = counts
        .iter()
        .filter(|(status, _)| filter.status.as_ref().is_none_or(|s| s == status))
        .map(|(_, n)| n)
        .sum();
    let genres = store::library_genres(pool, user_id).await?;

    Ok(Json(json!({
        "owner": { "username": username, "display_name": display_name },
        "items": items,
        "total": total,
        "counts": to_map(counts),
        "genres": genres,
    })))
}

/// A profile picture or banner, to whoever may see the profile.
pub async fn image(
    auth: Auth,
    State(state): State<AppState>,
    Path((username, kind)): Path<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let user = visible(&state, &auth, &username).await?;
    let row = profiles::picture(&state.pool, user.0, &kind).await?;
    let Some((mime, bytes, updated_at)) = row else {
        return Err(AppError::NotFound("no picture".into()));
    };
    let etag = format!("\"{}\"", updated_at.replace(['"', ' '], ""));
    let mut response = if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        bytes.into_response()
    };
    let h = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    // Private: these sit behind a sign-in and must never land in a shared cache.
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    Ok(response)
}
