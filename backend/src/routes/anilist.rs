//! Connecting AniList, and the choices a sync leaves for the person.

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use axum::Json;
use chrono::Utc;
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::anilist::{self, Api};
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store;

/// (anilist name, expires, synced, error, summary, attempted, retry at,
///  needs reconnect, failures)
type LinkRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    i64,
);

pub async fn status(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    if state.config.anilist.is_none() {
        return Ok(Json(json!({ "available": false })));
    }
    let link: Option<LinkRow> = sqlx::query_as(
        "SELECT anilist_name, expires_at, synced_at, last_error, last_summary, attempted_at,
                retry_at, needs_reconnect, failures
           FROM anilist_link WHERE user_id = ?",
    )
    .bind(auth.user_id())
    .fetch_optional(&state.pool)
    .await?;
    let Some((
        name,
        expires_at,
        synced_at,
        error,
        summary,
        attempted_at,
        retry_at,
        needs_reconnect,
        failures,
    )) = link
    else {
        return Ok(Json(json!({ "available": true, "connected": false })));
    };
    let (unsure,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM anilist_entry WHERE user_id = ? AND state = 'unsure'")
            .bind(auth.user_id())
            .fetch_one(&state.pool)
            .await?;
    // When the worker will look next: after the wait for a failure, or one
    // interval after the last sync that finished.
    let next_sync_at = if needs_reconnect {
        None
    } else {
        let after_last = synced_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| (t + chrono::Duration::minutes(anilist::sync_minutes())).to_rfc3339());
        match (retry_at.clone(), after_last) {
            (Some(r), Some(a)) => Some(r.max(a)),
            (r, a) => r.or(a),
        }
    };
    Ok(Json(json!({
        "available": true,
        "connected": true,
        "name": name,
        "expires_at": expires_at,
        "synced_at": synced_at,
        "syncing": anilist::is_syncing(auth.user_id()),
        "error": error,
        "summary": summary.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        "unsure": unsure,
        "attempted_at": attempted_at,
        "next_sync_at": next_sync_at,
        "needs_reconnect": needs_reconnect,
        "failures": failures,
        "interval_minutes": anilist::sync_minutes(),
    })))
}

/// Start AniList's sign-in. The state ties the answer to this account.
pub async fn connect(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let api = Api::new(&state)?;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    sqlx::query("DELETE FROM anilist_oauth WHERE user_id = ? OR created_at < ?")
        .bind(auth.user_id())
        .bind((Utc::now() - chrono::Duration::minutes(15)).to_rfc3339())
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO anilist_oauth (state, user_id, created_at) VALUES (?, ?, ?)")
        .bind(&token)
        .bind(auth.user_id())
        .bind(Utc::now().to_rfc3339())
        .execute(&state.pool)
        .await?;
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
            let worker = state.clone();
            let user_id = auth.user_id();
            tokio::spawn(async move {
                let _ = anilist::sync_user(&worker, user_id).await;
            });
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
    let claimed =
        sqlx::query("DELETE FROM anilist_oauth WHERE state = ? AND user_id = ? AND created_at > ?")
            .bind(&nonce)
            .bind(auth.user_id())
            .bind((Utc::now() - chrono::Duration::minutes(15)).to_rfc3339())
            .execute(&state.pool)
            .await?
            .rows_affected();
    if claimed == 0 {
        return Err(AppError::BadRequest(
            "that sign-in was not started here".into(),
        ));
    }

    let api = Api::new(state)?;
    let (token, seconds) = api.exchange(&code).await?;
    let (anilist_id, name) = api.viewer(&token).await?;
    let now = Utc::now();
    // A different AniList account means none of the old entries apply.
    let previous: Option<(i64,)> =
        sqlx::query_as("SELECT anilist_user_id FROM anilist_link WHERE user_id = ?")
            .bind(auth.user_id())
            .fetch_optional(&state.pool)
            .await?;
    if previous.is_some_and(|(id,)| id != anilist_id) {
        sqlx::query("DELETE FROM anilist_entry WHERE user_id = ?")
            .bind(auth.user_id())
            .execute(&state.pool)
            .await?;
    }
    sqlx::query(
        "INSERT INTO anilist_link (user_id, anilist_user_id, anilist_name, token, expires_at,
                                   connected_at, dirty)
         VALUES (?, ?, ?, ?, ?, ?, 1)
         ON CONFLICT (user_id) DO UPDATE SET
            anilist_user_id = excluded.anilist_user_id, anilist_name = excluded.anilist_name,
            token = excluded.token, expires_at = excluded.expires_at,
            connected_at = excluded.connected_at, dirty = 1, last_error = NULL,
            needs_reconnect = 0, failures = 0, retry_at = NULL",
    )
    .bind(auth.user_id())
    .bind(anilist_id)
    .bind(&name)
    .bind(&token)
    .bind((now + chrono::Duration::seconds(seconds)).to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(())
}

pub async fn sync_now(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    Api::new(&state)?;
    let connected: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM anilist_link WHERE user_id = ?")
        .bind(auth.user_id())
        .fetch_optional(&state.pool)
        .await?;
    if connected.is_none() {
        return Err(AppError::BadRequest("Connect AniList first.".into()));
    }
    // Asked for by hand: entries waiting for their next look get it now.
    sqlx::query(
        "UPDATE anilist_entry SET next_try_at = NULL WHERE user_id = ? AND state = 'unsure'",
    )
    .bind(auth.user_id())
    .execute(&state.pool)
    .await?;
    let worker = state.clone();
    let user_id = auth.user_id();
    tokio::spawn(async move {
        let _ = anilist::sync_user(&worker, user_id).await;
    });
    Ok(Json(json!({ "syncing": true })))
}

/// Forget the connection and every placement. Nothing on AniList or in the
/// Frog library changes.
pub async fn disconnect(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM anilist_entry WHERE user_id = ?")
        .bind(auth.user_id())
        .execute(&state.pool)
        .await?;
    sqlx::query("DELETE FROM anilist_link WHERE user_id = ?")
        .bind(auth.user_id())
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// One AniList entry and where it went, as the entries list reads it.
#[derive(sqlx::FromRow)]
struct EntryRow {
    media_id: i64,
    name: String,
    format: Option<String>,
    episodes: Option<i64>,
    state: String,
    candidates_json: String,
    title_name: Option<String>,
    poster_path: Option<String>,
    kind: Option<String>,
    tmdb_id: Option<i64>,
    season: Option<i64>,
    episode_offset: i64,
    al_status: Option<String>,
    al_progress: Option<i64>,
    outcome: Option<String>,
    method: Option<String>,
    confidence: Option<i64>,
    note: Option<String>,
    manual: bool,
    media_status: Option<String>,
    start_date: Option<String>,
    gone_at: Option<String>,
    next_try_at: Option<String>,
    local_kind: Option<String>,
    local_tmdb_id: Option<i64>,
}

/// Every AniList entry and where it went, the unsure ones first.
pub async fn entries(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let rows: Vec<EntryRow> = sqlx::query_as(
        "SELECT a.media_id, a.name, a.format, a.episodes, a.state, a.candidates_json,
                t.name AS title_name, t.poster_path, t.kind, t.tmdb_id, a.season,
                a.episode_offset, a.al_status, a.al_progress, a.outcome, a.method,
                a.confidence, a.note,
                a.manual, a.media_status, a.start_date, a.gone_at, a.next_try_at,
                k.kind AS local_kind, k.tmdb_id AS local_tmdb_id
           FROM anilist_entry a LEFT JOIN title t ON t.id = a.title_id
           LEFT JOIN title k ON k.anilist_id = a.media_id AND k.tmdb_id < 0
                AND EXISTS (SELECT 1 FROM library_entry l
                             WHERE l.user_id = a.user_id AND l.title_id = k.id)
          WHERE a.user_id = ?
          ORDER BY CASE a.state WHEN 'unsure' THEN 0 WHEN 'placed' THEN 1 ELSE 2 END,
                   a.name COLLATE NOCASE",
    )
    .bind(auth.user_id())
    .fetch_all(&state.pool)
    .await?;
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
                    "name": name,
                    "poster_path": r.poster_path,
                    "kind": r.kind,
                    "tmdb_id": r.tmdb_id,
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
                "kept": r.local_kind.zip(r.local_tmdb_id).map(|(kind, tmdb_id)| json!({
                    "kind": kind,
                    "tmdb_id": tmdb_id,
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
    let exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM anilist_entry WHERE user_id = ? AND media_id = ?")
            .bind(auth.user_id())
            .bind(media_id)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("no such AniList entry".into()));
    }

    if choice.ignore {
        sqlx::query(
            "UPDATE anilist_entry SET state = 'ignored', manual = 1, updated_at = ?
              WHERE user_id = ? AND media_id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(auth.user_id())
        .bind(media_id)
        .execute(&state.pool)
        .await?;
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
    let (title_id, _) = store::ensure_title(&state, kind, tmdb_id).await?;
    // Moving an entry: what its old place got from AniList goes with it.
    anilist::forget_place(&state, auth.user_id(), media_id).await?;
    sqlx::query(
        "UPDATE anilist_entry
            SET state = 'placed', title_id = ?, season = ?, episode_offset = ?, detached = 0,
                al_status = NULL, al_progress = NULL, al_score = NULL,
                frog_status = NULL, frog_progress = NULL, frog_score = NULL, updated_at = ?,
                manual = 1, outcome = 'matched', method = NULL, confidence = 100, note = NULL
          WHERE user_id = ? AND media_id = ?",
    )
    .bind(title_id)
    .bind(season)
    .bind(offset)
    .bind(Utc::now().to_rfc3339())
    .bind(auth.user_id())
    .bind(media_id)
    .execute(&state.pool)
    .await?;
    anilist::touch(&state.pool, auth.user_id()).await;
    Ok(Json(json!({ "ok": true })))
}
