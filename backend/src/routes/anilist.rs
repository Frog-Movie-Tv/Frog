//! Connecting AniList, and the choices a sync leaves for the person.

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use axum::Json;
use chrono::Utc;
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::anilist::{self, records, Api};
use crate::auth::Auth;
use crate::catalog::{self, ExternalId};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn status(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    if state.config.anilist.is_none() {
        return Ok(Json(json!({ "available": false })));
    }
    let Some(link) = records::link_status(&state.pool, auth.user_id()).await? else {
        return Ok(Json(json!({ "available": true, "connected": false })));
    };
    // When the worker will look next: after the wait for a failure, or one
    // interval after the last sync that finished.
    let next_sync_at = if link.needs_reconnect {
        None
    } else {
        let after_last = link
            .synced_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| (t + chrono::Duration::minutes(anilist::sync_minutes())).to_rfc3339());
        match (link.retry_at.clone(), after_last) {
            (Some(r), Some(a)) => Some(r.max(a)),
            (r, a) => r.or(a),
        }
    };
    Ok(Json(json!({
        "available": true,
        "connected": true,
        "name": link.name,
        "expires_at": link.expires_at,
        "synced_at": link.synced_at,
        "syncing": anilist::is_syncing(auth.user_id()),
        "error": link.last_error,
        "summary": link.last_summary.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        "unsure": link.unsure,
        "attempted_at": link.attempted_at,
        "next_sync_at": next_sync_at,
        "needs_reconnect": link.needs_reconnect,
        "failures": link.failures,
        "interval_minutes": anilist::sync_minutes(),
    })))
}

/// Start AniList's sign-in. The state ties the answer to this account.
pub async fn connect(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let api = Api::new(&state)?;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    records::start_sign_in(&state.pool, auth.user_id(), &token).await?;
    Ok(Json(json!({ "url": api.authorize_url(&token) })))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

/// Where AniList sends the browser back. Always ends on Settings, with the
/// outcome in the address for the page to say.
pub async fn callback(
    auth: Result<Auth, AppError>,
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> impl IntoResponse {
    let back = |outcome: &str| Redirect::to(&format!("/settings?anilist={outcome}#connections"));
    let Ok(auth) = auth else {
        return back("signed-out");
    };
    match finish(&auth, &state, q).await {
        Ok(()) => {
            sync_soon(&state, auth.user_id());
            back("connected")
        }
        Err(e) => {
            tracing::warn!(error = %e, "AniList sign-in did not finish");
            back("failed")
        }
    }
}

async fn finish(auth: &Auth, state: &AppState, q: CallbackQuery) -> AppResult<()> {
    let (Some(code), Some(nonce)) = (q.code, q.state) else {
        return Err(AppError::BadRequest("AniList sent nothing back".into()));
    };
    if !records::claim_sign_in(&state.pool, auth.user_id(), &nonce).await? {
        return Err(AppError::BadRequest(
            "that sign-in was not started here".into(),
        ));
    }

    let api = Api::new(state)?;
    let (token, seconds) = api.exchange(&code).await?;
    let (anilist_id, name) = api.viewer(&token).await?;
    let expires_at = (Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339();
    records::save_link(
        &state.pool,
        auth.user_id(),
        anilist_id,
        &name,
        &token,
        &expires_at,
    )
    .await
}

/// Sync in the background; the outcome is recorded on the link, where
/// Settings reads it.
fn sync_soon(state: &AppState, user_id: i64) {
    let worker = state.clone();
    tokio::spawn(async move {
        let _ = anilist::sync_user(&worker, user_id).await;
    });
}

pub async fn sync_now(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    Api::new(&state)?;
    if !records::is_connected(&state.pool, auth.user_id()).await? {
        return Err(AppError::BadRequest("Connect AniList first.".into()));
    }
    records::look_again_now(&state.pool, auth.user_id()).await?;
    sync_soon(&state, auth.user_id());
    Ok(Json(json!({ "syncing": true })))
}

/// Forget the connection and every placement. Nothing on AniList or in the
/// Frog library changes.
pub async fn disconnect(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    records::disconnect(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Every AniList entry and where it went, the unsure ones first.
pub async fn entries(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let rows = records::listing(&state.pool, auth.user_id()).await?;
    let entries: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let outcome = r.outcome.unwrap_or_else(|| match r.state.as_str() {
                "placed" => "matched".to_string(),
                _ => "not_confident".to_string(),
            });
            json!({
                "media_id": r.media_id,
                "name": r.name,
                "format": r.format,
                "episodes": r.episodes,
                "state": r.state,
                "candidates": serde_json::from_str::<Value>(&r.candidates_json).unwrap_or(json!([])),
                "title": r.title_name.map(|name| json!({
                    "id": r.title_id,
                    "tmdb_id": r.title_tmdb.as_deref()
                        .and_then(|k| ExternalId::parse("tmdb", k))
                        .and_then(|x| x.tmdb_id()),
                    "name": name,
                    "poster_path": r.poster_path,
                    "kind": r.kind,
                    "season": r.season,
                    "from_episode": r.episode_offset + 1,
                })),
                "status": r.al_status,
                "progress": r.al_progress,
                // matched, not_confident, no_candidate, upcoming, apply_failed
                "outcome": outcome,
                "method": if r.manual { Some("your choice".to_string()) } else { r.method },
                "confidence": r.confidence,
                "note": r.note,
                "manual": r.manual,
                "media_status": r.media_status,
                "start_date": r.start_date,
                // No longer on the AniList list; kept in Frog as it is.
                "gone": r.gone_at.is_some(),
                // When Frog looks for its place again.
                "next_try_at": r.next_try_at,
                // In the library as it is on AniList, until it is placed.
                "kept": r.kept_id.zip(r.kept_kind).map(|(id, kind)| json!({
                    "id": id,
                    "kind": kind,
                })),
            })
        })
        .collect();
    Ok(Json(json!({ "entries": entries })))
}

#[derive(Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub ignore: bool,
    pub kind: Option<String>,
    pub tmdb_id: Option<i64>,
    pub season: Option<i64>,
    /// 1-based, as people count episodes.
    pub from_episode: Option<i64>,
}

/// Say where an entry belongs (or that it does not belong in Frog). The next
/// sync treats it as new, so nothing on either side is lost.
pub async fn choose(
    auth: Auth,
    State(state): State<AppState>,
    Path(media_id): Path<i64>,
    Json(choice): Json<Choice>,
) -> AppResult<Json<Value>> {
    if !records::entry_exists(&state.pool, auth.user_id(), media_id).await? {
        return Err(AppError::NotFound("no such AniList entry".into()));
    }

    if choice.ignore {
        records::ignore(&state.pool, auth.user_id(), media_id).await?;
        return Ok(Json(json!({ "ok": true })));
    }

    let (Some(kind), Some(tmdb_id)) = (choice.kind.as_deref(), choice.tmdb_id) else {
        return Err(AppError::BadRequest("choose a title".into()));
    };
    if kind != "movie" && kind != "tv" {
        return Err(AppError::BadRequest("That isn't a film or a show.".into()));
    }
    let season = match kind {
        "tv" => Some(choice.season.unwrap_or(1).max(0)),
        _ => None,
    };
    let offset = (choice.from_episode.unwrap_or(1) - 1).max(0);
    let (title_id, _) = catalog::ensure(&state, &ExternalId::tmdb(kind, tmdb_id)).await?;
    // Moving an entry: what its old place got from AniList goes with it.
    anilist::forget_place(&state, auth.user_id(), media_id).await?;
    records::place_by_hand(
        &state.pool,
        auth.user_id(),
        media_id,
        title_id,
        season,
        offset,
    )
    .await?;
    anilist::touch(&state.pool, auth.user_id()).await;
    Ok(Json(json!({ "ok": true })))
}
