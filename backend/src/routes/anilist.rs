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

/// (anilist name, expires, synced, error, summary)
type LinkRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub async fn status(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    if state.config.anilist.is_none() {
        return Ok(Json(json!({ "available": false })));
    }
    let link: Option<LinkRow> = sqlx::query_as(
        "SELECT anilist_name, expires_at, synced_at, last_error, last_summary
           FROM anilist_link WHERE user_id = ?",
    )
    .bind(auth.user_id())
    .fetch_optional(&state.pool)
    .await?;
    let Some((name, expires_at, synced_at, error, summary)) = link else {
        return Ok(Json(json!({ "available": true, "connected": false })));
    };
    let (unsure,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM anilist_entry WHERE user_id = ? AND state = 'unsure'")
            .bind(auth.user_id())
            .fetch_one(&state.pool)
            .await?;
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
            connected_at = excluded.connected_at, dirty = 1, last_error = NULL",
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

/// (media id, name, format, episodes, state, candidates, title name, poster,
///  kind, tmdb id, season, offset, AniList status, AniList progress)
type EntryRow = (
    i64,
    String,
    Option<String>,
    Option<i64>,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    i64,
    Option<String>,
    Option<i64>,
);

/// Every AniList entry and where it went, the unsure ones first.
pub async fn entries(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let rows: Vec<EntryRow> = sqlx::query_as(
        "SELECT a.media_id, a.name, a.format, a.episodes, a.state, a.candidates_json,
                t.name, t.poster_path, t.kind, t.tmdb_id, a.season, a.episode_offset,
                a.al_status, a.al_progress
           FROM anilist_entry a LEFT JOIN title t ON t.id = a.title_id
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
            json!({
                "media_id": r.0,
                "name": r.1,
                "format": r.2,
                "episodes": r.3,
                "state": r.4,
                "candidates": serde_json::from_str::<Value>(&r.5).unwrap_or(json!([])),
                "title": r.6.map(|name| json!({
                    "name": name,
                    "poster_path": r.7,
                    "kind": r.8,
                    "tmdb_id": r.9,
                    "season": r.10,
                    "from_episode": r.11 + 1,
                })),
                "status": r.12,
                "progress": r.13,
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
            "UPDATE anilist_entry SET state = 'ignored', updated_at = ?
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
    sqlx::query(
        "UPDATE anilist_entry
            SET state = 'placed', title_id = ?, season = ?, episode_offset = ?, detached = 0,
                al_status = NULL, al_progress = NULL, al_score = NULL,
                frog_status = NULL, frog_progress = NULL, frog_score = NULL, updated_at = ?
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
