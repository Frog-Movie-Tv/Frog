//! Studios as Frog keeps them: those credited on titles it holds, and the
//! logo TMDB has for a studio AniList names (`studio_match`).

use chrono::Utc;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::anilist;
use crate::error::AppResult;
use crate::state::AppState;
use crate::tmdb::models;

/// (id, name, logo, titles)
pub type KnownStudioRow = (i64, String, Option<String>, i64);

/// Studios credited on titles this Frog holds whose name, or a word of it,
/// starts with `pattern`; the most credited first.
pub async fn known(pool: &SqlitePool, pattern: &str) -> AppResult<Vec<KnownStudioRow>> {
    Ok(sqlx::query_as(
        "SELECT tmdb_id, name, MAX(profile_path), COUNT(*)
           FROM title_person
          WHERE role = 'studio' AND (name LIKE ?1 || '%' OR name LIKE '% ' || ?1 || '%')
          GROUP BY tmdb_id
          ORDER BY COUNT(*) DESC
          LIMIT 8",
    )
    .bind(pattern)
    .fetch_all(pool)
    .await?)
}

/// Which of these TMDB companies are credited on a title this Frog holds.
pub async fn held(pool: &SqlitePool, ids: &[i64]) -> AppResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT tmdb_id FROM title_person WHERE role = 'studio' AND tmdb_id IN
           (SELECT value FROM json_each(?))",
    )
    .bind(serde_json::to_string(ids).unwrap_or_default())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// The logo of the TMDB company with exactly this name, for a studio AniList
/// names. AniList has no pictures of studios. Only a company whose whole name
/// is the same counts, and a studio that has none is remembered as having none.
pub async fn logo(state: &AppState, name: &str) -> Option<String> {
    const RECHECK_DAYS: i64 = 30;
    let key = anilist::plain(name);
    if key.is_empty() {
        return None;
    }
    let kept: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT logo_path, checked_at FROM studio_match WHERE name_key = ?")
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    let cutoff = (Utc::now() - chrono::Duration::days(RECHECK_DAYS)).to_rfc3339();
    if let Some((logo, _)) = kept.as_ref().filter(|(_, at)| *at > cutoff) {
        return logo.clone();
    }

    let raw = state.tmdb.search_company(name).await.ok()?;
    let same: Vec<&Value> = raw["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["name"].as_str().map(anilist::plain).as_deref() == Some(key.as_str()))
        .filter(|c| models::str_at(c, "logo_path").is_some())
        .collect();
    // AniList is anime, so a Japanese company of that name comes first.
    let logo = same
        .iter()
        .find(|c| c["origin_country"].as_str() == Some("JP"))
        .or(same.first())
        .and_then(|c| models::str_at(c, "logo_path"));
    let stored = sqlx::query(
        "INSERT INTO studio_match (name_key, logo_path, checked_at) VALUES (?, ?, ?)
         ON CONFLICT (name_key) DO UPDATE SET
            logo_path = excluded.logo_path, checked_at = excluded.checked_at",
    )
    .bind(&key)
    .bind(&logo)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await;
    if let Err(e) = stored {
        tracing::debug!(error = %e, "studio logo not remembered");
    }
    logo
}
