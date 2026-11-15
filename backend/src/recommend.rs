//! A title one person suggests to another on the same Frog.
//!
//! Only accounts whose profile the sender may see can be chosen, and only
//! those that still want recommendations (`inbox::Wants`). Recommending the
//! same title to the same person again refreshes the one row and tells them
//! afresh, rather than adding a second. Each recommendation is the
//! recipient's to answer: added, or dismissed, and undone back to neither.

use chrono::Utc;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::inbox::{self, Event};
use crate::state::AppState;

/// The longest note a recommendation carries, in characters.
pub const NOTE_MAX: usize = 280;

/// Someone who can be chosen in a people picker.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Person {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: bool,
    /// Already recommended this title, by this viewer.
    pub sent: bool,
}

/// (id, username, display name, has a picture, sent, how recently these two last swapped one)
type PersonRow = (i64, String, Option<String>, bool, bool, Option<String>);

/// Everyone `viewer` may name: enabled, profile on, and, per `kind`, still
/// wanting recommendations or "watched with". People the viewer has lately
/// swapped recommendations with come first.
pub async fn people(
    pool: &SqlitePool,
    viewer: i64,
    title_id: Option<i64>,
    kind: &str,
) -> AppResult<Vec<Person>> {
    let rows: Vec<PersonRow> = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name,
                EXISTS (SELECT 1 FROM user_image i WHERE i.user_id = u.id AND i.kind = 'avatar'),
                EXISTS (SELECT 1 FROM recommendation r
                         WHERE r.from_user_id = ?1 AND r.to_user_id = u.id AND r.title_id = ?2),
                (SELECT MAX(r.created_at) FROM recommendation r
                  WHERE (r.from_user_id = ?1 AND r.to_user_id = u.id)
                     OR (r.from_user_id = u.id AND r.to_user_id = ?1))
           FROM app_user u
          WHERE u.disabled = 0 AND u.profile_public = 1 AND u.id != ?1
          ORDER BY 6 IS NULL, 6 DESC, u.username COLLATE NOCASE",
    )
    .bind(viewer)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, username, display_name, avatar, sent, _) in rows {
        if !inbox::wants(pool, id).await?.allows(kind) {
            continue;
        }
        out.push(Person {
            id,
            username,
            display_name,
            avatar,
            sent,
        });
    }
    Ok(out)
}

/// The note as kept: trimmed, empty as none, refused past `NOTE_MAX`.
pub fn clean_note(note: Option<&str>) -> AppResult<Option<String>> {
    let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    if note.chars().count() > NOTE_MAX {
        return Err(AppError::BadRequest(format!(
            "a note can be {NOTE_MAX} characters at most"
        )));
    }
    Ok(Some(note.to_string()))
}

/// Recommend a title to these people. Anyone not in `people` for the sender
/// is refused before anything is written. Says how many were told.
pub async fn send(
    state: &AppState,
    from: i64,
    title_id: i64,
    to: &[i64],
    note: Option<&str>,
) -> AppResult<usize> {
    let pool = &state.pool;
    if to.is_empty() {
        return Err(AppError::BadRequest(
            "choose someone to recommend it to".into(),
        ));
    }
    let note = clean_note(note)?;
    let allowed = people(pool, from, Some(title_id), inbox::RECOMMENDATION).await?;
    if to.iter().any(|id| !allowed.iter().any(|p| p.id == *id)) {
        return Err(AppError::BadRequest(
            "one of those people cannot be sent recommendations".into(),
        ));
    }
    let title = crate::catalog::get_title(pool, title_id).await?;
    let (sender,): (String,) = sqlx::query_as(
        "SELECT COALESCE(NULLIF(TRIM(display_name), ''), username) FROM app_user WHERE id = ?",
    )
    .bind(from)
    .fetch_one(pool)
    .await?;
    let now = Utc::now().to_rfc3339();
    let mut told = 0;
    let mut seen = std::collections::HashSet::new();
    for &to_user in to {
        if !seen.insert(to_user) {
            continue;
        }
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO recommendation (title_id, from_user_id, to_user_id, note, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (title_id, from_user_id, to_user_id) DO UPDATE SET
                note = excluded.note, created_at = excluded.created_at,
                seen_at = NULL, answered = NULL, answered_at = NULL
             RETURNING id",
        )
        .bind(title_id)
        .bind(from)
        .bind(to_user)
        .bind(&note)
        .bind(&now)
        .fetch_one(pool)
        .await?;
        // Sent again: told afresh, at the top, not twice.
        let key = format!("rec:{id}");
        inbox::forget(pool, to_user, &key).await?;
        let body = match &note {
            Some(n) => format!("{sender} recommends it · {n}"),
            None => format!("{sender} recommends it"),
        };
        inbox::tell(
            state,
            to_user,
            &Event {
                kind: inbox::RECOMMENDATION,
                title_id: Some(title_id),
                actor_id: Some(from),
                payload: json!({ "recommendation_id": id, "note": note }),
                ref_key: Some(key),
            },
            Some(json!({
                "title": title.name,
                "body": body,
                "url": "/notifications",
                "tag": format!("frog-rec-{id}"),
            })),
        )
        .await?;
        told += 1;
    }
    Ok(told)
}

/// (id, created_at, note, answered, title id, name, kind, poster, year,
///  from username, from display name, from has a picture, your status)
type ForYouRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    bool,
    Option<String>,
);

/// Recommendations to one account that it has not dismissed, newest first.
pub async fn for_me(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Value>> {
    let rows: Vec<ForYouRow> = sqlx::query_as(
        "SELECT r.id, r.created_at, r.note, r.answered,
                t.id, t.name, t.kind, t.poster_path, substr(t.release_date, 1, 4),
                u.username, u.display_name,
                EXISTS (SELECT 1 FROM user_image i WHERE i.user_id = u.id AND i.kind = 'avatar'),
                (SELECT l.status FROM library_entry l
                  WHERE l.user_id = r.to_user_id AND l.title_id = t.id)
           FROM recommendation r
           JOIN title t ON t.id = r.title_id
           JOIN app_user u ON u.id = r.from_user_id AND u.disabled = 0
          WHERE r.to_user_id = ? AND (r.answered IS NULL OR r.answered != 'dismissed')
          ORDER BY r.created_at DESC, r.id DESC
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                created_at,
                note,
                answered,
                title_id,
                name,
                kind,
                poster,
                year,
                username,
                display_name,
                avatar,
                status,
            )| {
                json!({
                    "id": id,
                    "created_at": created_at,
                    "note": note,
                    "answered": answered,
                    "title": { "id": title_id, "name": name, "kind": kind,
                               "poster_path": poster, "year": year, "status": status },
                    "from": { "username": username, "display_name": display_name,
                              "avatar": avatar },
                })
            },
        )
        .collect())
}

/// Whether this account has ever been recommended anything it still sees.
pub async fn has_any(pool: &SqlitePool, user_id: i64) -> AppResult<bool> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM recommendation
          WHERE to_user_id = ? AND (answered IS NULL OR answered != 'dismissed')",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

/// The recipient answers: `added`, `dismissed`, or `None` to take an answer
/// back. Adding tells the sender, quietly, once.
pub async fn answer(
    pool: &SqlitePool,
    user_id: i64,
    id: i64,
    answer: Option<&str>,
) -> AppResult<()> {
    if let Some(a) = answer {
        if a != "added" && a != "dismissed" {
            return Err(AppError::BadRequest(format!("unknown answer {a}")));
        }
    }
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT from_user_id, title_id FROM recommendation WHERE id = ? AND to_user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some((from, title_id)) = row else {
        return Err(AppError::NotFound("no such recommendation".into()));
    };
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE recommendation SET answered = ?, answered_at = ?, seen_at = COALESCE(seen_at, ?)
          WHERE id = ? AND to_user_id = ?",
    )
    .bind(answer)
    .bind(answer.map(|_| now.clone()))
    .bind(&now)
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    if answer == Some("added") {
        inbox::record(
            pool,
            from,
            &Event {
                kind: inbox::ADDED,
                title_id: Some(title_id),
                actor_id: Some(user_id),
                payload: json!({}),
                ref_key: Some(format!("added:{id}")),
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
    fn a_note_is_trimmed_empty_is_none_and_long_is_refused() {
        assert_eq!(
            clean_note(Some("  watch it  ")).unwrap().as_deref(),
            Some("watch it")
        );
        assert_eq!(clean_note(Some("   ")).unwrap(), None);
        assert_eq!(clean_note(None).unwrap(), None);
        assert!(
            clean_note(Some(&"é".repeat(NOTE_MAX))).is_ok(),
            "characters, not bytes"
        );
        assert!(clean_note(Some(&"a".repeat(NOTE_MAX + 1))).is_err());
    }
}
