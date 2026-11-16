//! Who you watched something with, naming other accounts on this Frog.
//!
//! It is always optional and only ever written on your own watches
//! (`watch_with`). The people named are told on their Notifications page,
//! one line per show while they have not answered it, however many episodes
//! and evenings it grows to, and choose for themselves: Add to my diary
//! copies those watches into their own account as their own rows, each on
//! its own day, and Not me takes their name off all of them. An answered
//! line stays, and the next watch starts a new one. Nothing is written into
//! another account without them, which is the rule every personal row keeps.
//!
//! A show can be watched with someone over weeks (`watching_with`): every
//! episode ticked from then on names them, from the page, a whole season or
//! the browser extension alike.

use chrono::Utc;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::inbox::{self, Event};
use crate::state::AppState;

/// The newest watch this account has, so the watches a request makes can be
/// told apart from those it found.
pub async fn newest_watch(pool: &SqlitePool, user_id: i64) -> AppResult<i64> {
    let (id,): (Option<i64>,) = sqlx::query_as("SELECT MAX(id) FROM watch_event WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(id.unwrap_or(0))
}

/// The people `user_id` may name: their profile on, still letting people say
/// so (`inbox::Wants::together`). Anyone else asked for is refused.
pub async fn allowed(pool: &SqlitePool, user_id: i64, with: &[i64]) -> AppResult<Vec<i64>> {
    if with.is_empty() {
        return Ok(Vec::new());
    }
    let people = crate::recommend::people(pool, user_id, None, inbox::TOGETHER).await?;
    let mut out = Vec::new();
    for id in with {
        if !people.iter().any(|p| p.id == *id) {
            return Err(AppError::BadRequest(
                "one of those people cannot be named on a watch".into(),
            ));
        }
        if !out.contains(id) {
            out.push(*id);
        }
    }
    Ok(out)
}

/// Who a show is being watched with now.
pub async fn default_for(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT w.with_user_id FROM watching_with w
           JOIN app_user u ON u.id = w.with_user_id AND u.disabled = 0
          WHERE w.user_id = ? AND w.title_id = ?
          ORDER BY w.created_at",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Watch a show with these people from now on; nobody clears it.
pub async fn set_default(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    with: &[i64],
) -> AppResult<Vec<i64>> {
    let with = allowed(pool, user_id, with).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM watching_with WHERE user_id = ? AND title_id = ?")
        .bind(user_id)
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    let now = Utc::now().to_rfc3339();
    for id in &with {
        sqlx::query(
            "INSERT INTO watching_with (user_id, title_id, with_user_id, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(with)
}

/// Name these people on the watches this request just made of these titles
/// (every watch after `since`), and tell them. With `with` as `None`, each
/// show's own "watching with" is used.
pub async fn after(
    state: &AppState,
    user_id: i64,
    since: i64,
    title_ids: &[i64],
    with: Option<&[i64]>,
) -> AppResult<()> {
    let explicit = match with {
        Some(w) => Some(allowed(&state.pool, user_id, w).await?),
        None => None,
    };
    for &title_id in title_ids {
        let people = match &explicit {
            Some(w) => w.clone(),
            None => default_for(&state.pool, user_id, title_id).await?,
        };
        if people.is_empty() {
            continue;
        }
        let made: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM watch_event WHERE user_id = ? AND title_id = ? AND id > ? ORDER BY id",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(since)
        .fetch_all(&state.pool)
        .await?;
        let ids: Vec<i64> = made.into_iter().map(|r| r.0).collect();
        tag(state, user_id, &ids, &people).await?;
    }
    Ok(())
}

/// Name `people` on these watches of the account's own, and tell each one.
pub async fn tag(
    state: &AppState,
    user_id: i64,
    watch_ids: &[i64],
    people: &[i64],
) -> AppResult<()> {
    if watch_ids.is_empty() || people.is_empty() {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    for &watch in watch_ids {
        for &person in people {
            sqlx::query(
                "INSERT OR IGNORE INTO watch_with (watch_event_id, user_id, with_user_id, created_at)
                 SELECT id, user_id, ?, ? FROM watch_event WHERE id = ? AND user_id = ?",
            )
            .bind(person)
            .bind(&now)
            .bind(watch)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
        }
    }
    for &person in people {
        tell(state, user_id, person, watch_ids).await?;
    }
    Ok(())
}

/// What a "watched with you" line says: a film or one episode by the title,
/// a whole season by its number, anything more by how many episodes.
pub fn line_text(who: &str, name: &str, episodes: usize, whole_season: Option<i64>) -> String {
    match (episodes, whole_season) {
        (0 | 1, _) => format!("{who} watched {name} with you"),
        (_, Some(season)) => format!("{who} watched season {season} of {name} with you"),
        (n, None) => format!("{who} watched {n} episodes of {name} with you"),
    }
}

/// (watch id, its day, episode id, season number)
type LineWatch = (i64, String, Option<i64>, Option<i64>);

/// One line for `person` per show (or film) while they have not answered it:
/// every watch named after joins it, over as many evenings as it takes. Once
/// answered or removed, the next watch starts a new line.
async fn tell(state: &AppState, owner: i64, person: i64, watch_ids: &[i64]) -> AppResult<()> {
    let pool = &state.pool;
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT id, title_id FROM watch_event
          WHERE user_id = ? AND id IN (SELECT value FROM json_each(?)) ORDER BY id",
    )
    .bind(owner)
    .bind(json!(watch_ids).to_string())
    .fetch_all(pool)
    .await?;
    let (who,): (String,) = sqlx::query_as(
        "SELECT COALESCE(NULLIF(TRIM(display_name), ''), username) FROM app_user WHERE id = ?",
    )
    .bind(owner)
    .fetch_one(pool)
    .await?;
    let mut groups: Vec<(i64, Vec<i64>)> = Vec::new();
    for (id, title_id) in rows {
        match groups.iter_mut().find(|g| g.0 == title_id) {
            Some(g) => g.1.push(id),
            None => groups.push((title_id, vec![id])),
        }
    }
    for (title_id, ids) in groups {
        let open = open_line(pool, person, owner, title_id).await?;
        let mut all: Vec<i64> = open
            .as_ref()
            .map(|(_, p)| watch_ids_of(p))
            .unwrap_or_default();
        for &id in &ids {
            if !all.contains(&id) {
                all.push(id);
            }
        }
        let watches: Vec<LineWatch> = sqlx::query_as(
            "SELECT w.id, substr(w.watched_at, 1, 10), w.episode_id, e.season_number
               FROM watch_event w LEFT JOIN episode e ON e.id = w.episode_id
              WHERE w.user_id = ? AND w.id IN (SELECT value FROM json_each(?))
              ORDER BY w.watched_at",
        )
        .bind(owner)
        .bind(json!(all).to_string())
        .fetch_all(pool)
        .await?;
        let mut days: Vec<String> = Vec::new();
        let mut episodes: Vec<i64> = Vec::new();
        let mut seasons: Vec<i64> = Vec::new();
        for (_, day, episode, season) in &watches {
            if !days.contains(day) {
                days.push(day.clone());
            }
            if let Some(e) = episode {
                if !episodes.contains(e) {
                    episodes.push(*e);
                }
            }
            if let Some(s) = season {
                if !seasons.contains(s) {
                    seasons.push(*s);
                }
            }
        }
        let whole_season = match seasons.as_slice() {
            [season] if *season > 0 => {
                let (size,): (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM episode WHERE title_id = ? AND season_number = ?",
                )
                .bind(title_id)
                .bind(season)
                .fetch_one(pool)
                .await?;
                (size as usize == episodes.len()).then_some(*season)
            }
            _ => None,
        };
        let (name, backdrop, poster): (String, Option<String>, Option<String>) =
            sqlx::query_as("SELECT name, backdrop_path, poster_path FROM title WHERE id = ?")
                .bind(title_id)
                .fetch_one(pool)
                .await?;
        let text = line_text(&who, &name, episodes.len(), whole_season);
        let payload = json!({ "text": text, "watch_ids": all, "owner": owner, "days": days });
        match open {
            Some((id, _)) => {
                sqlx::query(
                    "UPDATE notification SET payload = ?, read_at = NULL, created_at = ?
                      WHERE id = ? AND user_id = ?",
                )
                .bind(payload.to_string())
                .bind(Utc::now().to_rfc3339())
                .bind(id)
                .bind(person)
                .execute(pool)
                .await?;
            }
            None => {
                let first = ids.iter().min().copied().unwrap_or(0);
                inbox::tell(
                    state,
                    person,
                    &Event {
                        kind: inbox::TOGETHER,
                        title_id: Some(title_id),
                        actor_id: Some(owner),
                        payload,
                        ref_key: Some(format!("together:{owner}:{title_id}:{first}")),
                    },
                    Some(push(owner, title_id, &name, &text, backdrop, poster)),
                )
                .await?;
            }
        }
    }
    Ok(())
}

/// The push for a "watched with you" line. One tag per show and person, so a
/// new line replaces the last on the lock screen rather than stacking.
pub fn push(
    owner: i64,
    title_id: i64,
    name: &str,
    text: &str,
    backdrop: Option<String>,
    poster: Option<String>,
) -> Value {
    json!({
        "title": name,
        "body": text,
        "url": "/notifications",
        "tag": format!("frog-together-{owner}-{title_id}"),
        "image": inbox::picture(backdrop.as_deref(), poster.as_deref()),
    })
}

/// The line `person` has not answered yet about `owner`'s watches of a title.
async fn open_line(
    pool: &SqlitePool,
    person: i64,
    owner: i64,
    title_id: i64,
) -> AppResult<Option<(i64, String)>> {
    Ok(sqlx::query_as(
        "SELECT id, payload FROM notification
          WHERE user_id = ? AND kind = 'together' AND actor_id = ? AND title_id = ?
            AND removed_at IS NULL AND json_extract(payload, '$.answered') IS NULL
          ORDER BY id DESC LIMIT 1",
    )
    .bind(person)
    .bind(owner)
    .bind(title_id)
    .fetch_optional(pool)
    .await?)
}

fn watch_ids_of(payload: &str) -> Vec<i64> {
    serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|p| serde_json::from_value(p["watch_ids"].clone()).ok())
        .unwrap_or_default()
}

/// The watches a "watched with you" line is about, still naming `me`: the
/// owner's watches of that title on the line's days. Found by what the line
/// is about rather than by row, since an undo puts a watch back as a new
/// row; a watch another line of the same show is about stays with that one.
async fn named(pool: &SqlitePool, me: i64, notification: i64) -> AppResult<(i64, Vec<i64>)> {
    let row: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT payload, title_id FROM notification
          WHERE id = ? AND user_id = ? AND kind = 'together'",
    )
    .bind(notification)
    .bind(me)
    .fetch_optional(pool)
    .await?;
    let Some((raw, Some(title_id))) = row else {
        return Err(AppError::NotFound("no such notification".into()));
    };
    let payload: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let owner = payload["owner"].as_i64().unwrap_or(0);
    // A line from before lines grew across evenings names one day.
    let days: Vec<String> = serde_json::from_value(payload["days"].clone())
        .ok()
        .or_else(|| payload["day"].as_str().map(|d| vec![d.to_string()]))
        .unwrap_or_default();
    let still: Vec<(i64,)> = sqlx::query_as(
        "SELECT x.watch_event_id FROM watch_with x
           JOIN watch_event w ON w.id = x.watch_event_id
          WHERE x.with_user_id = ?1 AND x.user_id = ?2 AND w.user_id = ?2
            AND w.title_id = ?3
            AND substr(w.watched_at, 1, 10) IN (SELECT value FROM json_each(?4))
          ORDER BY w.id",
    )
    .bind(me)
    .bind(owner)
    .bind(title_id)
    .bind(json!(days).to_string())
    .fetch_all(pool)
    .await?;
    let others: Vec<(String,)> = sqlx::query_as(
        "SELECT payload FROM notification
          WHERE user_id = ? AND kind = 'together' AND actor_id = ? AND title_id = ? AND id != ?",
    )
    .bind(me)
    .bind(owner)
    .bind(title_id)
    .bind(notification)
    .fetch_all(pool)
    .await?;
    let theirs: Vec<i64> = others.iter().flat_map(|(p,)| watch_ids_of(p)).collect();
    let mine = watch_ids_of(&raw);
    Ok((
        owner,
        still
            .into_iter()
            .map(|r| r.0)
            .filter(|id| mine.contains(id) || !theirs.contains(id))
            .collect(),
    ))
}

/// (title, episode, watched_at, undated, logged_at) of a watch being copied.
type CopiedRow = (i64, Option<i64>, String, bool, Option<String>);

/// Copy the watches someone named you on into your own diary, as your own
/// rows on the same days, each naming them back. A watch you already have
/// (same title, episode and moment) is not copied twice. A title not in your
/// library comes in with `status`.
pub async fn add_to_mine(
    pool: &SqlitePool,
    me: i64,
    notification: i64,
    status: Option<&str>,
) -> AppResult<i64> {
    let (owner, watches) = named(pool, me, notification).await?;
    let rows: Vec<CopiedRow> = sqlx::query_as(
        "SELECT title_id, episode_id, watched_at, undated, logged_at FROM watch_event
          WHERE user_id = ? AND id IN (SELECT value FROM json_each(?))",
    )
    .bind(owner)
    .bind(json!(watches).to_string())
    .fetch_all(pool)
    .await?;
    let now = Utc::now().to_rfc3339();
    let mut copied = 0;
    let mut titles = Vec::new();
    for (title_id, episode_id, watched_at, undated, logged_at) in rows {
        if crate::store::get_entry(pool, me, title_id).await?.is_none() {
            let patch = crate::store::LibraryPatch {
                status: Some(status.unwrap_or("watching").to_string()),
                ..Default::default()
            };
            crate::store::apply_patch(pool, me, title_id, &patch).await?;
        }
        let made: Option<(i64,)> = sqlx::query_as(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                      source, undated, logged_at, logged_how)
             SELECT ?1, ?2, ?3, ?4,
                    EXISTS (SELECT 1 FROM watch_event x WHERE x.user_id = ?1 AND x.title_id = ?2
                             AND x.episode_id IS ?3),
                    'manual', ?5, ?6, 'together'
              WHERE NOT EXISTS (SELECT 1 FROM watch_event x
                                 WHERE x.user_id = ?1 AND x.title_id = ?2
                                   AND x.episode_id IS ?3 AND x.watched_at = ?4)
             RETURNING id",
        )
        .bind(me)
        .bind(title_id)
        .bind(episode_id)
        .bind(&watched_at)
        .bind(undated)
        .bind(&logged_at)
        .fetch_optional(pool)
        .await?;
        if let Some((id,)) = made {
            sqlx::query(
                "INSERT OR IGNORE INTO watch_with (watch_event_id, user_id, with_user_id, created_at)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(me)
            .bind(owner)
            .bind(&now)
            .execute(pool)
            .await?;
            copied += 1;
        }
        if !titles.contains(&title_id) {
            titles.push(title_id);
        }
    }
    for title_id in titles {
        crate::release::after_watch(pool, me, title_id).await?;
    }
    sqlx::query("UPDATE notification SET read_at = COALESCE(read_at, ?), payload = json_set(payload, '$.answered', 'added') WHERE id = ? AND user_id = ?")
        .bind(&now)
        .bind(notification)
        .bind(me)
        .execute(pool)
        .await?;
    Ok(copied)
}

/// Take your name off the watches a line is about, and the line with it.
pub async fn not_me(pool: &SqlitePool, me: i64, notification: i64) -> AppResult<()> {
    let (_, watches) = named(pool, me, notification).await?;
    sqlx::query(
        "DELETE FROM watch_with
          WHERE with_user_id = ? AND watch_event_id IN (SELECT value FROM json_each(?))",
    )
    .bind(me)
    .bind(json!(watches).to_string())
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM notification WHERE id = ? AND user_id = ?")
        .bind(notification)
        .bind(me)
        .execute(pool)
        .await?;
    Ok(())
}

/// The people named on a diary day's watches of one title, by id.
pub async fn on_day(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    day: &str,
    film: bool,
) -> AppResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT x.with_user_id FROM watch_with x
           JOIN watch_event w ON w.id = x.watch_event_id
          WHERE x.user_id = ?1 AND w.user_id = ?1 AND w.title_id = ?2
            AND substr(w.watched_at, 1, 10) = ?3 AND (w.episode_id IS NULL) = ?4",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(day)
    .bind(film)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Say who a diary day's watches of one title were with: those added are
/// told, those left out lose the name. Nobody clears it.
pub async fn set_on_day(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    day: &str,
    film: bool,
    with: &[i64],
) -> AppResult<()> {
    let pool = &state.pool;
    let with = allowed(pool, user_id, with).await?;
    let watches: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM watch_event
          WHERE user_id = ? AND title_id = ? AND substr(watched_at, 1, 10) = ?
            AND (episode_id IS NULL) = ? AND undated = 0",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(day)
    .bind(film)
    .fetch_all(pool)
    .await?;
    let ids: Vec<i64> = watches.into_iter().map(|r| r.0).collect();
    let before = on_day(pool, user_id, title_id, day, film).await?;
    for gone in before.iter().filter(|p| !with.contains(p)) {
        sqlx::query(
            "DELETE FROM watch_with
              WHERE user_id = ? AND with_user_id = ?
                AND watch_event_id IN (SELECT value FROM json_each(?))",
        )
        .bind(user_id)
        .bind(gone)
        .bind(json!(ids).to_string())
        .execute(pool)
        .await?;
    }
    let new: Vec<i64> = with
        .iter()
        .copied()
        .filter(|p| !before.contains(p))
        .collect();
    tag(state, user_id, &ids, &new).await?;
    Ok(())
}

/// Names for ids, as a people list shows them: only people still here.
pub async fn names(pool: &SqlitePool, ids: &[i64]) -> AppResult<Vec<Value>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(i64, String, Option<String>, bool)> = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name,
                EXISTS (SELECT 1 FROM user_image i WHERE i.user_id = u.id AND i.kind = 'avatar')
           FROM app_user u
          WHERE u.disabled = 0 AND u.id IN (SELECT value FROM json_each(?))",
    )
    .bind(json!(ids).to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, username, display_name, avatar)| {
            json!({ "id": id, "username": username, "display_name": display_name, "avatar": avatar })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_says_the_title_a_season_or_how_many() {
        assert_eq!(
            line_text("Sam", "Arrival", 0, None),
            "Sam watched Arrival with you"
        );
        assert_eq!(
            line_text("Sam", "Breaking Bad", 1, Some(1)),
            "Sam watched Breaking Bad with you"
        );
        assert_eq!(
            line_text("Sam", "Breaking Bad", 7, Some(2)),
            "Sam watched season 2 of Breaking Bad with you"
        );
        assert_eq!(
            line_text("Sam", "Breaking Bad", 3, None),
            "Sam watched 3 episodes of Breaking Bad with you"
        );
    }

    #[test]
    fn its_push_shows_the_picture_and_keeps_one_tag_per_show() {
        let p = push(4, 9, "Breaking Bad", "x", None, Some("/p.jpg".into()));
        assert_eq!(p["image"], json!("/img/w780/p.jpg"));
        assert_eq!(p["tag"], json!("frog-together-4-9"));
        let wide = push(4, 9, "Breaking Bad", "y", Some("/b.jpg".into()), None);
        assert_eq!(wide["image"], json!("/img/w780/b.jpg"));
        assert_eq!(wide["tag"], p["tag"]);
    }
}
