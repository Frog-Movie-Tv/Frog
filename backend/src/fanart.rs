//! Title logos TMDB has none of.
//!
//! A title's logo comes from TMDB. When it has none, this finds one:
//!
//! 1. fanart.tv, for anything TMDB knows: films by TMDB's id, shows by
//!    TheTVDB's (from TMDB's external ids). For an anime Frog keeps from
//!    AniList that is the TMDB entry `anime_map` points at.
//! 2. The show it belongs to. AniList knows a third season's prequel and a
//!    special's parent; their logo is this one's too. The chain is followed a
//!    few steps back, until a step has a logo in Frog, on TMDB or on fanart.tv.
//!
//! Only an empty `title.logo_path` is ever filled, stored as
//! `/fanart/<path>` for fanart.tv's files or TMDB's own path for a parent's,
//! and served through `images` like every other picture. TMDB still wins:
//! `catalog` writes `COALESCE(new, old)`. `title_logo_check` (shared) says when
//! a title may be asked again.

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::anilist::Api;
use crate::catalog::{self, ExternalId};
use crate::state::AppState;
use crate::tmdb::models;

/// How long a title page waits for a logo the first time it is opened.
const FIRST_LOOK: std::time::Duration = std::time::Duration::from_millis(2500);
/// Titles the worker looks at in one pass.
const BATCH: i64 = 10;
/// Steps back through prequels and parents before giving up.
const ANCESTORS: usize = 5;
/// A title nothing was found for waits this long before it is asked again.
const RETRY_DAYS: i64 = 30;

/// fanart.tv's address for a file: the full picture, or its small preview
/// for the sizes a list asks for.
pub fn asset_url(base: &str, rest: &str, size: &str) -> String {
    let folder = if matches!(size, "w92" | "w154" | "w185") {
        "preview"
    } else {
        "fanart"
    };
    format!("{}/{folder}/{rest}", base.trim_end_matches('/'))
}

/// The best logo in fanart.tv's answer, as the path under its `fanart/`
/// folder. Wide HD logos first; English, then text-free, then any language;
/// within those the one people liked most.
pub fn pick(answer: &Value, kind: &str) -> Option<String> {
    let (hd, plain) = if kind == "movie" {
        ("hdmovielogo", "movielogo")
    } else {
        ("hdtvlogo", "clearlogo")
    };
    let mut best: Option<(i64, String)> = None;
    for (field, bonus) in [(hd, 1000), (plain, 0)] {
        for logo in answer[field].as_array().into_iter().flatten() {
            let Some(url) = logo["url"].as_str() else {
                continue;
            };
            let Some((_, rest)) = url.split_once("/fanart/") else {
                continue;
            };
            let lang = match logo["lang"].as_str() {
                Some("en") => 300,
                Some("00") | Some("") | None => 200,
                Some("ja") => 0,
                Some(_) => 100,
            };
            let likes = logo["likes"]
                .as_str()
                .and_then(|l| l.parse::<i64>().ok())
                .or_else(|| logo["likes"].as_i64())
                .unwrap_or(0)
                .min(99);
            let score = bonus + lang + likes;
            if best.as_ref().is_none_or(|(b, _)| score > *b) {
                best = Some((score, rest.to_string()));
            }
        }
    }
    best.map(|(_, rest)| format!("/fanart/{rest}"))
}

/// The entry an AniList title belongs to: its parent (a special's show),
/// else its prequel (the season before).
pub fn ancestor(media: &Value) -> Option<i64> {
    let edges: Vec<&Value> = media["relations"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["node"]["type"].as_str() == Some("ANIME"))
        .collect();
    ["PARENT", "PREQUEL"].iter().find_map(|wanted| {
        edges
            .iter()
            .find(|e| e["relationType"].as_str() == Some(wanted))
            .and_then(|e| e["node"]["id"].as_i64())
    })
}

const RELATIONS_QUERY: &str = "query ($id: Int) {
  Media(id: $id, type: ANIME) { relations { edges { relationType node { id type } } } }
}";

/// fanart.tv's logo for a TMDB entry. Shows need TheTVDB's id, which TMDB's
/// own detail carries.
async fn from_fanart(state: &AppState, kind: &str, tmdb_id: i64) -> Option<String> {
    let client = state.config.fanart.as_ref()?;
    let path = if kind == "movie" {
        format!("/movies/{tmdb_id}")
    } else {
        let detail = state.tmdb.tv(tmdb_id).await.ok()?;
        let tvdb = detail["external_ids"]["tvdb_id"].as_i64()?;
        format!("/tv/{tvdb}")
    };
    let resp = state
        .http
        .get(format!("{}{path}", client.api_url.trim_end_matches('/')))
        .header("api-key", &client.key)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| tracing::debug!(error = %e.without_url(), "fanart.tv not reached"))
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let answer: Value = resp.json().await.ok()?;
    pick(&answer, kind)
}

/// TMDB's own logo for an entry: what a borrowed id or a parent show has.
async fn from_tmdb(state: &AppState, kind: &str, tmdb_id: i64) -> Option<String> {
    let detail = match kind {
        "movie" => state.tmdb.movie(tmdb_id).await,
        _ => state.tmdb.tv(tmdb_id).await,
    }
    .ok()?;
    models::TitleCore::from_detail(kind, &detail)?.logo_path
}

/// A logo for a TMDB entry, TMDB's own first.
async fn for_tmdb(state: &AppState, kind: &str, tmdb_id: i64) -> Option<String> {
    match from_tmdb(state, kind, tmdb_id).await {
        Some(logo) => Some(logo),
        None => from_fanart(state, kind, tmdb_id).await,
    }
}

/// The TMDB entry the community list gives an AniList one.
async fn mapped(state: &AppState, anilist_id: i64) -> Option<(String, i64)> {
    sqlx::query_as(
        "SELECT tmdb_kind, tmdb_id FROM anime_map
          WHERE anilist_id = ? AND tmdb_kind IN ('movie', 'tv') AND tmdb_id IS NOT NULL
          LIMIT 1",
    )
    .bind(anilist_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
}

/// A logo for one AniList entry, without looking further back: Frog's own
/// title for it, or its TMDB entry.
async fn for_anilist(state: &AppState, anilist_id: i64) -> Option<String> {
    if let Ok(Some(title_id)) = catalog::find(&state.pool, &ExternalId::AniList(anilist_id)).await {
        let logo: Option<(Option<String>,)> =
            sqlx::query_as("SELECT logo_path FROM title WHERE id = ?")
                .bind(title_id)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
        if let Some((Some(logo),)) = logo {
            return Some(logo);
        }
    }
    let (kind, tmdb_id) = mapped(state, anilist_id).await?;
    for_tmdb(state, &kind, tmdb_id).await
}

/// Look for a title's logo everywhere this module knows.
async fn find(state: &AppState, title_id: i64) -> Option<String> {
    let external = catalog::externals(&state.pool, title_id).await.ok()?;
    let anilist = external.anilist.first().copied();

    // Its own TMDB entry, or the one AniList's maps to.
    let tmdb = match (&external.tmdb, anilist) {
        (Some(t), _) => Some((t.kind.clone(), t.id)),
        (None, Some(id)) => mapped(state, id).await,
        (None, None) => None,
    };
    if let Some((kind, id)) = &tmdb {
        if let Some(logo) = for_tmdb(state, kind, *id).await {
            return Some(logo);
        }
    }

    // The show it belongs to, a few steps back.
    let mut current = anilist?;
    let api = Api::new(state).ok()?;
    for _ in 0..ANCESTORS {
        let media = api
            .graphql("", RELATIONS_QUERY, json!({ "id": current }))
            .await
            .ok()?;
        current = ancestor(&media["Media"])?;
        if let Some(logo) = for_anilist(state, current).await {
            return Some(logo);
        }
    }
    None
}

/// Find a logo for one title and keep it, or note that there was none.
async fn fill(state: &AppState, title_id: i64) -> Option<String> {
    let logo = find(state, title_id).await;
    let now = Utc::now();
    if let Some(logo) = &logo {
        let stored =
            sqlx::query("UPDATE title SET logo_path = COALESCE(logo_path, ?) WHERE id = ?")
                .bind(logo)
                .bind(title_id)
                .execute(&state.pool)
                .await;
        if let Err(e) = stored {
            tracing::warn!(title_id, error = %e, "logo not stored");
        }
    }
    let next = if logo.is_some() {
        now + Duration::days(365)
    } else {
        now + Duration::days(RETRY_DAYS)
    };
    let noted = sqlx::query(
        "INSERT INTO title_logo_check (title_id, checked_at, next_at) VALUES (?, ?, ?)
         ON CONFLICT (title_id) DO UPDATE SET
            checked_at = excluded.checked_at, next_at = excluded.next_at",
    )
    .bind(title_id)
    .bind(now.to_rfc3339())
    .bind(next.to_rfc3339())
    .execute(&state.pool)
    .await;
    if let Err(e) = noted {
        tracing::warn!(title_id, error = %e, "logo check not noted");
    }
    logo
}

/// A title page's logo, the first time anyone opens it without one: a quick
/// look, held to a moment so the page never waits on another site.
pub async fn first_look(state: &AppState, title_id: i64) -> Option<String> {
    let checked: Option<(String,)> =
        sqlx::query_as("SELECT next_at FROM title_logo_check WHERE title_id = ?")
            .bind(title_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if checked.is_some_and(|(next,)| next > Utc::now().to_rfc3339()) {
        return None;
    }
    let state = state.clone();
    let look = tokio::spawn(async move { fill(&state, title_id).await });
    tokio::time::timeout(FIRST_LOOK, look)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
}

/// Tracked titles without a logo whose turn has come, a few at a time.
pub async fn fill_due(state: &AppState) {
    let due: Vec<(i64,)> = sqlx::query_as(
        "SELECT t.id FROM title t LEFT JOIN title_logo_check c ON c.title_id = t.id
          WHERE t.logo_path IS NULL AND t.merged_into IS NULL
            AND (c.next_at IS NULL OR c.next_at <= ?)
            AND EXISTS (SELECT 1 FROM library_entry l WHERE l.title_id = t.id)
          ORDER BY c.next_at IS NOT NULL, c.next_at
          LIMIT ?",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(BATCH)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    for (title_id,) in due {
        fill(state, title_id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_english_hd_logo_wins_over_a_liked_japanese_one() {
        let answer = json!({
            "hdtvlogo": [
                { "url": "https://assets.fanart.tv/fanart/tv/1/hdtvlogo/ja.png", "lang": "ja", "likes": "40" },
                { "url": "https://assets.fanart.tv/fanart/tv/1/hdtvlogo/en.png", "lang": "en", "likes": "2" }
            ],
            "clearlogo": [
                { "url": "https://assets.fanart.tv/fanart/tv/1/clearlogo/old.png", "lang": "en", "likes": "90" }
            ]
        });
        assert_eq!(
            pick(&answer, "tv").as_deref(),
            Some("/fanart/tv/1/hdtvlogo/en.png")
        );
    }

    #[test]
    fn a_film_reads_its_own_fields_and_nothing_is_nothing() {
        let answer = json!({
            "movielogo": [{ "url": "https://assets.fanart.tv/fanart/movies/9/movielogo/a.png", "lang": "00" }]
        });
        assert_eq!(
            pick(&answer, "movie").as_deref(),
            Some("/fanart/movies/9/movielogo/a.png")
        );
        assert_eq!(pick(&answer, "tv"), None);
        assert_eq!(pick(&json!({}), "movie"), None);
    }

    #[test]
    fn a_special_goes_to_its_show_and_a_season_to_the_one_before() {
        let special = json!({ "relations": { "edges": [
            { "relationType": "PREQUEL", "node": { "id": 2, "type": "ANIME" } },
            { "relationType": "PARENT", "node": { "id": 1, "type": "ANIME" } }
        ] } });
        assert_eq!(ancestor(&special), Some(1));
        let season = json!({ "relations": { "edges": [
            { "relationType": "ADAPTATION", "node": { "id": 5, "type": "MANGA" } },
            { "relationType": "PREQUEL", "node": { "id": 3, "type": "ANIME" } },
            { "relationType": "SEQUEL", "node": { "id": 4, "type": "ANIME" } }
        ] } });
        assert_eq!(ancestor(&season), Some(3));
        let first = json!({ "relations": { "edges": [
            { "relationType": "PARENT", "node": { "id": 8, "type": "MANGA" } }
        ] } });
        assert_eq!(ancestor(&first), None);
    }

    #[test]
    fn lists_get_the_small_preview() {
        assert_eq!(
            asset_url("https://assets.fanart.tv/", "tv/1/hdtvlogo/a.png", "w92"),
            "https://assets.fanart.tv/preview/tv/1/hdtvlogo/a.png"
        );
        assert_eq!(
            asset_url("https://assets.fanart.tv", "tv/1/hdtvlogo/a.png", "w500"),
            "https://assets.fanart.tv/fanart/tv/1/hdtvlogo/a.png"
        );
    }
}
