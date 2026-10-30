//! Everything Frog stores about AniList, per account: the connection
//! (`anilist_link`), a sign-in in progress (`anilist_oauth`), and each list
//! entry with where it was placed and both sides as of the last sync
//! (`anilist_entry`). Every statement names the account it is about.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde_json::Value;
use sqlx::SqlitePool;

use super::client::{Remote, View};
use super::place::{Target, RESOLVER};
use crate::catalog::ExternalId;
use crate::error::AppResult;

// --- the connection --------------------------------------------------------

// --- entries -----------------------------------------------------------------

/// Entries gone from the AniList list are marked, and ones back unmarked.
/// Nothing in the library goes with them. A list that comes back empty when
/// Frog knows entries is more likely a hiccup than a cleared list, so then
/// nothing is marked at all.
pub(super) async fn mark_gone(
    pool: &SqlitePool,
    user_id: i64,
    remotes: &[Remote],
    known: &HashMap<i64, Known>,
) -> AppResult<i64> {
    let present: HashSet<i64> = remotes.iter().map(|r| r.media_id).collect();
    let now = Utc::now().to_rfc3339();
    for (media_id, k) in known {
        if k.gone && present.contains(media_id) {
            sqlx::query(
                "UPDATE anilist_entry SET gone_at = NULL WHERE user_id = ? AND media_id = ?",
            )
            .bind(user_id)
            .bind(media_id)
            .execute(pool)
            .await?;
        }
    }
    if remotes.is_empty() && !known.is_empty() {
        tracing::warn!(
            user_id,
            known = known.len(),
            "AniList returned an empty list; nothing marked as removed"
        );
        return Ok(0);
    }
    let mut removed = 0;
    for (media_id, k) in known {
        if !k.gone && !present.contains(media_id) {
            sqlx::query("UPDATE anilist_entry SET gone_at = ? WHERE user_id = ? AND media_id = ?")
                .bind(&now)
                .bind(user_id)
                .bind(media_id)
                .execute(pool)
                .await?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// One stored entry, as the sync reads it back.
#[derive(sqlx::FromRow)]
struct EntryRow {
    media_id: i64,
    state: String,
    title_id: Option<i64>,
    /// The placed title's TMDB id, as `title_external` holds it.
    tmdb_key: Option<String>,
    season: Option<i64>,
    episode_offset: i64,
    al_status: Option<String>,
    al_progress: Option<i64>,
    al_score: Option<i64>,
    frog_status: Option<String>,
    frog_progress: Option<i64>,
    frog_score: Option<i64>,
    detached: bool,
    manual: bool,
    resolver: i64,
    episodes: Option<i64>,
    format: Option<String>,
    media_status: Option<String>,
    start_date: Option<String>,
    tries: i64,
    next_try_at: Option<String>,
    gone_at: Option<String>,
}

pub(super) struct Known {
    pub state: String,
    pub title_id: Option<i64>,
    pub target: Option<Target>,
    pub al: Option<View>,
    pub frog: Option<View>,
    pub detached: bool,
    /// Chosen by the person: kept, whatever the matcher thinks.
    pub manual: bool,
    pub resolver: i64,
    /// What AniList said about the media last time, to notice a change.
    pub meta: (Option<i64>, Option<String>, Option<String>, Option<String>),
    pub tries: i64,
    pub next_try_at: Option<String>,
    pub gone: bool,
}

impl Known {
    /// Not placed, tried recently, and nothing on AniList's side has changed
    /// since: looking again now would find what it found then.
    pub fn wait_to_place(&self, remote: &Remote, now: &str) -> bool {
        self.state == "unsure"
            && self.next_try_at.as_deref().is_some_and(|t| t > now)
            && self.meta
                == (
                    remote.episodes,
                    remote.format.clone(),
                    remote.media_status.clone(),
                    remote.start.clone(),
                )
    }
}

fn snapshot(status: Option<String>, progress: Option<i64>, score: Option<i64>) -> Option<View> {
    Some(View {
        status: status?,
        progress: progress.unwrap_or(0),
        score: score.unwrap_or(0),
    })
}

pub(super) async fn known_entries(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<HashMap<i64, Known>> {
    let rows: Vec<EntryRow> = sqlx::query_as(
        "SELECT a.media_id, a.state, a.title_id, x.external_id AS tmdb_key, a.season,
                a.episode_offset, a.al_status, a.al_progress, a.al_score,
                a.frog_status, a.frog_progress, a.frog_score, a.detached, a.manual, a.resolver,
                a.episodes, a.format, a.media_status, a.start_date, a.tries, a.next_try_at,
                a.gone_at
           FROM anilist_entry a
           LEFT JOIN title_external x
                  ON x.title_id = a.title_id AND x.provider = 'tmdb' AND x.is_primary = 1
          WHERE a.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::new();
    for r in rows {
        let tmdb = r
            .tmdb_key
            .as_deref()
            .and_then(|k| ExternalId::parse("tmdb", k));
        let target = match tmdb {
            Some(ExternalId::Tmdb { kind, id }) if kind == "movie" => Some(Target::Movie(id)),
            Some(ExternalId::Tmdb { id, .. }) => r.season.map(|season| Target::Show {
                tmdb_id: id,
                season,
                offset: r.episode_offset,
            }),
            _ => None,
        };
        out.insert(
            r.media_id,
            Known {
                state: r.state,
                title_id: r.title_id,
                target,
                al: snapshot(r.al_status, r.al_progress, r.al_score),
                frog: snapshot(r.frog_status, r.frog_progress, r.frog_score),
                detached: r.detached,
                manual: r.manual,
                resolver: r.resolver,
                meta: (r.episodes, r.format, r.media_status, r.start_date),
                tries: r.tries,
                next_try_at: r.next_try_at,
                gone: r.gone_at.is_some(),
            },
        );
    }
    Ok(out)
}

/// How an entry was (or was not) placed, as Settings shows it.
pub(super) struct Verdict<'a> {
    pub state: &'a str,
    pub outcome: &'a str,
    pub method: Option<&'a str>,
    pub confidence: i64,
    pub note: Option<String>,
}

pub(super) async fn save_entry(
    pool: &SqlitePool,
    user_id: i64,
    remote: &Remote,
    verdict: &Verdict<'_>,
    placed: Option<(i64, Target)>,
    candidates: Option<&[Value]>,
) -> AppResult<()> {
    let (title_id, season, offset) = match placed {
        Some((id, Target::Show { season, offset, .. })) => (Some(id), Some(season), offset),
        Some((id, Target::Movie(_))) => (Some(id), None, 0),
        None => (None, None, 0),
    };
    let next_airing_at = remote
        .next_airing
        .and_then(|(_, at)| chrono::DateTime::from_timestamp(at, 0))
        .map(|t| t.to_rfc3339());
    sqlx::query(
        "INSERT INTO anilist_entry (user_id, media_id, name, format, episodes, state, title_id,
                                    season, episode_offset, candidates_json, updated_at,
                                    outcome, method, confidence, note, resolver, media_status,
                                    start_date, next_airing_at, next_airing_episode)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (user_id, media_id) DO UPDATE SET
            name = excluded.name, format = excluded.format, episodes = excluded.episodes,
            state = excluded.state, title_id = excluded.title_id, season = excluded.season,
            episode_offset = excluded.episode_offset,
            candidates_json = CASE WHEN excluded.candidates_json = '[]'
                                   THEN anilist_entry.candidates_json
                                   ELSE excluded.candidates_json END,
            updated_at = excluded.updated_at, outcome = excluded.outcome,
            method = excluded.method, confidence = excluded.confidence, note = excluded.note,
            resolver = excluded.resolver, media_status = excluded.media_status,
            start_date = excluded.start_date, next_airing_at = excluded.next_airing_at,
            next_airing_episode = excluded.next_airing_episode,
            tries = CASE WHEN excluded.state = 'placed' THEN 0 ELSE anilist_entry.tries END,
            next_try_at = CASE WHEN excluded.state = 'placed' THEN NULL
                               ELSE anilist_entry.next_try_at END",
    )
    .bind(user_id)
    .bind(remote.media_id)
    .bind(&remote.name)
    .bind(&remote.format)
    .bind(remote.episodes)
    .bind(verdict.state)
    .bind(title_id)
    .bind(season)
    .bind(offset)
    .bind(serde_json::to_string(&candidates.unwrap_or(&[]))?)
    .bind(Utc::now().to_rfc3339())
    .bind(verdict.outcome)
    .bind(verdict.method)
    .bind(verdict.confidence)
    .bind(&verdict.note)
    .bind(RESOLVER)
    .bind(&remote.media_status)
    .bind(&remote.start)
    .bind(next_airing_at)
    .bind(remote.next_airing.map(|(ep, _)| ep))
    .execute(pool)
    .await?;
    Ok(())
}

/// What both sides looked like at the end of this sync; `detached` leaves
/// the entry alone from now on (taken out of the library in Frog).
pub(super) async fn save_snapshots(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    al: &View,
    frog: Option<&View>,
    detached: bool,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry
            SET al_status = ?, al_progress = ?, al_score = ?,
                frog_status = ?, frog_progress = ?, frog_score = ?,
                detached = ?, updated_at = ?
          WHERE user_id = ? AND media_id = ?",
    )
    .bind(&al.status)
    .bind(al.progress)
    .bind(al.score)
    .bind(frog.map(|f| f.status.clone()))
    .bind(frog.map(|f| f.progress))
    .bind(frog.map(|f| f.score))
    .bind(detached)
    .bind(Utc::now().to_rfc3339())
    .bind(user_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Accounts whose list is due: changed in Frog, never synced, or idle long
/// enough to look again, and not waiting out a failure. It all lives in the
/// database, so a restart picks up where the last run left off. Expired or
/// refused tokens are skipped; Settings asks for a reconnect instead.
pub(super) async fn due(pool: &SqlitePool, idle_minutes: i64) -> AppResult<Vec<i64>> {
    let idle = (Utc::now() - chrono::Duration::minutes(idle_minutes)).to_rfc3339();
    let settle = (Utc::now() - chrono::Duration::seconds(45)).to_rfc3339();
    let now = Utc::now().to_rfc3339();
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM anilist_link
          WHERE expires_at > ?1 AND needs_reconnect = 0
            AND (retry_at IS NULL OR retry_at <= ?1)
            AND (synced_at IS NULL OR synced_at < ?2 OR (dirty = 1 AND synced_at < ?3))
          ORDER BY COALESCE(synced_at, '')",
    )
    .bind(now)
    .bind(idle)
    .bind(settle)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// The AniList account and token a sync reads with.
pub(super) async fn link_token(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<Option<(i64, String)>> {
    Ok(
        sqlx::query_as("SELECT anilist_user_id, token FROM anilist_link WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn is_connected(pool: &SqlitePool, user_id: i64) -> AppResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM anilist_link WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

/// The connection as Settings shows it.
pub struct LinkStatus {
    pub name: String,
    pub expires_at: String,
    pub synced_at: Option<String>,
    pub last_error: Option<String>,
    pub last_summary: Option<String>,
    pub attempted_at: Option<String>,
    pub retry_at: Option<String>,
    pub needs_reconnect: bool,
    pub failures: i64,
    /// Entries waiting for the person to say where they go.
    pub unsure: i64,
}

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

pub async fn link_status(pool: &SqlitePool, user_id: i64) -> AppResult<Option<LinkStatus>> {
    let link: Option<LinkRow> = sqlx::query_as(
        "SELECT anilist_name, expires_at, synced_at, last_error, last_summary, attempted_at,
                retry_at, needs_reconnect, failures
           FROM anilist_link WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some((
        name,
        expires_at,
        synced_at,
        last_error,
        last_summary,
        attempted_at,
        retry_at,
        needs_reconnect,
        failures,
    )) = link
    else {
        return Ok(None);
    };
    let (unsure,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM anilist_entry WHERE user_id = ? AND state = 'unsure'")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(Some(LinkStatus {
        name,
        expires_at,
        synced_at,
        last_error,
        last_summary,
        attempted_at,
        retry_at,
        needs_reconnect,
        failures,
        unsure,
    }))
}

/// A sign-in has fifteen minutes to come back.
fn sign_in_cutoff() -> String {
    (Utc::now() - chrono::Duration::minutes(15)).to_rfc3339()
}

/// Remember a sign-in just started, dropping this account's earlier ones and
/// anyone's that expired.
pub async fn start_sign_in(pool: &SqlitePool, user_id: i64, nonce: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM anilist_oauth WHERE user_id = ? OR created_at < ?")
        .bind(user_id)
        .bind(sign_in_cutoff())
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO anilist_oauth (state, user_id, created_at) VALUES (?, ?, ?)")
        .bind(nonce)
        .bind(user_id)
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

/// Use up a sign-in this account started, if it did and it has not expired.
pub async fn claim_sign_in(pool: &SqlitePool, user_id: i64, nonce: &str) -> AppResult<bool> {
    let claimed =
        sqlx::query("DELETE FROM anilist_oauth WHERE state = ? AND user_id = ? AND created_at > ?")
            .bind(nonce)
            .bind(user_id)
            .bind(sign_in_cutoff())
            .execute(pool)
            .await?
            .rows_affected();
    Ok(claimed > 0)
}

/// Keep a finished sign-in. A different AniList account than before means
/// none of the old entries apply, so they go.
pub async fn save_link(
    pool: &SqlitePool,
    user_id: i64,
    anilist_user_id: i64,
    name: &str,
    token: &str,
    expires_at: &str,
) -> AppResult<()> {
    let previous: Option<(i64,)> =
        sqlx::query_as("SELECT anilist_user_id FROM anilist_link WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    if previous.is_some_and(|(id,)| id != anilist_user_id) {
        sqlx::query("DELETE FROM anilist_entry WHERE user_id = ?")
            .bind(user_id)
            .execute(pool)
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
    .bind(user_id)
    .bind(anilist_user_id)
    .bind(name)
    .bind(token)
    .bind(expires_at)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Forget the connection and every placement. Nothing on AniList or in the
/// Frog library changes.
pub async fn disconnect(pool: &SqlitePool, user_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM anilist_entry WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM anilist_link WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A sync that finished: the checkpoint Settings shows and the worker counts
/// from, with every sign of trouble cleared.
pub(super) async fn sync_succeeded(
    pool: &SqlitePool,
    user_id: i64,
    at: &str,
    summary_json: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_link SET synced_at = ?, attempted_at = ?, dirty = 0,
                last_error = NULL, failures = 0, retry_at = NULL, needs_reconnect = 0,
                last_summary = ? WHERE user_id = ?",
    )
    .bind(at)
    .bind(at)
    .bind(summary_json)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// How many syncs in a row have failed for this account.
pub(super) async fn failures(pool: &SqlitePool, user_id: i64) -> AppResult<i64> {
    let (failures,): (i64,) = sqlx::query_as("SELECT failures FROM anilist_link WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or((0,));
    Ok(failures)
}

/// A sync that failed. `synced_at` stays where it was.
pub(super) async fn sync_failed(
    pool: &SqlitePool,
    user_id: i64,
    at: &str,
    message: &str,
    retry_at: &str,
    signed_out: bool,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_link SET attempted_at = ?, last_error = ?, failures = failures + 1,
                retry_at = ?, needs_reconnect = ? WHERE user_id = ?",
    )
    .bind(at)
    .bind(message)
    .bind(retry_at)
    .bind(signed_out)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

// --- one entry, as a sync and Settings change it ------------------------------

/// The show an unplaced entry belongs to, when that much is known.
pub(super) async fn set_show(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    title_id: i64,
) -> AppResult<()> {
    sqlx::query("UPDATE anilist_entry SET title_id = ? WHERE user_id = ? AND media_id = ?")
        .bind(title_id)
        .bind(user_id)
        .bind(media_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// One more miss for an unplaced entry, and when to look again.
pub(super) async fn schedule_look(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    tries: i64,
    next_try_at: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry SET tries = ?, next_try_at = ? WHERE user_id = ? AND media_id = ?",
    )
    .bind(tries)
    .bind(next_try_at)
    .bind(user_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// A kept placement: its verdict stays, only what AniList says of the media
/// is brought up to date.
pub(super) async fn refresh_kept(
    pool: &SqlitePool,
    user_id: i64,
    remote: &Remote,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry SET media_status = ?, start_date = ?, episodes = ?,
                updated_at = ?, tries = 0, next_try_at = NULL
          WHERE user_id = ? AND media_id = ?",
    )
    .bind(&remote.media_status)
    .bind(&remote.start)
    .bind(remote.episodes)
    .bind(Utc::now().to_rfc3339())
    .bind(user_id)
    .bind(remote.media_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Placed, but writing it failed: Settings says so, and the entry is not
/// remembered as synced (resolver 0), so the next sync tries again.
pub(super) async fn apply_failed(pool: &SqlitePool, user_id: i64, media_id: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry SET outcome = 'apply_failed', note = ?, resolver = 0
          WHERE user_id = ? AND media_id = ?",
    )
    .bind("Placed, but Frog could not write it. It tries again at the next sync.")
    .bind(user_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Frog's side of an entry once its show is settled.
pub(super) async fn save_frog_snapshot(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    frog: Option<&View>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry SET frog_status = ?, frog_progress = ?, frog_score = ?
          WHERE user_id = ? AND media_id = ?",
    )
    .bind(frog.map(|f| f.status.clone()))
    .bind(frog.map(|f| f.progress))
    .bind(frog.map(|f| f.score))
    .bind(user_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every entry placed in a show and still synced: (season, offset, AniList
/// status, media id). Specials are season 0.
pub(super) async fn placed_in(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Vec<(i64, i64, String, i64)>> {
    Ok(sqlx::query_as(
        "SELECT COALESCE(season, 0), episode_offset, COALESCE(al_status, 'PLANNING'), media_id
           FROM anilist_entry
          WHERE user_id = ? AND title_id = ? AND state = 'placed' AND detached = 0",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?)
}

/// The other entries placed in the same show: (media id, season, offset,
/// episodes). With `regular_only`, specials are left out.
pub(super) async fn others_in(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    media_id: i64,
    regular_only: bool,
) -> AppResult<Vec<(i64, i64, i64, Option<i64>)>> {
    let sql = if regular_only {
        "SELECT media_id, season, episode_offset, episodes FROM anilist_entry
          WHERE user_id = ? AND title_id = ? AND state = 'placed' AND season > 0
            AND media_id != ?"
    } else {
        "SELECT media_id, season, episode_offset, episodes FROM anilist_entry
          WHERE user_id = ? AND title_id = ? AND state = 'placed' AND season IS NOT NULL
            AND media_id != ?"
    };
    Ok(sqlx::query_as(sql)
        .bind(user_id)
        .bind(title_id)
        .bind(media_id)
        .fetch_all(pool)
        .await?)
}

/// Where an entry is placed now: (title, season, offset, episodes, the
/// title's TMDB id as `title_external` holds it).
pub(super) type Placed = (Option<i64>, Option<i64>, i64, Option<i64>, Option<String>);

pub(super) async fn placement(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
) -> AppResult<Option<Placed>> {
    Ok(sqlx::query_as(
        "SELECT a.title_id, a.season, a.episode_offset, a.episodes, x.external_id
           FROM anilist_entry a
           LEFT JOIN title_external x
                  ON x.title_id = a.title_id AND x.provider = 'tmdb' AND x.is_primary = 1
          WHERE a.user_id = ? AND a.media_id = ? AND a.state = 'placed'",
    )
    .bind(user_id)
    .bind(media_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn entry_exists(pool: &SqlitePool, user_id: i64, media_id: i64) -> AppResult<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM anilist_entry WHERE user_id = ? AND media_id = ?")
            .bind(user_id)
            .bind(media_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

/// The person says an entry does not belong in Frog.
pub async fn ignore(pool: &SqlitePool, user_id: i64, media_id: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry SET state = 'ignored', manual = 1, updated_at = ?
          WHERE user_id = ? AND media_id = ?",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(user_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// The person says where an entry goes. Both snapshots are cleared, so the
/// next sync treats it as new and nothing on either side is lost.
pub async fn place_by_hand(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    title_id: i64,
    season: Option<i64>,
    offset: i64,
) -> AppResult<()> {
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
    .bind(user_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Asked for by hand: entries waiting for their next look get it now.
pub async fn look_again_now(pool: &SqlitePool, user_id: i64) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry SET next_try_at = NULL WHERE user_id = ? AND state = 'unsure'",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// One AniList entry and where it went, as Settings lists it.
#[derive(sqlx::FromRow)]
pub struct Listed {
    pub media_id: i64,
    pub name: String,
    pub format: Option<String>,
    pub episodes: Option<i64>,
    pub state: String,
    pub candidates_json: String,
    pub title_name: Option<String>,
    pub poster_path: Option<String>,
    pub kind: Option<String>,
    /// Frog's id for the title it is placed in.
    pub title_id: Option<i64>,
    /// That title's TMDB id, as `title_external` holds it: placing is in
    /// TMDB's terms.
    pub title_tmdb: Option<String>,
    pub season: Option<i64>,
    pub episode_offset: i64,
    pub al_status: Option<String>,
    pub al_progress: Option<i64>,
    pub outcome: Option<String>,
    pub method: Option<String>,
    pub confidence: Option<i64>,
    pub note: Option<String>,
    pub manual: bool,
    pub media_status: Option<String>,
    pub start_date: Option<String>,
    pub gone_at: Option<String>,
    pub next_try_at: Option<String>,
    /// The AniList-only title it is kept as, when this account keeps it.
    pub kept_id: Option<i64>,
    pub kept_kind: Option<String>,
}

/// Every entry, the unsure ones first.
pub async fn listing(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Listed>> {
    Ok(sqlx::query_as(
        "SELECT a.media_id, a.name, a.format, a.episodes, a.state, a.candidates_json,
                t.name AS title_name, t.poster_path, t.kind, t.id AS title_id,
                tx.external_id AS title_tmdb, a.season,
                a.episode_offset, a.al_status, a.al_progress, a.outcome, a.method,
                a.confidence, a.note,
                a.manual, a.media_status, a.start_date, a.gone_at, a.next_try_at,
                k.id AS kept_id, k.kind AS kept_kind
           FROM anilist_entry a LEFT JOIN title t ON t.id = a.title_id
           LEFT JOIN title_external tx
                  ON tx.title_id = t.id AND tx.provider = 'tmdb' AND tx.is_primary = 1
           LEFT JOIN title_external kx
                  ON kx.provider = 'anilist' AND kx.external_id = CAST(a.media_id AS TEXT)
                 AND kx.is_primary = 1
           LEFT JOIN title k ON k.id = kx.title_id
                AND EXISTS (SELECT 1 FROM library_entry l
                             WHERE l.user_id = a.user_id AND l.title_id = k.id)
          WHERE a.user_id = ?
          ORDER BY CASE a.state WHEN 'unsure' THEN 0 WHEN 'placed' THEN 1 ELSE 2 END,
                   a.name COLLATE NOCASE",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}
