//! What a profile shows of one account, to whoever may see it: who it is,
//! what it is watching, what it rated, and its pictures. Whether a viewer
//! may see it at all is `visible`; everything else reads for the one account
//! that answered.

use std::collections::HashMap;

use chrono::NaiveDate;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

/// (username, display name, me, has a picture)
pub type ListRow = (String, Option<String>, bool, bool);

/// Accounts whose profile is on, plus the viewer's own.
pub async fn listed(pool: &SqlitePool, viewer: i64) -> AppResult<Vec<ListRow>> {
    Ok(sqlx::query_as(
        "SELECT u.username, u.display_name, u.id = ?,
                EXISTS (SELECT 1 FROM user_image i WHERE i.user_id = u.id AND i.kind = 'avatar')
           FROM app_user u
          WHERE u.disabled = 0 AND (u.profile_public = 1 OR u.id = ?)
          ORDER BY u.username COLLATE NOCASE",
    )
    .bind(viewer)
    .bind(viewer)
    .fetch_all(pool)
    .await?)
}

/// (id, username, created_at, profile_public, display name, bio, banner backdrop, pins)
pub type UserRow = (
    i64,
    String,
    String,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

/// The account behind a name, when this viewer may see its profile. A private
/// or switched-off one answers exactly like one that does not exist.
pub async fn visible(pool: &SqlitePool, viewer: i64, username: &str) -> AppResult<UserRow> {
    let found: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, created_at, profile_public, display_name, bio, banner_backdrop,
                pins_json
           FROM app_user WHERE username = ? COLLATE NOCASE AND disabled = 0",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    match found {
        Some(row) if row.3 || row.0 == viewer => Ok(row),
        _ => Err(AppError::NotFound("no such profile".into())),
    }
}

/// (kind, title id, name, poster_path, rating)
pub type CardRow = (String, i64, String, Option<String>, Option<f64>);

/// (films, episodes, shows, minutes)
pub type TotalsRow = (i64, i64, i64, f64);

/// (in library, completed, favourites, mean rating)
pub type LibraryRow = (i64, i64, i64, Option<f64>);

/// Everything a profile page shows of one account.
pub struct Facts {
    pub banner_position: Option<String>,
    pub watching: Vec<CardRow>,
    pub favourites: Vec<CardRow>,
    pub recent: Vec<CardRow>,
    /// Pinned titles, in the order chosen, skipping any no longer tracked.
    pub pinned: Vec<CardRow>,
    /// The same figures as the stats page: watches of titles in the library.
    pub totals: TotalsRow,
    /// Chapters read, of every manga in the library.
    pub chapters: i64,
    pub library: LibraryRow,
    pub genres: Vec<(String, i64)>,
    /// Days with a dated watch, in order, for the streaks.
    pub days: Vec<NaiveDate>,
    /// (kind, updated_at) of each picture uploaded.
    pub images: Vec<(String, String)>,
}

pub async fn facts(pool: &SqlitePool, user_id: i64, pins_json: &str) -> AppResult<Facts> {
    let (banner_position,): (Option<String>,) =
        sqlx::query_as("SELECT banner_position FROM app_user WHERE id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

    let watching: Vec<CardRow> = sqlx::query_as(
        "SELECT t.kind, t.id, t.name, t.poster_path, l.rating
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND l.status = 'watching'
          ORDER BY l.updated_at DESC LIMIT 18",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let favourites: Vec<CardRow> = sqlx::query_as(
        "SELECT t.kind, t.id, t.name, t.poster_path, l.rating
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND l.favourite = 1
          ORDER BY l.rating IS NULL, l.rating DESC, l.updated_at DESC LIMIT 18",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let recent: Vec<CardRow> = sqlx::query_as(
        "SELECT t.kind, t.id, t.name, t.poster_path, l.rating
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = w.user_id
          WHERE w.user_id = ? AND w.undated = 0
          GROUP BY t.id
          ORDER BY MAX(w.watched_at) DESC LIMIT 18",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let pins: Vec<i64> = serde_json::from_str::<Vec<Value>>(pins_json)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| p["title_id"].as_i64())
        .collect();
    let mut pinned = Vec::new();
    for title_id in crate::tmdb::models::unique_by(pins, |id| *id) {
        let row: Option<CardRow> = sqlx::query_as(
            "SELECT t.kind, t.id, t.name, t.poster_path, l.rating
               FROM library_entry l JOIN title t ON t.id = l.title_id
              WHERE l.user_id = ? AND t.id = ?",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_optional(pool)
        .await?;
        pinned.extend(row);
    }

    let totals: TotalsRow = sqlx::query_as(
        "SELECT COALESCE(SUM(w.episode_id IS NULL), 0),
                COALESCE(SUM(w.episode_id IS NOT NULL), 0),
                COUNT(DISTINCT CASE WHEN w.episode_id IS NOT NULL THEN w.title_id END),
                CAST(COALESCE(SUM(COALESCE(e.runtime, t.runtime, 0)), 0) AS REAL)
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let (chapters,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(l.progress), 0) FROM library_entry l
           JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND t.kind = 'manga'",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let library: LibraryRow = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(status = 'completed'), 0), COALESCE(SUM(favourite), 0),
                AVG(rating)
           FROM library_entry WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let genres: Vec<(String, i64)> = sqlx::query_as(
        "SELECT g.value, COUNT(*) FROM library_entry l
           JOIN title t ON t.id = l.title_id, json_each(t.genres_json) g
          WHERE l.user_id = ? AND l.status IN ('watching', 'completed')
          GROUP BY g.value ORDER BY 2 DESC, 1 LIMIT 5",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let days: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT substr(watched_at, 1, 10) FROM watch_event
          WHERE user_id = ? AND undated = 0 ORDER BY 1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let days = days
        .into_iter()
        .filter_map(|(d,)| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .collect();
    let images: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, updated_at FROM user_image WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await?;

    Ok(Facts {
        banner_position,
        watching,
        favourites,
        recent,
        pinned,
        totals,
        chapters,
        library,
        genres,
        days,
        images,
    })
}

/// How many chapters a manga has, as its title keeps it.
pub async fn chapter_count(pool: &SqlitePool, title_id: i64) -> AppResult<i64> {
    let (total,): (Option<i64>,) = sqlx::query_as("SELECT episode_count FROM title WHERE id = ?")
        .bind(title_id)
        .fetch_one(pool)
        .await?;
    Ok(total.unwrap_or(0))
}

/// (kind, id, name, poster, status, rating)
pub type RatedRow = (String, i64, String, Option<String>, String, Option<f64>);

/// Everything an account rated, and every season rating it has, by title:
/// (title, season, first episode, rating).
pub async fn rated(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<(Vec<RatedRow>, Vec<(i64, i64, i64, f64)>)> {
    let seasons: Vec<(i64, i64, i64, f64)> = sqlx::query_as(
        "SELECT title_id, season_number, first_episode, rating FROM season_rating
          WHERE user_id = ? ORDER BY title_id, season_number, first_episode",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let rows: Vec<RatedRow> = sqlx::query_as(
        "SELECT t.kind, t.id, t.name, t.poster_path, l.status, l.rating
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?
            AND (l.rating IS NOT NULL
                 OR EXISTS (SELECT 1 FROM season_rating s
                             WHERE s.user_id = l.user_id AND s.title_id = l.title_id))",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok((rows, seasons))
}

/// (kind, name, poster) of each title, by Frog's id.
pub async fn titles_by_id(
    pool: &SqlitePool,
    ids: &[i64],
) -> AppResult<HashMap<i64, (String, String, Option<String>)>> {
    let mut titles = HashMap::new();
    for chunk in ids.chunks(400) {
        let marks = vec!["?"; chunk.len()].join(", ");
        let sql = format!("SELECT id, kind, name, poster_path FROM title WHERE id IN ({marks})");
        let mut q = sqlx::query_as::<_, (i64, String, String, Option<String>)>(&sql);
        for id in chunk {
            q = q.bind(id);
        }
        for (id, kind, name, poster) in q.fetch_all(pool).await? {
            titles.insert(id, (kind, name, poster));
        }
    }
    Ok(titles)
}

/// A profile picture or banner: (mime, bytes, updated_at).
pub async fn picture(
    pool: &SqlitePool,
    user_id: i64,
    kind: &str,
) -> AppResult<Option<(String, Vec<u8>, String)>> {
    Ok(sqlx::query_as(
        "SELECT mime, bytes, updated_at FROM user_image WHERE user_id = ? AND kind = ?",
    )
    .bind(user_id)
    .bind(kind)
    .fetch_optional(pool)
    .await?)
}
