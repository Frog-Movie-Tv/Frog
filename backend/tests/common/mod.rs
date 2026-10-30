//! What more than one test file needs to set a database up.
#![allow(dead_code)]

use frog::catalog::{self, ExternalId};
use frog::state::AppState;

/// Lets a test write a title the way a provider names it, in one statement:
/// `INSERT INTO title_seed (kind, tmdb_id, name, …)`. The title gets its
/// provider row as `catalog` would give it: TMDB for a positive id, AniList
/// for a negative one (how AniList-only titles were addressed before Frog had
/// ids of its own). Test databases only; the app never sees this view.
pub async fn add_title_seed(state: &AppState) {
    for sql in [
        "CREATE VIEW IF NOT EXISTS title_seed AS
         SELECT id, kind, 0 AS tmdb_id, name, original_name, overview, poster_path,
                backdrop_path, release_date, runtime, genres_json, tmdb_status,
                episode_count, season_count, NULL AS anilist_id
           FROM title",
        "CREATE TRIGGER IF NOT EXISTS title_seed_insert INSTEAD OF INSERT ON title_seed
         BEGIN
           INSERT INTO title (id, kind, name, original_name, overview, poster_path,
                              backdrop_path, release_date, runtime, genres_json, tmdb_status,
                              episode_count, season_count)
           VALUES (NEW.id, NEW.kind, NEW.name, NEW.original_name, NEW.overview,
                   NEW.poster_path, NEW.backdrop_path, NEW.release_date, NEW.runtime,
                   COALESCE(NEW.genres_json, '[]'), NEW.tmdb_status, NEW.episode_count,
                   NEW.season_count);
           INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
           VALUES (COALESCE(NEW.id, (SELECT MAX(id) FROM title)),
                   CASE WHEN NEW.tmdb_id > 0 THEN 'tmdb' ELSE 'anilist' END,
                   CASE WHEN NEW.tmdb_id > 0 THEN NEW.kind || '/' || NEW.tmdb_id
                        ELSE CAST(-NEW.tmdb_id AS TEXT) END,
                   1, '2026-01-01T00:00:00Z');
         END",
        // What a test reads back to say which TMDB title a row is.
        "CREATE VIEW IF NOT EXISTS title_tmdb AS
         SELECT title_id,
                substr(external_id, 1, instr(external_id, '/') - 1) AS kind,
                CAST(substr(external_id, instr(external_id, '/') + 1) AS INTEGER) AS tmdb_id
           FROM title_external WHERE provider = 'tmdb'",
    ] {
        sqlx::query(sql)
            .execute(&state.pool)
            .await
            .expect("title_seed");
    }
}

/// Frog's id for a title as a provider names it (a negative id is AniList's),
/// putting it in the catalog first if need be, as opening it would.
pub async fn title_id(state: &AppState, kind: &str, id: i64) -> i64 {
    let source = if id > 0 {
        ExternalId::tmdb(kind, id)
    } else {
        ExternalId::AniList(-id)
    };
    if let Some(found) = catalog::find(&state.pool, &source).await.expect("lookup") {
        return found;
    }
    catalog::ensure(state, &source)
        .await
        .unwrap_or_else(|e| panic!("{kind} {id} could not be opened: {e}"))
        .0
}

/// Frog's id for a title already in the catalog, by TMDB id (any kind) or,
/// negative, AniList id. Never creates one, so "not there" stays testable.
pub async fn existing_id(state: &AppState, id: i64) -> Option<i64> {
    if id < 0 {
        // The AniList title itself, not where a merge took it.
        return catalog::find_exact(&state.pool, &ExternalId::AniList(-id))
            .await
            .expect("lookup");
    }
    sqlx::query_as::<_, (i64,)>("SELECT title_id FROM title_tmdb WHERE tmdb_id = ? LIMIT 1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .expect("lookup")
        .map(|r| r.0)
}
