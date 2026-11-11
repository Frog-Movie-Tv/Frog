//! Shelves: hand-picked lists in your own order, watchlists (hand-picked from
//! everything, so a title need not be in the library), and smart shelves that
//! fill themselves from rules ("shows in progress rated 8 or more").
//!
//! A watchlist is stored as a manual shelf with `watchlist` set; the app sees
//! it as a kind of its own. Every statement on `shelf` and `shelf_item` is
//! here, and every one names the account; `routes::shelves` is HTTP only.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::catalog::{TitleRow, TITLE_COLUMNS};
use crate::error::{AppError, AppResult};
use crate::store::{self, STATUSES};

/// What a smart shelf matches. Every field is optional; an empty rule set
/// matches the whole library.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rules {
    /// movie | tv
    pub kind: Option<String>,
    /// Any of these statuses.
    pub statuses: Vec<String>,
    /// Any of these genres.
    pub genres: Vec<String>,
    /// 0-100, inclusive.
    pub min_rating: Option<f64>,
    pub max_rating: Option<f64>,
    /// unstarted | started | unfinished | finished
    pub progress: Option<String>,
    /// Only favourites.
    pub favourite: bool,
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
    /// The library's sort keys: added | updated | name | rating | release | last_watched
    pub sort: Option<String>,
    /// asc | desc
    pub order: Option<String>,
    pub limit: Option<i64>,
}

const PROGRESS: [&str; 4] = ["unstarted", "started", "unfinished", "finished"];
const MAX_ITEMS: i64 = 500;

impl Rules {
    pub fn validate(&self) -> AppResult<()> {
        let bad = |m: &str| Err(AppError::BadRequest(m.into()));
        if let Some(k) = &self.kind {
            if k != "movie" && k != "tv" {
                return bad("That isn't a film or a show.");
            }
        }
        if self
            .statuses
            .iter()
            .any(|s| !STATUSES.contains(&s.as_str()))
        {
            return bad("unknown status in rules");
        }
        for r in [self.min_rating, self.max_rating].into_iter().flatten() {
            if !(0.0..=100.0).contains(&r) {
                return bad("ratings in rules must be between 0 and 100");
            }
        }
        if let Some(p) = &self.progress {
            if !PROGRESS.contains(&p.as_str()) {
                return bad("progress must be unstarted, started, unfinished or finished");
            }
        }
        if self.genres.len() > 30 || self.genres.iter().any(|g| g.len() > 60) {
            return bad("too many genres in rules");
        }
        Ok(())
    }
}

// Progress, as the library's progress bars count it: specials do not count.
const TOTAL_EPS: &str =
    "(SELECT COUNT(*) FROM episode e WHERE e.title_id = t.id AND e.season_number > 0)";

/// The account is written in rather than bound: these fragments are pasted
/// into a QueryBuilder several times each, and a placeholder would have to be
/// bound once per appearance in the right order. It is an `i64` from the
/// session, so there is nothing in it to escape.
fn watched_eps(user_id: i64) -> String {
    format!(
        "(SELECT COUNT(DISTINCT e.id) FROM episode e
      JOIN watch_event w ON w.episode_id = e.id AND w.user_id = {user_id}
     WHERE e.title_id = t.id AND e.season_number > 0)"
    )
}

fn film_seen(user_id: i64) -> String {
    format!(
        "EXISTS (SELECT 1 FROM watch_event w
                  WHERE w.user_id = {user_id} AND w.title_id = t.id)"
    )
}

/// Titles a smart shelf holds right now.
pub async fn evaluate(pool: &SqlitePool, user_id: i64, rules: &Rules) -> AppResult<Vec<TitleRow>> {
    let watched_eps = watched_eps(user_id);
    let film_seen = film_seen(user_id);
    let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(format!(
        "SELECT {TITLE_COLUMNS}
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = "
    ));
    q.push_bind(user_id);
    if let Some(kind) = &rules.kind {
        q.push(" AND t.kind = ").push_bind(kind.clone());
    }
    if !rules.statuses.is_empty() {
        q.push(" AND l.status IN (");
        let mut list = q.separated(", ");
        for s in &rules.statuses {
            list.push_bind(s.clone());
        }
        q.push(")");
    }
    if !rules.genres.is_empty() {
        q.push(" AND (");
        for (i, g) in rules.genres.iter().enumerate() {
            if i > 0 {
                q.push(" OR ");
            }
            q.push("t.genres_json LIKE ")
                .push_bind(format!("%\"{g}\"%"));
        }
        q.push(")");
    }
    if let Some(min) = rules.min_rating {
        q.push(" AND l.rating >= ").push_bind(min);
    }
    if let Some(max) = rules.max_rating {
        q.push(" AND l.rating <= ").push_bind(max);
    }
    if rules.favourite {
        q.push(" AND l.favourite = 1");
    }
    if let Some(from) = rules.from_year {
        q.push(" AND CAST(substr(t.release_date, 1, 4) AS INTEGER) >= ")
            .push_bind(from);
    }
    if let Some(to) = rules.to_year {
        q.push(" AND CAST(substr(t.release_date, 1, 4) AS INTEGER) <= ")
            .push_bind(to);
    }
    // A manga goes by its chapters read: a count, against a total that is
    // often not known while it is still coming out.
    let read = "COALESCE(l.progress, 0)";
    let chapters = "COALESCE(t.episode_count, 0)";
    match rules.progress.as_deref() {
        Some("unstarted") => q.push(format!(
            " AND ((t.kind = 'movie' AND NOT {film_seen}) OR (t.kind = 'tv' AND {watched_eps} = 0)
                OR (t.kind = 'manga' AND {read} = 0))"
        )),
        Some("started") => q.push(format!(
            " AND ((t.kind = 'tv' AND {watched_eps} > 0 AND {watched_eps} < {TOTAL_EPS})
                OR (t.kind = 'manga' AND {read} > 0 AND ({chapters} = 0 OR {read} < {chapters})))"
        )),
        Some("unfinished") => q.push(format!(
            " AND ((t.kind = 'movie' AND NOT {film_seen})
                OR (t.kind = 'tv' AND ({TOTAL_EPS} = 0 OR {watched_eps} < {TOTAL_EPS}))
                OR (t.kind = 'manga' AND ({chapters} = 0 OR {read} < {chapters})))"
        )),
        Some("finished") => q.push(format!(
            " AND ((t.kind = 'movie' AND {film_seen})
                OR (t.kind = 'tv' AND {TOTAL_EPS} > 0 AND {watched_eps} >= {TOTAL_EPS})
                OR (t.kind = 'manga' AND {chapters} > 0 AND {read} >= {chapters}))"
        )),
        _ => &mut q,
    };
    q.push(" ");
    q.push(store::order_clause(
        rules.sort.as_deref(),
        rules.order.as_deref(),
        user_id,
    ));
    q.push(" LIMIT ")
        .push_bind(rules.limit.unwrap_or(MAX_ITEMS).clamp(1, MAX_ITEMS));

    Ok(q.build_query_as::<TitleRow>().fetch_all(pool).await?)
}

/// (id, name, kind, rules, position, watchlist)
pub type ShelfRow = (i64, String, String, String, i64, bool);

/// The kind the app sees.
pub fn kind_of(row: &ShelfRow) -> &str {
    if row.5 {
        "watchlist"
    } else {
        &row.2
    }
}

/// The one gate for every shelf operation: a shelf that belongs to somebody
/// else is simply not there.
pub async fn find(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<ShelfRow> {
    sqlx::query_as(
        "SELECT id, name, kind, rules_json, position, watchlist
           FROM shelf WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("no such shelf".into()))
}

pub fn rules_of(raw: &str) -> Rules {
    serde_json::from_str(raw).unwrap_or_default()
}

async fn manual_titles(pool: &SqlitePool, shelf_id: i64) -> AppResult<Vec<TitleRow>> {
    Ok(sqlx::query_as::<_, TitleRow>(&format!(
        "SELECT {TITLE_COLUMNS}
           FROM shelf_item s
           JOIN title t ON t.id = s.title_id
          WHERE s.shelf_id = ?
          ORDER BY s.position, s.rowid"
    ))
    .bind(shelf_id)
    .fetch_all(pool)
    .await?)
}

pub async fn titles_of(
    pool: &SqlitePool,
    user_id: i64,
    row: &ShelfRow,
) -> AppResult<Vec<TitleRow>> {
    if row.2 == "smart" {
        evaluate(pool, user_id, &rules_of(&row.3)).await
    } else {
        manual_titles(pool, row.0).await
    }
}

pub fn summary(row: &ShelfRow, titles: &[TitleRow]) -> Value {
    json!({
        "id": row.0,
        "name": row.1,
        "kind": kind_of(row),
        "rules": rules_of(&row.3),
        "position": row.4,
        "count": titles.len(),
        "posters": titles.iter().filter_map(|t| t.poster_path.clone()).take(4).collect::<Vec<_>>(),
    })
}

/// One account's shelves, in their order, with a count and a cover each.
pub async fn summaries(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Value>> {
    let rows: Vec<ShelfRow> = sqlx::query_as(
        "SELECT id, name, kind, rules_json, position, watchlist
           FROM shelf WHERE user_id = ? ORDER BY position, id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut shelves = Vec::with_capacity(rows.len());
    for row in &rows {
        let titles = titles_of(pool, user_id, row).await?;
        shelves.push(summary(row, &titles));
    }
    Ok(shelves)
}

/// A new shelf, last in the account's order; a hand-picked one may start with
/// titles, in order (how a deleted shelf comes back). Returns its id.
pub async fn create(
    pool: &SqlitePool,
    user_id: i64,
    name: &str,
    kind: &str,
    rules: &Rules,
    watchlist: bool,
    title_ids: &[i64],
) -> AppResult<i64> {
    let mut tx = pool.begin().await?;
    let (next,): (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(position), -1) + 1 FROM shelf WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    let id = sqlx::query(
        "INSERT INTO shelf (user_id, name, kind, rules_json, position, created_at, watchlist)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(name)
    .bind(kind)
    .bind(serde_json::to_string(rules).unwrap_or_else(|_| "{}".into()))
    .bind(next)
    .bind(Utc::now().to_rfc3339())
    .bind(watchlist)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    if kind == "manual" {
        for (pos, title_id) in title_ids.iter().enumerate() {
            sqlx::query(
                "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
                 SELECT ?, id, ? FROM title WHERE id = ?",
            )
            .bind(id)
            .bind(pos as i64)
            .bind(title_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(id)
}

pub async fn rename(pool: &SqlitePool, user_id: i64, id: i64, name: &str) -> AppResult<()> {
    sqlx::query("UPDATE shelf SET name = ? WHERE id = ? AND user_id = ?")
        .bind(name)
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_rules(pool: &SqlitePool, user_id: i64, id: i64, rules: &Rules) -> AppResult<()> {
    sqlx::query("UPDATE shelf SET rules_json = ? WHERE id = ? AND user_id = ?")
        .bind(serde_json::to_string(rules).unwrap_or_else(|_| "{}".into()))
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a shelf, handing back its titles in order, which is what undo
/// needs to put it back exactly. The shelf was already found to be the
/// account's (`find`).
pub async fn delete(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<Vec<i64>> {
    let title_ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT i.title_id FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
          WHERE i.shelf_id = ? AND s.user_id = ? ORDER BY i.position",
    )
    .bind(id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    sqlx::query("DELETE FROM shelf WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(title_ids.into_iter().map(|(t,)| t).collect())
}

/// The account's shelves in this order.
pub async fn reorder(pool: &SqlitePool, user_id: i64, ids: &[i64]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for (pos, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE shelf SET position = ? WHERE id = ? AND user_id = ?")
            .bind(pos as i64)
            .bind(id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The shelf, when it is the account's and filled by hand.
pub async fn hand_picked(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<()> {
    let row = find(pool, user_id, id).await?;
    if row.2 != "manual" {
        return Err(AppError::BadRequest(
            "a smart shelf fills itself; change its rules instead".into(),
        ));
    }
    Ok(())
}

/// Put a title last on a hand-picked shelf (`hand_picked` first).
pub async fn add_item(pool: &SqlitePool, id: i64, title_id: i64) -> AppResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
         VALUES (?, ?, (SELECT COALESCE(MAX(position), -1) + 1 FROM shelf_item WHERE shelf_id = ?))",
    )
    .bind(id)
    .bind(title_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Take a title off a hand-picked shelf (`hand_picked` first).
pub async fn remove_item(pool: &SqlitePool, id: i64, title_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM shelf_item WHERE shelf_id = ? AND title_id = ?")
        .bind(id)
        .bind(title_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A hand-picked shelf's titles in this order (`hand_picked` first).
pub async fn reorder_items(pool: &SqlitePool, id: i64, title_ids: &[i64]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for (pos, title_id) in title_ids.iter().enumerate() {
        sqlx::query("UPDATE shelf_item SET position = ? WHERE shelf_id = ? AND title_id = ?")
            .bind(pos as i64)
            .bind(id)
            .bind(title_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The hand-picked shelves and watchlists, and whether this title is on each:
/// (id, name, on).
pub async fn holding(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Vec<(i64, String, bool)>> {
    Ok(sqlx::query_as(
        "SELECT s.id, s.name,
                EXISTS (SELECT 1 FROM shelf_item i
                          WHERE i.shelf_id = s.id AND i.title_id = ?)
           FROM shelf s
          WHERE s.user_id = ? AND s.kind = 'manual'
          ORDER BY s.position, s.id",
    )
    .bind(title_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}
