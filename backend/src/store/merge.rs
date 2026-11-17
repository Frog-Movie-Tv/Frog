//! Moving an account's data from one title to another, merges and their
//! undo, and folding a copy into its title for every account at once.

use super::*;

/// (status, rating, favourite, notes, started, finished, rewatches, added, notify, progress)
type MergeEntryRow = (
    String,
    Option<f64>,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    String,
    bool,
    Option<i64>,
);

/// Move one account's data from one title to another: the library entry,
/// shelves and film watches. Used when an AniList-only title turns out to be
/// a TMDB one, or a person says where it belongs.
///
/// Nothing is dropped on a clash: the further-along status wins, the rating
/// and dates already on the target stay, notes are kept side by side, and a
/// film watch AniList made up is not copied onto a film that has a watch.
/// What both sides held before is written to `title_merge`. All of it is one
/// transaction, so a failure leaves both titles as they were, and running it
/// again finds nothing left to move. Says whether anything moved.
pub async fn move_title_data(
    pool: &SqlitePool,
    user_id: i64,
    from: i64,
    to: i64,
    reason: &str,
) -> AppResult<bool> {
    if from == to {
        return Ok(false);
    }
    let mut tx = pool.begin().await?;
    let kinds: Vec<(i64, String)> = sqlx::query_as("SELECT id, kind FROM title WHERE id IN (?, ?)")
        .bind(from)
        .bind(to)
        .fetch_all(&mut *tx)
        .await?;
    let kind_of = |id: i64| kinds.iter().find(|k| k.0 == id).map(|k| k.1.clone());
    let (Some(from_kind), Some(to_kind)) = (kind_of(from), kind_of(to)) else {
        return Err(AppError::NotFound("no such title".into()));
    };

    let entry_sql = "SELECT status, rating, favourite, notes, started_at, finished_at,
                            rewatch_count, added_at, notify, progress
                       FROM library_entry WHERE user_id = ? AND title_id = ?";
    let old: Option<MergeEntryRow> = sqlx::query_as(entry_sql)
        .bind(user_id)
        .bind(from)
        .fetch_optional(&mut *tx)
        .await?;
    let have: Option<MergeEntryRow> = sqlx::query_as(entry_sql)
        .bind(user_id)
        .bind(to)
        .fetch_optional(&mut *tx)
        .await?;
    let (films,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watch_event WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
    )
    .bind(user_id)
    .bind(from)
    .fetch_one(&mut *tx)
    .await?;
    let (shelved,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
          WHERE s.user_id = ? AND i.title_id = ?",
    )
    .bind(user_id)
    .bind(from)
    .fetch_one(&mut *tx)
    .await?;
    if old.is_none() && films == 0 && shelved == 0 {
        return Ok(false);
    }

    // Where each side sat on this account's shelves, so an undo can put it back.
    let shelves_of = |title: i64| {
        sqlx::query_as::<_, (i64, i64)>(
            "SELECT i.shelf_id, i.position FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
              WHERE s.user_id = ? AND i.title_id = ?",
        )
        .bind(user_id)
        .bind(title)
    };
    let from_shelves = shelves_of(from).fetch_all(&mut *tx).await?;
    let to_shelves: Vec<i64> = shelves_of(to)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|(shelf, _)| shelf)
        .collect();

    let as_json = |e: &Option<MergeEntryRow>| {
        e.as_ref().map(|e| {
            serde_json::json!({
                "status": e.0, "rating": e.1, "favourite": e.2, "notes": e.3,
                "started_at": e.4, "finished_at": e.5, "rewatch_count": e.6,
                "added_at": e.7, "notify": e.8, "progress": e.9,
            })
        })
    };
    let mut before = serde_json::json!({
        "from": as_json(&old),
        "to": as_json(&have),
        "film_watches": films,
        "shelved": shelved,
        "from_shelves": from_shelves,
        "to_shelves": to_shelves,
        "watch_ids": [],
    });

    if let Some(o) = &old {
        let now = Utc::now().stamp();
        let merged = match &have {
            None => o.clone(),
            Some(h) => {
                let notes = match (&h.3, &o.3) {
                    (Some(a), Some(b)) if a.trim() != b.trim() => Some(format!("{a}\n\n{b}")),
                    (a, b) => a.clone().or_else(|| b.clone()),
                };
                (
                    crate::transfer::merged_status(&h.0, &o.0),
                    h.1.or(o.1),
                    h.2 || o.2,
                    notes,
                    min_opt(&h.4, &o.4),
                    max_opt(&h.5, &o.5),
                    h.6.max(o.6),
                    h.7.clone().min(o.7.clone()),
                    h.8,
                    h.9,
                )
            }
        };
        // A film's rating is the film's. A show's own rating is not what one
        // AniList entry was scored (that lands on its season when the entry
        // is synced there), so it stays in the record rather than moving.
        let rating = if to_kind == "tv" {
            have.as_ref().and_then(|h| h.1)
        } else {
            merged.1
        };
        sqlx::query(
            "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes,
                                        started_at, finished_at, rewatch_count, added_at,
                                        updated_at, notify)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (user_id, title_id) DO UPDATE SET
                status = excluded.status, rating = excluded.rating,
                favourite = excluded.favourite, notes = excluded.notes,
                started_at = excluded.started_at, finished_at = excluded.finished_at,
                rewatch_count = excluded.rewatch_count, added_at = excluded.added_at,
                updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(to)
        .bind(&merged.0)
        .bind(rating)
        .bind(merged.2)
        .bind(&merged.3)
        .bind(&merged.4)
        .bind(&merged.5)
        .bind(merged.6)
        .bind(&merged.7)
        .bind(&now)
        .bind(merged.8)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM library_entry WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(from)
            .execute(&mut *tx)
            .await?;
    }

    if from_kind == "movie" && to_kind == "movie" {
        // Made-up watches only go across when the film has none of its own.
        sqlx::query(
            "DELETE FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                AND source = 'anilist' AND undated = 1
                AND EXISTS (SELECT 1 FROM watch_event w
                             WHERE w.user_id = ? AND w.title_id = ? AND w.episode_id IS NULL)",
        )
        .bind(user_id)
        .bind(from)
        .bind(user_id)
        .bind(to)
        .execute(&mut *tx)
        .await?;
        let moved: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM watch_event WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
        )
        .bind(user_id)
        .bind(from)
        .fetch_all(&mut *tx)
        .await?;
        before["watch_ids"] = serde_json::json!(moved.iter().map(|m| m.0).collect::<Vec<_>>());
        sqlx::query(
            "UPDATE watch_event SET title_id = ?
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
        )
        .bind(to)
        .bind(user_id)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    } else {
        // A film-level watch cannot become a show's: AniList's own made-up one
        // goes (the show's episodes carry it now); any logged by hand stay on
        // the old title, where the diary still has them.
        sqlx::query(
            "DELETE FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                AND source = 'anilist' AND undated = 1",
        )
        .bind(user_id)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
         SELECT i.shelf_id, ?, i.position FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
          WHERE s.user_id = ? AND i.title_id = ?",
    )
    .bind(to)
    .bind(user_id)
    .bind(from)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM shelf_item
          WHERE title_id = ? AND shelf_id IN (SELECT id FROM shelf WHERE user_id = ?)",
    )
    .bind(from)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO title_merge (user_id, from_title, to_title, reason, before_json, merged_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(reason)
    .bind(before.to_string())
    .bind(Utc::now().stamp())
    .execute(&mut *tx)
    .await?;

    // Once nobody keeps an AniList-only title, it points at where it went,
    // so an old link, and its AniList id, still find it (`catalog::find`).
    sqlx::query(
        "UPDATE title SET merged_into = ?
          WHERE id = ?
            AND EXISTS (SELECT 1 FROM title_external x
                         WHERE x.title_id = title.id AND x.provider = 'anilist'
                           AND x.is_primary = 1)
            AND NOT EXISTS (SELECT 1 FROM library_entry l WHERE l.title_id = title.id)",
    )
    .bind(to)
    .bind(from)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// One merge as Settings lists it.
#[derive(Debug, Serialize)]
pub struct MergeView {
    pub id: i64,
    pub from_id: i64,
    pub from_name: String,
    pub from_poster: Option<String>,
    pub to_id: i64,
    pub to_name: String,
    pub to_poster: Option<String>,
    pub reason: String,
    pub merged_at: String,
    pub undone_at: Option<String>,
}

/// (id, from, from name, from poster, to, to name, to poster, reason, merged_at, undone_at)
type MergeRow = (
    i64,
    i64,
    String,
    Option<String>,
    i64,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
);

/// This account's merges, newest first.
pub async fn merges(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<MergeView>> {
    let rows: Vec<MergeRow> = sqlx::query_as(
        "SELECT m.id, m.from_title, f.name, f.poster_path, m.to_title, t.name, t.poster_path,
                m.reason, m.merged_at, m.undone_at
           FROM title_merge m
           JOIN title f ON f.id = m.from_title
           JOIN title t ON t.id = m.to_title
          WHERE m.user_id = ?
          ORDER BY m.merged_at DESC, m.id DESC
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MergeView {
            id: r.0,
            from_id: r.1,
            from_name: r.2,
            from_poster: r.3,
            to_id: r.4,
            to_name: r.5,
            to_poster: r.6,
            reason: r.7,
            merged_at: r.8,
            undone_at: r.9,
        })
        .collect())
}

/// A merge that can be undone: (from, to).
pub async fn undoable_merge(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<(i64, i64)> {
    let row: Option<(i64, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT from_title, to_title, merged_at, undone_at FROM title_merge
          WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some((from, to, at, undone)) = row else {
        return Err(AppError::NotFound("No such merge.".into()));
    };
    if undone.is_some() {
        return Err(AppError::Conflict("That merge is already undone.".into()));
    }
    // A later merge of either title went on from what this one left.
    let (later,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM title_merge
          WHERE user_id = ?1 AND id != ?2 AND undone_at IS NULL
            AND (merged_at > ?3 OR (merged_at = ?3 AND id > ?2))
            AND (from_title IN (?4, ?5) OR to_title IN (?4, ?5))",
    )
    .bind(user_id)
    .bind(id)
    .bind(&at)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    if later > 0 {
        return Err(AppError::Conflict(
            "Undo the later merge of this title first.".into(),
        ));
    }
    Ok((from, to))
}

/// Put one account's data back as it was before a merge: both library
/// entries, the film watches that moved, and the shelves. The copy is its own
/// title again. Watches logged on a show's episodes stay where they went.
pub async fn undo_merge(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<(i64, i64)> {
    let (from, to) = undoable_merge(pool, user_id, id).await?;
    let (raw,): (String,) =
        sqlx::query_as("SELECT before_json FROM title_merge WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    let before: serde_json::Value = serde_json::from_str(&raw)?;
    let now = Utc::now().stamp();

    let mut tx = pool.begin().await?;
    for (title, side) in [(from, &before["from"]), (to, &before["to"])] {
        if side.is_null() {
            sqlx::query("DELETE FROM library_entry WHERE user_id = ? AND title_id = ?")
                .bind(user_id)
                .bind(title)
                .execute(&mut *tx)
                .await?;
            continue;
        }
        sqlx::query(
            "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes,
                                        started_at, finished_at, rewatch_count, added_at,
                                        updated_at, notify, progress)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (user_id, title_id) DO UPDATE SET
                status = excluded.status, rating = excluded.rating,
                favourite = excluded.favourite, notes = excluded.notes,
                started_at = excluded.started_at, finished_at = excluded.finished_at,
                rewatch_count = excluded.rewatch_count, added_at = excluded.added_at,
                updated_at = excluded.updated_at, notify = excluded.notify,
                progress = excluded.progress",
        )
        .bind(user_id)
        .bind(title)
        .bind(side["status"].as_str().unwrap_or("planned"))
        .bind(side["rating"].as_f64())
        .bind(side["favourite"].as_bool().unwrap_or(false))
        .bind(side["notes"].as_str())
        .bind(side["started_at"].as_str())
        .bind(side["finished_at"].as_str())
        .bind(side["rewatch_count"].as_i64().unwrap_or(0))
        .bind(side["added_at"].as_str().unwrap_or(&now))
        .bind(&now)
        .bind(side["notify"].as_bool().unwrap_or(true))
        .bind(side["progress"].as_i64())
        .execute(&mut *tx)
        .await?;
    }

    for watch in before["watch_ids"].as_array().into_iter().flatten() {
        sqlx::query(
            "UPDATE watch_event SET title_id = ? WHERE id = ? AND user_id = ? AND title_id = ?",
        )
        .bind(from)
        .bind(watch.as_i64())
        .bind(user_id)
        .bind(to)
        .execute(&mut *tx)
        .await?;
    }

    let had: Vec<i64> = before["to_shelves"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_i64)
        .collect();
    for pair in before["from_shelves"].as_array().into_iter().flatten() {
        let (Some(shelf), Some(position)) = (pair[0].as_i64(), pair[1].as_i64()) else {
            continue;
        };
        sqlx::query(
            "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
             SELECT id, ?, ? FROM shelf WHERE id = ? AND user_id = ?",
        )
        .bind(from)
        .bind(position)
        .bind(shelf)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        if !had.contains(&shelf) {
            sqlx::query(
                "DELETE FROM shelf_item
                  WHERE title_id = ? AND shelf_id IN (SELECT id FROM shelf WHERE id = ? AND user_id = ?)",
            )
            .bind(to)
            .bind(shelf)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    sqlx::query("UPDATE title SET merged_into = NULL WHERE id = ? AND merged_into = ?")
        .bind(from)
        .bind(to)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE title_merge SET undone_at = ? WHERE id = ? AND user_id = ?")
        .bind(&now)
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((from, to))
}

/// Watches on a series copy's first season, moved onto the same episodes of
/// the show it is the first season of. Episodes the show does not list stay
/// where they are, in the diary.
pub async fn move_first_season_watches(pool: &SqlitePool, from: i64, to: i64) -> AppResult<u64> {
    let moved = sqlx::query(
        "UPDATE watch_event
            SET title_id = ?2,
                episode_id = (SELECT t.id FROM episode t
                               WHERE t.title_id = ?2 AND t.season_number = 1
                                 AND t.episode_number = (SELECT e.episode_number FROM episode e
                                                          WHERE e.id = watch_event.episode_id))
          WHERE title_id = ?1
            AND episode_id IN (SELECT e.id FROM episode e
                                WHERE e.title_id = ?1 AND e.season_number = 1
                                  AND EXISTS (SELECT 1 FROM episode t
                                               WHERE t.title_id = ?2 AND t.season_number = 1
                                                 AND t.episode_number = e.episode_number))",
    )
    .bind(from)
    .bind(to)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(moved)
}

/// Fold a copy of a title into the one it duplicates: every account that
/// holds anything on the copy has it moved (`move_title_data`), and the copy
/// then points at the survivor, so its links and provider ids still lead
/// there. Nothing is dropped.
pub async fn merge_copy(pool: &SqlitePool, from: i64, to: i64, reason: &str) -> AppResult<()> {
    let holders: Vec<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM library_entry WHERE title_id = ?1
         UNION SELECT user_id FROM watch_event WHERE title_id = ?1
         UNION SELECT s.user_id FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
                WHERE i.title_id = ?1",
    )
    .bind(from)
    .fetch_all(pool)
    .await?;
    for (user_id,) in holders {
        move_title_data(pool, user_id, from, to, reason).await?;
    }
    sqlx::query("UPDATE title SET merged_into = ? WHERE id = ? AND merged_into IS NULL")
        .bind(to)
        .bind(from)
        .execute(pool)
        .await?;
    Ok(())
}

fn min_opt(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.clone().min(b.clone())),
        (a, b) => a.clone().or_else(|| b.clone()),
    }
}

fn max_opt(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.clone().max(b.clone())),
        (a, b) => a.clone().or_else(|| b.clone()),
    }
}
