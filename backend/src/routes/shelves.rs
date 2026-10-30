//! Shelves: hand-picked lists in your own order, and smart shelves that fill
//! themselves from rules ("shows in progress rated 8 or more").

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::auth::Auth;
use crate::catalog::{self, TitleRow, TITLE_COLUMNS};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
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
    match rules.progress.as_deref() {
        Some("unstarted") => q.push(format!(
            " AND ((t.kind = 'movie' AND NOT {film_seen}) OR (t.kind = 'tv' AND {watched_eps} = 0))"
        )),
        Some("started") => q.push(format!(
            " AND t.kind = 'tv' AND {watched_eps} > 0 AND {watched_eps} < {TOTAL_EPS}"
        )),
        Some("unfinished") => q.push(format!(
            " AND ((t.kind = 'movie' AND NOT {film_seen})
                OR (t.kind = 'tv' AND ({TOTAL_EPS} = 0 OR {watched_eps} < {TOTAL_EPS})))"
        )),
        Some("finished") => q.push(format!(
            " AND ((t.kind = 'movie' AND {film_seen})
                OR (t.kind = 'tv' AND {TOTAL_EPS} > 0 AND {watched_eps} >= {TOTAL_EPS}))"
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

/// (id, name, kind, rules, position)
type ShelfRow = (i64, String, String, String, i64);

/// The one gate for every shelf operation: a shelf that belongs to somebody
/// else is simply not there.
async fn shelf(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<ShelfRow> {
    sqlx::query_as(
        "SELECT id, name, kind, rules_json, position
           FROM shelf WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("no such shelf".into()))
}

fn rules_of(raw: &str) -> Rules {
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

async fn titles_of(pool: &SqlitePool, user_id: i64, row: &ShelfRow) -> AppResult<Vec<TitleRow>> {
    if row.2 == "smart" {
        evaluate(pool, user_id, &rules_of(&row.3)).await
    } else {
        manual_titles(pool, row.0).await
    }
}

fn summary(row: &ShelfRow, titles: &[TitleRow]) -> Value {
    json!({
        "id": row.0,
        "name": row.1,
        "kind": row.2,
        "rules": rules_of(&row.3),
        "position": row.4,
        "count": titles.len(),
        "posters": titles.iter().filter_map(|t| t.poster_path.clone()).take(4).collect::<Vec<_>>(),
    })
}

pub async fn list(auth: Auth, State(state): State<AppState>) -> AppResult<Json<Value>> {
    let shelves = summaries(&state.pool, auth.user_id()).await?;
    Ok(Json(json!({ "shelves": shelves })))
}

/// One account's shelves, in their order, with a count and a cover each.
pub async fn summaries(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Value>> {
    let rows: Vec<ShelfRow> = sqlx::query_as(
        "SELECT id, name, kind, rules_json, position
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

pub async fn get(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let row = shelf(&state.pool, auth.user_id(), id).await?;
    let titles = titles_of(&state.pool, auth.user_id(), &row).await?;
    let shelf = summary(&row, &titles);
    let items = store::build_items(&state.pool, auth.user_id(), titles).await?;
    Ok(Json(json!({ "shelf": shelf, "items": items })))
}

fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::BadRequest(
            "a shelf name needs 1 to 60 characters".into(),
        ));
    }
    Ok(name.to_string())
}

#[derive(Deserialize)]
pub struct CreateBody {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub rules: Rules,
    /// Local title ids to start a manual shelf with, in order. Used to bring
    /// a deleted shelf back.
    #[serde(default)]
    pub title_ids: Vec<i64>,
}

pub async fn create(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> AppResult<Json<Value>> {
    let name = clean_name(&body.name)?;
    if body.kind != "manual" && body.kind != "smart" {
        return Err(AppError::BadRequest("kind must be manual or smart".into()));
    }
    body.rules.validate()?;

    let mut tx = state.pool.begin().await?;
    let (next,): (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(position), -1) + 1 FROM shelf WHERE user_id = ?")
            .bind(auth.user_id())
            .fetch_one(&mut *tx)
            .await?;
    let id = sqlx::query(
        "INSERT INTO shelf (user_id, name, kind, rules_json, position, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(auth.user_id())
    .bind(&name)
    .bind(&body.kind)
    .bind(serde_json::to_string(&body.rules).unwrap_or_else(|_| "{}".into()))
    .bind(next)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    if body.kind == "manual" {
        for (pos, title_id) in body.title_ids.iter().enumerate() {
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

    let row = shelf(&state.pool, auth.user_id(), id).await?;
    let titles = titles_of(&state.pool, auth.user_id(), &row).await?;
    Ok(Json(json!({ "shelf": summary(&row, &titles) })))
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub name: Option<String>,
    pub rules: Option<Rules>,
}

pub async fn update(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> AppResult<Json<Value>> {
    let row = shelf(&state.pool, auth.user_id(), id).await?;
    if let Some(name) = &body.name {
        sqlx::query("UPDATE shelf SET name = ? WHERE id = ? AND user_id = ?")
            .bind(clean_name(name)?)
            .bind(id)
            .bind(auth.user_id())
            .execute(&state.pool)
            .await?;
    }
    if let Some(rules) = &body.rules {
        if row.2 != "smart" {
            return Err(AppError::BadRequest("only smart shelves have rules".into()));
        }
        rules.validate()?;
        sqlx::query("UPDATE shelf SET rules_json = ? WHERE id = ? AND user_id = ?")
            .bind(serde_json::to_string(rules).unwrap_or_else(|_| "{}".into()))
            .bind(id)
            .bind(auth.user_id())
            .execute(&state.pool)
            .await?;
    }
    let row = shelf(&state.pool, auth.user_id(), id).await?;
    let titles = titles_of(&state.pool, auth.user_id(), &row).await?;
    Ok(Json(json!({ "shelf": summary(&row, &titles) })))
}

pub async fn remove(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let row = shelf(&state.pool, auth.user_id(), id).await?;
    // Hand back what undo needs to put it back exactly.
    let title_ids: Vec<(i64,)> =
        sqlx::query_as("SELECT title_id FROM shelf_item WHERE shelf_id = ? ORDER BY position")
            .bind(id)
            .fetch_all(&state.pool)
            .await?;
    sqlx::query("DELETE FROM shelf WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(auth.user_id())
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({
        "name": row.1,
        "kind": row.2,
        "rules": rules_of(&row.3),
        "title_ids": title_ids.into_iter().map(|(t,)| t).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct OrderBody {
    pub ids: Vec<i64>,
}

pub async fn reorder(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<OrderBody>,
) -> AppResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    for (pos, id) in body.ids.iter().enumerate() {
        sqlx::query("UPDATE shelf SET position = ? WHERE id = ? AND user_id = ?")
            .bind(pos as i64)
            .bind(id)
            .bind(auth.user_id())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn manual(pool: &SqlitePool, user_id: i64, id: i64) -> AppResult<()> {
    let row = shelf(pool, user_id, id).await?;
    if row.2 != "manual" {
        return Err(AppError::BadRequest(
            "a smart shelf fills itself; change its rules instead".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ItemBody {
    /// Frog's id for the title.
    pub title_id: i64,
}

pub async fn add_item(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ItemBody>,
) -> AppResult<Json<Value>> {
    manual(&state.pool, auth.user_id(), id).await?;
    let title_id = catalog::get_title(&state.pool, body.title_id).await?.id;
    sqlx::query(
        "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
         VALUES (?, ?, (SELECT COALESCE(MAX(position), -1) + 1 FROM shelf_item WHERE shelf_id = ?))",
    )
    .bind(id)
    .bind(title_id)
    .bind(id)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_item(
    auth: Auth,
    State(state): State<AppState>,
    Path((id, title_id)): Path<(i64, i64)>,
) -> AppResult<Json<Value>> {
    manual(&state.pool, auth.user_id(), id).await?;
    sqlx::query("DELETE FROM shelf_item WHERE shelf_id = ? AND title_id = ?")
        .bind(id)
        .bind(title_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ItemOrderBody {
    /// Local title ids, in the new order.
    pub title_ids: Vec<i64>,
}

pub async fn reorder_items(
    auth: Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ItemOrderBody>,
) -> AppResult<Json<Value>> {
    manual(&state.pool, auth.user_id(), id).await?;
    let mut tx = state.pool.begin().await?;
    for (pos, title_id) in body.title_ids.iter().enumerate() {
        sqlx::query("UPDATE shelf_item SET position = ? WHERE shelf_id = ? AND title_id = ?")
            .bind(pos as i64)
            .bind(id)
            .bind(title_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// The manual shelves, and whether this title is on each: the title page's
/// "Add to shelf" menu.
pub async fn for_title(
    auth: Auth,
    State(state): State<AppState>,
    Path(title_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let rows: Vec<(i64, String, bool)> = sqlx::query_as(
        "SELECT s.id, s.name,
                EXISTS (SELECT 1 FROM shelf_item i
                          WHERE i.shelf_id = s.id AND i.title_id = ?)
           FROM shelf s
          WHERE s.user_id = ? AND s.kind = 'manual'
          ORDER BY s.position, s.id",
    )
    .bind(title_id)
    .bind(auth.user_id())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "shelves": rows.into_iter().map(|(id, name, on)| json!({ "id": id, "name": name, "on": on })).collect::<Vec<_>>()
    })))
}
