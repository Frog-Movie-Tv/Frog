//! The catalog: every film and show Frog knows, shared by every account.
//!
//! A title is Frog's own row and its id, `title.id`, is what everything
//! personal points at. Which providers know a title, and as what, is
//! `title_external`: a TMDB title is `tmdb movie/550`, an anime TMDB lacks is
//! `anilist 21`, and TMDB's IMDb id comes along as `imdb tt0113277`. One of a
//! title's providers is primary and supplies its metadata; a title can also
//! have none.
//!
//! - One provider id never names two titles (it is the table's key), and
//!   `link` refuses to move one silently.
//! - A provider that no longer has a title only marks it `gone_at`; the title
//!   and everyone's history stay.
//! - Two titles found to be one are merged per account by
//!   `store::move_title_data`, which points the old title at the new one with
//!   `merged_into` once nobody keeps it. `find` follows that, so the old
//!   title's provider ids keep resolving, to the survivor.
//!
//! `docs/CATALOG-IDENTITY.md` has the reasoning.

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

use crate::error::{AppError, AppResult};
use crate::palette::{self, Palette};
use crate::state::AppState;
use crate::text;
use crate::tmdb::models::{self, EpisodeInfo, TitleCore};

/// How many `merged_into` steps `find` follows before giving up on a cycle.
const MAX_MERGE_HOPS: usize = 8;

/// A title as one provider knows it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExternalId {
    /// TMDB's ids are unique per kind only.
    Tmdb {
        kind: String,
        id: i64,
    },
    AniList(i64),
    Imdb(String),
}

impl ExternalId {
    pub fn tmdb(kind: &str, id: i64) -> Self {
        ExternalId::Tmdb {
            kind: kind.to_string(),
            id,
        }
    }

    pub fn provider(&self) -> &'static str {
        match self {
            ExternalId::Tmdb { .. } => "tmdb",
            ExternalId::AniList(_) => "anilist",
            ExternalId::Imdb(_) => "imdb",
        }
    }

    /// The id as `title_external.external_id` holds it.
    pub fn key(&self) -> String {
        match self {
            ExternalId::Tmdb { kind, id } => format!("{kind}/{id}"),
            ExternalId::AniList(id) => id.to_string(),
            ExternalId::Imdb(id) => id.clone(),
        }
    }

    pub fn parse(provider: &str, key: &str) -> Option<Self> {
        match provider {
            "tmdb" => {
                let (kind, id) = key.split_once('/')?;
                (kind == "movie" || kind == "tv").then_some(())?;
                Some(ExternalId::tmdb(kind, id.parse().ok().filter(|n| *n > 0)?))
            }
            "anilist" => Some(ExternalId::AniList(
                key.parse().ok().filter(|n: &i64| *n > 0)?,
            )),
            "imdb" => key
                .starts_with("tt")
                .then(|| ExternalId::Imdb(key.to_string())),
            _ => None,
        }
    }

    /// The TMDB id, when this is one.
    pub fn tmdb_id(&self) -> Option<i64> {
        match self {
            ExternalId::Tmdb { id, .. } => Some(*id),
            _ => None,
        }
    }
}

/// Every provider id of a title, as the API and backups give them:
/// `{ "tmdb": { "kind": "tv", "id": 1399 }, "anilist": [21], "imdb": ["tt…"] }`.
/// AniList and IMDb are lists: a title merged from others answers to theirs.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct Externals {
    pub tmdb: Option<TmdbRef>,
    pub anilist: Vec<i64>,
    pub imdb: Vec<String>,
    /// The provider the metadata comes from: "tmdb", "anilist", or none.
    pub primary: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TmdbRef {
    pub kind: String,
    pub id: i64,
}

impl Externals {
    fn add(&mut self, id: ExternalId, primary: bool) {
        if primary {
            self.primary = Some(id.provider());
        }
        match id {
            // A title has one TMDB entry; a merged one's own stays its own.
            ExternalId::Tmdb { kind, id } if self.tmdb.is_none() || primary => {
                self.tmdb = Some(TmdbRef { kind, id })
            }
            ExternalId::Tmdb { .. } => {}
            ExternalId::AniList(id) if !self.anilist.contains(&id) => self.anilist.push(id),
            ExternalId::AniList(_) => {}
            ExternalId::Imdb(id) if !self.imdb.contains(&id) => self.imdb.push(id),
            ExternalId::Imdb(_) => {}
        }
    }
}

// --- identity -----------------------------------------------------------------

/// The title a provider id names, following merges to the title that took
/// its place.
pub async fn find(pool: &SqlitePool, id: &ExternalId) -> AppResult<Option<i64>> {
    let Some(mut title) = find_exact(pool, id).await? else {
        return Ok(None);
    };
    for _ in 0..MAX_MERGE_HOPS {
        let (next,): (Option<i64>,) = sqlx::query_as("SELECT merged_into FROM title WHERE id = ?")
            .bind(title)
            .fetch_one(pool)
            .await?;
        match next {
            Some(n) if n != title => title = n,
            _ => break,
        }
    }
    Ok(Some(title))
}

/// The title a provider id is linked to, merged or not.
pub async fn find_exact(pool: &SqlitePool, id: &ExternalId) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT title_id FROM title_external WHERE provider = ? AND external_id = ?",
    )
    .bind(id.provider())
    .bind(id.key())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

/// The provider a title's metadata comes from.
pub async fn primary(pool: &SqlitePool, title_id: i64) -> AppResult<Option<ExternalId>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider, external_id FROM title_external WHERE title_id = ? AND is_primary = 1",
    )
    .bind(title_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(p, k)| ExternalId::parse(&p, &k)))
}

/// Every provider id a title answers to: its own, then those of titles
/// merged into it.
pub async fn externals(pool: &SqlitePool, title_id: i64) -> AppResult<Externals> {
    let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT x.provider, x.external_id, x.is_primary, x.title_id = ?1
           FROM title_external x
          WHERE x.title_id = ?1
             OR x.title_id IN (SELECT id FROM title WHERE merged_into = ?1)
          ORDER BY x.title_id = ?1 DESC, x.is_primary DESC, x.added_at",
    )
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    let mut out = Externals::default();
    for (provider, key, is_primary, own) in rows {
        if let Some(id) = ExternalId::parse(&provider, &key) {
            out.add(id, is_primary && own);
        }
    }
    Ok(out)
}

/// Say that `id` names `title_id`. Linking what is already linked to the same
/// title only clears `gone_at`; a provider id that names another title is
/// refused, never moved. A primary link is refused when the title already
/// has a different primary.
async fn link(
    conn: &mut SqliteConnection,
    title_id: i64,
    id: &ExternalId,
    primary: bool,
) -> AppResult<()> {
    let existing: Option<(i64, bool)> = sqlx::query_as(
        "SELECT title_id, is_primary FROM title_external WHERE provider = ? AND external_id = ?",
    )
    .bind(id.provider())
    .bind(id.key())
    .fetch_optional(&mut *conn)
    .await?;
    match existing {
        Some((owner, _)) if owner != title_id => {
            return Err(AppError::Conflict(format!(
                "{} {} already belongs to another title",
                id.provider(),
                id.key()
            )));
        }
        Some((_, was_primary)) => {
            if primary && !was_primary {
                return Err(AppError::Conflict(
                    "a title's provider is not changed by linking".into(),
                ));
            }
            sqlx::query(
                "UPDATE title_external SET gone_at = NULL WHERE provider = ? AND external_id = ?",
            )
            .bind(id.provider())
            .bind(id.key())
            .execute(&mut *conn)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(title_id)
            .bind(id.provider())
            .bind(id.key())
            .bind(primary)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// The provider no longer has this id. The title and its history stay.
pub async fn mark_gone(pool: &SqlitePool, id: &ExternalId) -> AppResult<()> {
    sqlx::query(
        "UPDATE title_external SET gone_at = COALESCE(gone_at, ?)
          WHERE provider = ? AND external_id = ?",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(id.provider())
    .bind(id.key())
    .execute(pool)
    .await?;
    Ok(())
}

// --- titles ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TitleRow {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub release_date: Option<String>,
    pub last_air_date: Option<String>,
    pub tmdb_status: Option<String>,
    pub runtime: Option<i64>,
    pub episode_count: Option<i64>,
    pub season_count: Option<i64>,
    pub genres_json: String,
    pub tmdb_vote: Option<f64>,
    pub collection_id: Option<i64>,
    pub accent_json: Option<String>,
}

/// The columns of `TitleRow`, for queries that read titles with `t` as the
/// title's alias.
pub const TITLE_COLUMNS: &str = "t.id, t.kind, t.name, t.overview, t.poster_path, t.backdrop_path,
        t.logo_path, t.release_date, t.last_air_date, t.tmdb_status, t.runtime, t.episode_count,
        t.season_count, t.genres_json, t.tmdb_vote, t.collection_id, t.accent_json";

pub fn parse_accent(raw: Option<&str>) -> Palette {
    raw.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

pub fn parse_genres(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

pub async fn get_title(pool: &SqlitePool, id: i64) -> AppResult<TitleRow> {
    sqlx::query_as::<_, TitleRow>(&format!(
        "SELECT {TITLE_COLUMNS} FROM title t WHERE t.id = ?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("no such title".into()))
}

/// The title a provider id names, made from the provider's detail if the
/// catalog does not have it yet, and refreshed from it if it does. Gives
/// Frog's id and the detail, TMDB's or shaped like TMDB's, so callers can
/// draw cast, videos and providers from the same answer.
pub async fn ensure(state: &AppState, id: &ExternalId) -> AppResult<(i64, Value)> {
    match id {
        ExternalId::Tmdb { kind, id: tmdb_id } => ensure_tmdb(state, id, kind, *tmdb_id).await,
        ExternalId::AniList(media_id) => ensure_anilist(state, *media_id).await,
        ExternalId::Imdb(_) => Err(AppError::BadRequest(
            "Frog reads titles from TMDB or AniList, not IMDb.".into(),
        )),
    }
}

/// A title by Frog's id, with its detail from its primary provider. A title
/// whose provider no longer has it, or that has no provider, still opens:
/// with what Frog kept.
pub async fn load(state: &AppState, title_id: i64) -> AppResult<(TitleRow, Value)> {
    let row = get_title(&state.pool, title_id).await?;
    let Some(source) = primary(&state.pool, title_id).await? else {
        return Ok((row.clone(), kept_detail(&row)));
    };
    let (id, detail) = ensure(state, &source).await?;
    if id != title_id {
        // The provider id was linked to this title a moment ago; it cannot
        // name another one.
        return Err(AppError::Other(anyhow::anyhow!(
            "title {title_id}'s provider names title {id}"
        )));
    }
    Ok((get_title(&state.pool, title_id).await?, detail))
}

/// The detail of a title as Frog kept it, for when no provider answers.
fn kept_detail(row: &TitleRow) -> Value {
    json!({ "name": row.name, "overview": row.overview })
}

async fn ensure_tmdb(
    state: &AppState,
    id: &ExternalId,
    kind: &str,
    tmdb_id: i64,
) -> AppResult<(i64, Value)> {
    let fetched = match kind {
        "movie" => state.tmdb.movie(tmdb_id).await,
        "tv" => state.tmdb.tv(tmdb_id).await,
        other => return Err(AppError::BadRequest(format!("unknown kind {other}"))),
    };
    let detail = match fetched {
        Ok(detail) => detail,
        Err(AppError::NotFound(m)) => {
            // Gone from TMDB: a title Frog has keeps opening with what it kept.
            let Some(title_id) = find_exact(&state.pool, id).await? else {
                return Err(AppError::NotFound(m));
            };
            mark_gone(&state.pool, id).await?;
            tracing::info!(title_id, tmdb = %id.key(), "TMDB no longer has this title");
            let row = get_title(&state.pool, title_id).await?;
            return Ok((title_id, kept_detail(&row)));
        }
        Err(e) => return Err(e),
    };
    let core = TitleCore::from_detail(kind, &detail)
        .ok_or_else(|| AppError::Tmdb("TMDB returned a title with no name".into()))?;
    let title_id = keep(state, id, &core, &detail).await?;
    Ok((title_id, detail))
}

/// A title TMDB does not have, from AniList, in TMDB's shape
/// (`anilist::media`): stored and returned like any other, episodes and all.
/// AniList out of reach, a title Frog has opens with what it kept.
async fn ensure_anilist(state: &AppState, media_id: i64) -> AppResult<(i64, Value)> {
    let pool = &state.pool;
    let id = ExternalId::AniList(media_id);
    let existing = find_exact(pool, &id).await?;
    let media = match crate::anilist::media::fetch(state, media_id).await {
        Ok(media) => media,
        Err(e) => {
            let Some(title_id) = existing else {
                return Err(e);
            };
            if matches!(e, AppError::NotFound(_)) {
                mark_gone(pool, &id).await?;
            }
            tracing::debug!(media_id, error = %e, "AniList detail unavailable; using what is kept");
            let row = get_title(pool, title_id).await?;
            return Ok((title_id, kept_detail(&row)));
        }
    };
    let kind = match existing {
        // A kept title keeps its kind.
        Some(title_id) => get_title(pool, title_id).await?.kind,
        None if media["format"].as_str() == Some("MOVIE") => "movie".to_string(),
        None => "tv".to_string(),
    };
    let detail = crate::anilist::media::as_tmdb(state, &kind, &media).await?;
    let core = TitleCore::from_detail(&kind, &detail)
        .ok_or_else(|| AppError::AniList("AniList returned a title with no name".into()))?;
    let title_id = keep(state, &id, &core, &detail).await?;
    if kind == "tv" {
        upsert_episodes(pool, title_id, &crate::anilist::media::episodes(&media)).await?;
    }
    Ok((title_id, detail))
}

/// Store a title's detail under the provider id it came from: a new title
/// with that provider as primary, or the title already linked to it brought
/// up to date. Then what comes next, the page tint and the credits Stats
/// ranks.
async fn keep(
    state: &AppState,
    id: &ExternalId,
    core: &TitleCore,
    detail: &Value,
) -> AppResult<i64> {
    let pool = &state.pool;
    let title_id = match find_exact(pool, id).await? {
        Some(title_id) => {
            update_title(pool, title_id, core).await?;
            let mut conn = pool.acquire().await?;
            link(&mut conn, title_id, id, false).await?;
            title_id
        }
        None => insert_title(pool, id, core).await?,
    };
    if let Some(imdb) = core.imdb_id.as_deref().filter(|s| s.starts_with("tt")) {
        let mut conn = pool.acquire().await?;
        if let Err(e) = link(&mut conn, title_id, &ExternalId::Imdb(imdb.into()), false).await {
            // TMDB has given one IMDb id to two entries before; the first keeps it.
            tracing::debug!(title_id, imdb, error = %e, "IMDb id not linked");
        }
    }

    // What the provider says comes next: a show between two episodes of one
    // season is airing, one waiting for an episode 1 is between seasons.
    let next = &detail["next_episode_to_air"];
    sqlx::query(
        "UPDATE title SET next_air_date = ?, next_air_season = ?, next_air_episode = ? WHERE id = ?",
    )
    .bind(next["air_date"].as_str().filter(|d| !d.is_empty()))
    .bind(next["season_number"].as_i64())
    .bind(next["episode_number"].as_i64())
    .bind(title_id)
    .execute(pool)
    .await?;

    ensure_palette(state, title_id, core.poster_path.as_deref()).await;
    // Stats are a nice-to-have on top of the title; never fail the page for them.
    if let Err(e) = store_credits(pool, title_id, detail).await {
        tracing::warn!(title_id, error = %e, "could not store credits");
    }
    Ok(title_id)
}

/// A new title with `id` as its primary provider. Two requests making the
/// same title at once both insert; the one whose link lands keeps its row
/// and the other takes it back, so the catalog never holds two.
async fn insert_title(pool: &SqlitePool, id: &ExternalId, core: &TitleCore) -> AppResult<i64> {
    let mut tx = pool.begin().await?;
    let title_id = sqlx::query(
        "INSERT INTO title (kind, name, original_name, overview, poster_path, backdrop_path,
                            logo_path, release_date, last_air_date, tmdb_status, runtime,
                            episode_count, season_count, genres_json, tmdb_vote, collection_id,
                            refreshed_at, search_name)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&core.kind)
    .bind(&core.name)
    .bind(&core.original_name)
    .bind(&core.overview)
    .bind(&core.poster_path)
    .bind(&core.backdrop_path)
    .bind(&core.logo_path)
    .bind(&core.release_date)
    .bind(&core.last_air_date)
    .bind(&core.tmdb_status)
    .bind(core.runtime)
    .bind(core.episode_count)
    .bind(core.season_count)
    .bind(&core.genres_json)
    .bind(core.tmdb_vote)
    .bind(core.collection_id)
    .bind(Utc::now().to_rfc3339())
    .bind(text::search_name(&core.name, core.original_name.as_deref()))
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    let linked = sqlx::query(
        "INSERT INTO title_external (title_id, provider, external_id, is_primary, added_at)
         VALUES (?, ?, ?, 1, ?)
         ON CONFLICT (provider, external_id) DO NOTHING",
    )
    .bind(title_id)
    .bind(id.provider())
    .bind(id.key())
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if linked == 0 {
        drop(tx);
        return find_exact(pool, id)
            .await?
            .ok_or_else(|| AppError::Other(anyhow::anyhow!("title vanished while being made")));
    }
    tx.commit().await?;
    Ok(title_id)
}

async fn update_title(pool: &SqlitePool, title_id: i64, core: &TitleCore) -> AppResult<()> {
    sqlx::query(
        "UPDATE title SET
            name = ?, original_name = ?, search_name = ?, overview = ?, poster_path = ?,
            backdrop_path = ?, logo_path = COALESCE(?, logo_path), release_date = ?,
            last_air_date = ?, tmdb_status = ?, runtime = ?, episode_count = ?,
            season_count = ?, genres_json = ?, tmdb_vote = ?, collection_id = ?,
            refreshed_at = ?
          WHERE id = ?",
    )
    .bind(&core.name)
    .bind(&core.original_name)
    .bind(text::search_name(&core.name, core.original_name.as_deref()))
    .bind(&core.overview)
    .bind(&core.poster_path)
    .bind(&core.backdrop_path)
    .bind(&core.logo_path)
    .bind(&core.release_date)
    .bind(&core.last_air_date)
    .bind(&core.tmdb_status)
    .bind(core.runtime)
    .bind(core.episode_count)
    .bind(core.season_count)
    .bind(&core.genres_json)
    .bind(core.tmdb_vote)
    .bind(core.collection_id)
    .bind(Utc::now().to_rfc3339())
    .bind(title_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// What AniList says about an entry TMDB does not have.
pub struct LocalTitle<'a> {
    pub anilist_id: i64,
    pub kind: &'a str,
    pub name: &'a str,
    pub original_name: Option<&'a str>,
    pub release_date: Option<&'a str>,
    pub episodes: Option<i64>,
    /// AniList's release state, in the words TMDB uses for its own.
    pub status: Option<&'a str>,
}

/// The title an AniList entry is kept as while TMDB has nothing for it: one
/// per AniList id, shared like any title, with AniList as its provider.
/// Refreshed from the sync's own answer each time (no detail is read), and
/// brought back if it had been folded into a TMDB title before.
pub async fn ensure_local_title(pool: &SqlitePool, t: &LocalTitle<'_>) -> AppResult<i64> {
    let id = ExternalId::AniList(t.anilist_id);
    if let Some(title_id) = find_exact(pool, &id).await? {
        let (original,): (Option<String>,) =
            sqlx::query_as("SELECT original_name FROM title WHERE id = ?")
                .bind(title_id)
                .fetch_one(pool)
                .await?;
        sqlx::query(
            "UPDATE title SET name = ?, search_name = ?, release_date = COALESCE(?, release_date),
                    episode_count = ?, tmdb_status = ?, merged_into = NULL
              WHERE id = ?",
        )
        .bind(t.name)
        .bind(text::search_name(
            t.name,
            original.as_deref().or(t.original_name),
        ))
        .bind(t.release_date)
        .bind(t.episodes)
        .bind(t.status)
        .bind(title_id)
        .execute(pool)
        .await?;
        return Ok(title_id);
    }
    let core = TitleCore {
        kind: t.kind.to_string(),
        name: t.name.to_string(),
        original_name: t.original_name.map(str::to_string),
        release_date: t.release_date.map(str::to_string),
        episode_count: t.episodes,
        season_count: (t.kind == "tv").then_some(1),
        tmdb_status: t.status.map(str::to_string),
        ..TitleCore::default()
    };
    let title_id = insert_title(pool, &id, &core).await?;
    // Not read from AniList yet (`ensure` does that when it is opened), and
    // its credits come with that detail rather than the TMDB backfill.
    sqlx::query("UPDATE title SET refreshed_at = NULL, people_synced_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(title_id)
        .execute(pool)
        .await?;
    Ok(title_id)
}

/// Replace the credits stats rank by. Marks the title as synced even when the
/// provider lists nobody, so the worker does not keep asking.
pub async fn store_credits(pool: &SqlitePool, title_id: i64, detail: &Value) -> AppResult<()> {
    let credits = models::stat_credits(detail);
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM title_person WHERE title_id = ?")
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    for c in &credits {
        sqlx::query(
            "INSERT INTO title_person (title_id, role, tmdb_id, name, profile_path, ord)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(title_id)
        .bind(c.role)
        .bind(c.tmdb_id)
        .bind(&c.name)
        .bind(&c.profile_path)
        .bind(c.ord)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE title SET people_synced_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Derive the page tint from the poster once, then keep it. A tiny w92 thumb is
/// enough and costs about 5 KB.
async fn ensure_palette(state: &AppState, title_id: i64, poster_path: Option<&str>) {
    let Some(poster) = poster_path else { return };

    let existing: Option<(Option<String>,)> =
        sqlx::query_as("SELECT accent_json FROM title WHERE id = ?")
            .bind(title_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if matches!(existing, Some((Some(_),))) {
        return;
    }

    let bytes = match crate::images::cached_image(state, "w92", poster).await {
        Ok((_, b)) => b,
        Err(e) => {
            tracing::debug!(error = %e, "palette: could not load poster");
            return;
        }
    };

    if let Some(p) = palette::extract(&bytes) {
        let json = serde_json::to_string(&p).unwrap_or_default();
        if let Err(e) = sqlx::query("UPDATE title SET accent_json = ? WHERE id = ?")
            .bind(json)
            .bind(title_id)
            .execute(&state.pool)
            .await
        {
            tracing::debug!(title_id, error = %e, "palette not stored");
        }
    }
}

pub async fn upsert_episodes(
    pool: &SqlitePool,
    title_id: i64,
    episodes: &[EpisodeInfo],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for e in episodes {
        sqlx::query(
            "INSERT INTO episode (title_id, season_number, episode_number, tmdb_episode_id,
                                  name, overview, still_path, air_date, runtime, tmdb_vote)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (title_id, season_number, episode_number) DO UPDATE SET
                tmdb_episode_id = excluded.tmdb_episode_id,
                name = excluded.name,
                overview = excluded.overview,
                still_path = excluded.still_path,
                air_date = excluded.air_date,
                runtime = excluded.runtime,
                tmdb_vote = excluded.tmdb_vote",
        )
        .bind(title_id)
        .bind(e.season_number)
        .bind(e.episode_number)
        .bind(e.tmdb_episode_id)
        .bind(&e.name)
        .bind(&e.overview)
        .bind(&e.still_path)
        .bind(&e.air_date)
        .bind(e.runtime)
        .bind(e.tmdb_vote)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Make sure a show's episodes are listed, from its provider, before
/// anything ticks one. `detail` saves reading it again when the caller has it.
pub async fn ensure_episodes(
    state: &AppState,
    title_id: i64,
    detail: Option<&Value>,
) -> AppResult<()> {
    let (have,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM episode WHERE title_id = ?")
        .bind(title_id)
        .fetch_one(&state.pool)
        .await?;
    if have > 0 {
        return Ok(());
    }
    match detail {
        Some(detail) => sync_all_seasons(state, title_id, detail).await,
        None => {
            let (_, detail) = load(state, title_id).await?;
            sync_all_seasons(state, title_id, &detail).await
        }
    }
}

/// Pull every season of a show into the local episode table. Used when a show
/// is added, so progress and "up next" work without opening each season. A
/// title from AniList has its episodes from AniList, with its detail.
pub async fn sync_all_seasons(state: &AppState, title_id: i64, detail: &Value) -> AppResult<()> {
    let Some(ExternalId::Tmdb { id: tmdb_id, .. }) = primary(&state.pool, title_id).await? else {
        return Ok(());
    };
    for season in models::seasons(detail) {
        let payload = match state.tmdb.season(tmdb_id, season.season_number).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(season = season.season_number, error = %e, "season sync skipped");
                continue;
            }
        };
        let episodes = models::episodes_of_season(&payload);
        upsert_episodes(&state.pool, title_id, &episodes).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_read_back_as_written() {
        for id in [
            ExternalId::tmdb("movie", 550),
            ExternalId::tmdb("tv", 1399),
            ExternalId::AniList(21),
            ExternalId::Imdb("tt0113277".into()),
        ] {
            assert_eq!(
                ExternalId::parse(id.provider(), &id.key()),
                Some(id.clone())
            );
        }
        assert_eq!(ExternalId::tmdb("tv", 1399).key(), "tv/1399");
    }

    #[test]
    fn malformed_provider_ids_are_refused() {
        for (provider, key) in [
            ("tmdb", "1399"),
            ("tmdb", "book/1"),
            ("tmdb", "tv/-21"),
            ("anilist", "-21"),
            ("anilist", "x"),
            ("imdb", "0113277"),
            ("tvdb", "81189"),
        ] {
            assert_eq!(ExternalId::parse(provider, key), None, "{provider} {key}");
        }
    }

    #[test]
    fn accent_falls_back_to_the_default_theme_colour() {
        assert_eq!(parse_accent(None), Palette::default());
        assert_eq!(parse_accent(Some("not json")), Palette::default());
        let custom = r##"{"accent":"#ff0000","shade":"#100000","tint":"#ffcccc"}"##;
        assert_eq!(parse_accent(Some(custom)).accent, "#ff0000");
    }

    #[test]
    fn genres_survive_a_malformed_column() {
        assert_eq!(
            parse_genres("[\"Drama\",\"Crime\"]"),
            vec!["Drama", "Crime"]
        );
        assert!(parse_genres("").is_empty());
    }

    #[test]
    fn a_titles_own_ids_come_before_those_merged_into_it() {
        let mut x = Externals::default();
        x.add(ExternalId::tmdb("tv", 1), true);
        x.add(ExternalId::AniList(5), false);
        x.add(ExternalId::tmdb("tv", 2), false);
        x.add(ExternalId::AniList(5), false);
        assert_eq!(x.primary, Some("tmdb"));
        assert_eq!(x.tmdb.map(|t| t.id), Some(1));
        assert_eq!(x.anilist, vec![5]);
    }
}
