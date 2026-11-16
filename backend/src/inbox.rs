//! What each account is told inside Frog: the Notifications page and the
//! bell's count.
//!
//! Every event is written here at the moment Frog decides to tell someone,
//! the same moment a push goes out, so the page works for anyone who never
//! switched push on and nothing is told twice (`ref_key`). Which kinds an
//! account wants is one small setting (`app_setting` key `notify_kinds`),
//! and a kind switched off is neither written here nor pushed.

use crate::clock::Stamp;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::AppResult;

/// How long a notification is kept.
pub const KEEP_DAYS: i64 = 90;

/// `app_setting` key holding which kinds the account wants.
const KINDS_KEY: &str = "notify_kinds";

/// A recommendation to you.
pub const RECOMMENDATION: &str = "recommendation";
/// Someone added a title you recommended to them. Quiet: never pushed.
pub const ADDED: &str = "added";
/// New episodes of a show you follow.
pub const EPISODE: &str = "episode";
/// A film you follow, out today.
pub const FILM: &str = "film";
/// A badge given to you.
pub const BADGE: &str = "badge";
/// Frog itself was updated.
pub const UPDATE: &str = "update";
/// Something went wrong on the server. Administrators only.
pub const OPS: &str = "ops";
/// Someone says you watched something with them.
pub const TOGETHER: &str = "together";

/// The kinds an account can switch off, each covering push and the page.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Wants {
    /// Off also takes the account out of everyone's Recommend list.
    pub recommendations: bool,
    /// New episodes and films.
    pub releases: bool,
    pub badges: bool,
    pub updates: bool,
    /// Off also takes the account out of everyone's "Watched with" list.
    pub together: bool,
}

impl Default for Wants {
    fn default() -> Self {
        Self {
            recommendations: true,
            releases: true,
            badges: true,
            updates: true,
            together: true,
        }
    }
}

impl Wants {
    /// Whether this account wants to hear about a kind at all.
    pub fn allows(&self, kind: &str) -> bool {
        match kind {
            RECOMMENDATION | ADDED => self.recommendations,
            EPISODE | FILM => self.releases,
            BADGE => self.badges,
            UPDATE => self.updates,
            TOGETHER => self.together,
            _ => true,
        }
    }
}

/// What the account wants, every kind on until it says otherwise.
pub async fn wants(pool: &SqlitePool, user_id: i64) -> AppResult<Wants> {
    let raw: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = ?")
            .bind(user_id)
            .bind(KINDS_KEY)
            .fetch_optional(pool)
            .await?;
    // A field missing from an older save keeps its default.
    let mut out = serde_json::to_value(Wants::default()).unwrap_or(Value::Null);
    if let Some(saved) = raw.and_then(|(v,)| serde_json::from_str::<Value>(&v).ok()) {
        if let (Some(base), Some(saved)) = (out.as_object_mut(), saved.as_object()) {
            for (k, v) in saved {
                if base.contains_key(k) && v.is_boolean() {
                    base.insert(k.clone(), v.clone());
                }
            }
        }
    }
    Ok(serde_json::from_value(out).unwrap_or_default())
}

pub async fn set_wants(pool: &SqlitePool, user_id: i64, wants: &Wants) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_setting (user_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, key) DO UPDATE SET
            value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(KINDS_KEY)
    .bind(serde_json::to_string(wants).unwrap_or_default())
    .bind(Utc::now().stamp())
    .execute(pool)
    .await?;
    Ok(())
}

/// One thing to tell one account.
#[derive(Debug, Default, Clone)]
pub struct Event<'a> {
    pub kind: &'a str,
    pub title_id: Option<i64>,
    pub actor_id: Option<i64>,
    pub payload: Value,
    /// Whatever makes this event itself: the same key is written once.
    pub ref_key: Option<String>,
}

/// Write an event to one account's page, when it wants that kind. Says
/// whether a row was written: false for a kind switched off or an event
/// already there.
pub async fn record(pool: &SqlitePool, user_id: i64, event: &Event<'_>) -> AppResult<bool> {
    if !wants(pool, user_id).await?.allows(event.kind) {
        return Ok(false);
    }
    let written = sqlx::query(
        "INSERT OR IGNORE INTO notification
            (user_id, kind, title_id, actor_id, payload, ref_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(event.kind)
    .bind(event.title_id)
    .bind(event.actor_id)
    .bind(event.payload.to_string())
    .bind(&event.ref_key)
    .bind(Utc::now().stamp())
    .execute(pool)
    .await?
    .rows_affected();
    Ok(written > 0)
}

/// Write an event and, when it is new and the account wants it, push it to
/// every browser the account subscribed. The push goes behind the request:
/// a slow push service never holds up whoever caused it.
pub async fn tell(
    state: &crate::state::AppState,
    user_id: i64,
    event: &Event<'_>,
    push: Option<Value>,
) -> AppResult<bool> {
    let written = record(&state.pool, user_id, event).await?;
    if let (true, Some(payload)) = (written, push) {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::push::broadcast(&state, user_id, &payload).await {
                tracing::warn!(error = %e, "a notification could not be pushed");
            }
        });
    }
    Ok(written)
}

/// A push's picture, as the service worker fetches it: the wide one, else
/// the poster.
pub fn picture(wide: Option<&str>, poster: Option<&str>) -> Option<String> {
    wide.or(poster).map(|p| format!("/img/w780{p}"))
}

/// Take one event back off an account's page, by its key: a recommendation
/// sent again is told afresh, at the top, rather than twice.
pub async fn forget(pool: &SqlitePool, user_id: i64, ref_key: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM notification WHERE user_id = ? AND ref_key = ?")
        .bind(user_id)
        .bind(ref_key)
        .execute(pool)
        .await?;
    Ok(())
}

/// (id, kind, created_at, read_at, payload, title id, title name, title kind,
///  poster, actor username, actor display name, actor has a picture,
///  the title's status in this library)
type FeedRow = (
    i64,
    String,
    String,
    Option<String>,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<bool>,
    Option<String>,
);

/// The newest page of an account's notifications, before `before` (an id)
/// when given.
pub async fn list(
    pool: &SqlitePool,
    user_id: i64,
    before: Option<i64>,
    limit: i64,
) -> AppResult<Vec<Value>> {
    let rows: Vec<FeedRow> = sqlx::query_as(
        "SELECT n.id, n.kind, n.created_at, n.read_at, n.payload,
                t.id, t.name, t.kind, t.poster_path,
                u.username, u.display_name,
                CASE WHEN u.id IS NULL THEN NULL ELSE EXISTS (
                    SELECT 1 FROM user_image i WHERE i.user_id = u.id AND i.kind = 'avatar')
                END,
                (SELECT l.status FROM library_entry l
                  WHERE l.user_id = n.user_id AND l.title_id = n.title_id)
           FROM notification n
           LEFT JOIN title t ON t.id = n.title_id
           LEFT JOIN app_user u ON u.id = n.actor_id AND u.disabled = 0
          WHERE n.user_id = ?1 AND n.removed_at IS NULL AND (?2 IS NULL OR n.id < ?2)
          ORDER BY n.id DESC
          LIMIT ?3",
    )
    .bind(user_id)
    .bind(before)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                kind,
                created_at,
                read_at,
                payload,
                title_id,
                title_name,
                title_kind,
                poster,
                username,
                display_name,
                avatar,
                status,
            )| {
                json!({
                    "id": id,
                    "kind": kind,
                    "created_at": created_at,
                    "read": read_at.is_some(),
                    "payload": serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null),
                    "title": title_id.map(|id| json!({
                        "id": id, "name": title_name, "kind": title_kind, "poster_path": poster,
                        "status": status,
                    })),
                    "actor": username.map(|username| json!({
                        "username": username,
                        "display_name": display_name,
                        "avatar": avatar.unwrap_or(false),
                    })),
                })
            },
        )
        .collect())
}

/// How many of an account's notifications it has not seen.
pub async fn unread(pool: &SqlitePool, user_id: i64) -> AppResult<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notification
              WHERE user_id = ? AND read_at IS NULL AND removed_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Mark these as read, or every one when `ids` is empty.
pub async fn mark_read(pool: &SqlitePool, user_id: i64, ids: &[i64]) -> AppResult<()> {
    let now = Utc::now().stamp();
    if ids.is_empty() {
        sqlx::query("UPDATE notification SET read_at = ? WHERE user_id = ? AND read_at IS NULL")
            .bind(&now)
            .bind(user_id)
            .execute(pool)
            .await?;
        return Ok(());
    }
    for id in ids {
        sqlx::query(
            "UPDATE notification SET read_at = ?
              WHERE user_id = ? AND id = ? AND read_at IS NULL",
        )
        .bind(&now)
        .bind(user_id)
        .bind(id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Take notifications off an account's page: one by id, or every one when
/// `id` is `None`. Says which went, for Undo. The rows stay, hidden, so the
/// same event is never written there again; a recommendation stays
/// answerable, since that lives on the recommendation itself.
pub async fn remove(pool: &SqlitePool, user_id: i64, id: Option<i64>) -> AppResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "UPDATE notification SET removed_at = ?1
          WHERE user_id = ?2 AND removed_at IS NULL AND (?3 IS NULL OR id = ?3)
          RETURNING id",
    )
    .bind(Utc::now().stamp())
    .bind(user_id)
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut ids: Vec<i64> = rows.into_iter().map(|r| r.0).collect();
    ids.sort_unstable();
    Ok(ids)
}

/// Undo of `remove`: these are back on the page as they were.
pub async fn restore(pool: &SqlitePool, user_id: i64, ids: &[i64]) -> AppResult<()> {
    sqlx::query(
        "UPDATE notification SET removed_at = NULL
          WHERE user_id = ? AND id IN (SELECT value FROM json_each(?))",
    )
    .bind(user_id)
    .bind(json!(ids).to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Housekeeping: notifications older than `KEEP_DAYS` go, whoever's they are.
pub async fn trim(pool: &SqlitePool) -> AppResult<u64> {
    let cutoff = (Utc::now() - Duration::days(KEEP_DAYS)).stamp();
    Ok(sqlx::query("DELETE FROM notification WHERE created_at < ?")
        .bind(cutoff)
        .execute(pool)
        .await?
        .rows_affected())
}

/// Tell every account that was here before it that Frog is now `version`,
/// once per version. An account made today learns about the next one.
pub async fn note_update(pool: &SqlitePool, version: &str) -> AppResult<()> {
    let since = (Utc::now() - Duration::days(1)).stamp();
    let accounts: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM app_user WHERE disabled = 0 AND created_at < ?")
            .bind(since)
            .fetch_all(pool)
            .await?;
    for (user_id,) in accounts {
        record(
            pool,
            user_id,
            &Event {
                kind: UPDATE,
                payload: json!({ "version": version }),
                ref_key: Some(format!("update:{version}")),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_switch_covers_its_kinds_and_nothing_else() {
        let off = Wants {
            releases: false,
            ..Wants::default()
        };
        assert!(!off.allows(EPISODE));
        assert!(!off.allows(FILM));
        assert!(off.allows(RECOMMENDATION));
        assert!(off.allows(OPS), "administrators always hear of trouble");
        let quiet = Wants {
            recommendations: false,
            ..Wants::default()
        };
        assert!(
            !quiet.allows(ADDED),
            "a recommender's quiet line goes with it"
        );
    }
}
