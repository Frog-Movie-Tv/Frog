//! The two copies of a library an account can take away: a complete JSON
//! backup that Frog can read back (`frog-library`), and a flat CSV for
//! anything that reads spreadsheets. Both formats are a contract with files
//! people already hold, so change them only by adding.
//!
//! Version 2 of the backup adds each title's Frog id and every provider id it
//! is known by (`ids`). Every version 1 field is still there; `tmdb_id` is
//! null for an anime TMDB lacks. A Frog id is only ever a label: an import,
//! here or on another Frog, goes by provider ids and names, never by it.

use chrono::Utc;
use serde_json::{json, Value};

use crate::catalog::{self, ExternalId};
use crate::error::AppResult;
use crate::state::AppState;

/// (title id, kind, TMDB id as `title_external` holds it, name, release
///  date, status, rating, favourite, notes, added, watches)
type ExportRow = (
    i64,
    String,
    Option<String>,
    String,
    Option<String>,
    String,
    Option<f64>,
    bool,
    Option<String>,
    String,
    i64,
);

/// (season, episode, my rating, my notes, watch count)
type EpisodeExportRow = (i64, i64, Option<f64>, Option<String>, i64);

/// (watched_at, is_rewatch, source)
type WatchExportRow = (String, i64, String);

async fn export_rows(state: &AppState, user_id: i64) -> AppResult<Vec<ExportRow>> {
    Ok(sqlx::query_as(
        "SELECT t.id, t.kind, x.external_id, t.name, t.release_date, l.status, l.rating,
                l.favourite, l.notes, l.added_at,
                (SELECT COUNT(*) FROM watch_event w
                  WHERE w.user_id = ?1 AND w.title_id = t.id)
           FROM library_entry l JOIN title t ON t.id = l.title_id
           LEFT JOIN title_external x
                  ON x.title_id = t.id AND x.provider = 'tmdb' AND x.is_primary = 1
          WHERE l.user_id = ?1
          ORDER BY t.name COLLATE NOCASE",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?)
}

/// The number in a TMDB id as `title_external` holds it.
fn tmdb_id_of(key: Option<&str>) -> Option<i64> {
    key.and_then(|k| ExternalId::parse("tmdb", k))
        .and_then(|x| x.tmdb_id())
}

/// A complete backup: everything needed to rebuild the library elsewhere.
pub async fn library_json(state: &AppState, user_id: i64) -> AppResult<Value> {
    let rows = export_rows(state, user_id).await?;

    let mut titles = Vec::with_capacity(rows.len());
    for (title_id, kind, tmdb, name, release_date, status, rating, favourite, notes, added_at, _) in
        rows
    {
        let ids = catalog::externals(&state.pool, title_id).await?;
        let episodes: Vec<EpisodeExportRow> = sqlx::query_as(
            "SELECT e.season_number, e.episode_number, s.rating, s.notes,
                    (SELECT COUNT(*) FROM watch_event w
                      WHERE w.user_id = ?1 AND w.episode_id = e.id)
               FROM episode e
               LEFT JOIN episode_state s ON s.episode_id = e.id AND s.user_id = ?1
              WHERE e.title_id = ?2
                AND (s.rating IS NOT NULL OR s.notes IS NOT NULL
                     OR EXISTS (SELECT 1 FROM watch_event w
                                 WHERE w.user_id = ?1 AND w.episode_id = e.id))
              ORDER BY e.season_number, e.episode_number",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_all(&state.pool)
        .await?;

        let watches: Vec<WatchExportRow> = sqlx::query_as(
            "SELECT w.watched_at, w.is_rewatch, w.source
               FROM watch_event w
              WHERE w.user_id = ? AND w.title_id = ? AND w.episode_id IS NULL
              ORDER BY w.watched_at",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_all(&state.pool)
        .await?;

        titles.push(json!({
            "id": title_id,
            "ids": {
                "tmdb": ids.tmdb,
                "anilist": ids.anilist,
                "imdb": ids.imdb,
            },
            "kind": kind,
            "tmdb_id": tmdb_id_of(tmdb.as_deref()),
            "name": name,
            "release_date": release_date,
            "status": status,
            "rating": rating,
            "favourite": favourite,
            "notes": notes,
            "added_at": added_at,
            "episodes": episodes
                .into_iter()
                .map(|(s, e, rating, notes, watched)| json!({
                    "season": s, "episode": e, "rating": rating,
                    "notes": notes, "watched": watched > 0,
                }))
                .collect::<Vec<_>>(),
            "watches": watches
                .into_iter()
                .map(|(at, rewatch, source)| json!({
                    "watched_at": at, "rewatch": rewatch == 1, "source": source,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let settings: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = 'preferences'")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;

    Ok(json!({
        "format": "frog-library",
        "version": 2,
        "exported_at": Utc::now().to_rfc3339(),
        "app_version": env!("CARGO_PKG_VERSION"),
        "settings": settings
            .and_then(|(v,)| serde_json::from_str::<Value>(&v).ok())
            .unwrap_or_else(|| json!({})),
        "titles": titles,
    }))
}

/// A flat sheet, one row per title. Frog's and AniList's ids came later, so
/// they come last.
pub async fn library_csv(state: &AppState, user_id: i64) -> AppResult<String> {
    let rows = export_rows(state, user_id).await?;

    let mut out = String::from(
        "Type,TMDB ID,Title,Year,Status,Rating,Favourite,Added,Watches,Frog ID,AniList ID\n",
    );
    for (
        title_id,
        kind,
        tmdb,
        name,
        release_date,
        status,
        rating,
        favourite,
        _,
        added_at,
        watches,
    ) in rows
    {
        let anilist = catalog::externals(&state.pool, title_id).await?.anilist;
        let year = release_date.unwrap_or_default();
        let year = year.get(0..4).unwrap_or("");
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            if kind == "tv" { "Show" } else { "Film" },
            tmdb_id_of(tmdb.as_deref())
                .map(|id| id.to_string())
                .unwrap_or_default(),
            csv_field(&name),
            year,
            status,
            rating.map(|r| format!("{r:.0}")).unwrap_or_default(),
            if favourite { "yes" } else { "no" },
            added_at.get(0..10).unwrap_or(""),
            watches,
            title_id,
            anilist.first().map(|id| id.to_string()).unwrap_or_default(),
        ));
    }
    Ok(out)
}

/// Quote anything that would otherwise break a cell.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_only_what_needs_it() {
        assert_eq!(csv_field("Heat"), "Heat");
        assert_eq!(
            csv_field("Crouching Tiger, Hidden Dragon"),
            "\"Crouching Tiger, Hidden Dragon\""
        );
        assert_eq!(csv_field("He said \"hi\""), "\"He said \"\"hi\"\"\"");
    }

    #[test]
    fn only_a_well_formed_tmdb_id_reads_as_one() {
        assert_eq!(tmdb_id_of(Some("tv/1399")), Some(1399));
        assert_eq!(tmdb_id_of(Some("1399")), None);
        assert_eq!(tmdb_id_of(None), None);
    }
}
