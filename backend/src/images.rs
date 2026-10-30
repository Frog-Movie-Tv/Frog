//! The picture cache: posters, backdrops, stills and AniList's covers, kept in
//! SQLite (`image_cache`) so a second viewing never leaves the server.
//!
//! Each picture is keyed by size and path (`w342/abc.jpg`, `anilist/media/…`,
//! `crunchyroll/…`) and only fetched from a fixed list of hosts and sizes, so
//! the cache cannot become an open relay.
//!
//! It is held to a budget. The worker drops pictures nobody has looked at
//! for `MAX_IDLE_DAYS`, then, if the cache is still over `budget_bytes()`
//! (`FROG_IMAGE_CACHE_MB`, 512 by default), the least recently served until
//! it is back under nine tenths of it. A dropped picture is fetched again the
//! next time a page needs it. Nothing else lives in the table, so nothing
//! else can go. SQLite reuses the freed pages for new pictures; the file
//! itself does not shrink.

use axum::http::header;
use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Sizes we are willing to proxy. A whitelist keeps the cache table bounded and
/// stops the endpoint becoming an open image relay.
const SIZES: [&str; 9] = [
    "w92", "w154", "w185", "w300", "w342", "w500", "w780", "w1280", "original",
];

/// A picture nobody has looked at for this long goes, whatever the budget.
pub const MAX_IDLE_DAYS: i64 = 180;

/// Serving a picture marks it at most this often, so reading a page is not a
/// write per poster.
const TOUCH_HOURS: i64 = 24;

/// How large the cache may grow, from `FROG_IMAGE_CACHE_MB` (64 at least).
pub fn budget_bytes() -> i64 {
    static BUDGET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(|| {
        let mb = std::env::var("FROG_IMAGE_CACHE_MB")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(512)
            .max(64);
        mb * 1024 * 1024
    })
}

/// Fetch through our own origin, with a SQLite-backed cache.
///
/// Going through the backend instead of hotlinking image.tmdb.org buys three
/// things: the service worker can cache posters for offline, the canvas can
/// read pixels without CORS, and a second viewing never hits TMDB at all.
pub async fn cached_image(
    state: &AppState,
    size: &str,
    path: &str,
) -> AppResult<(String, Vec<u8>)> {
    if size == "anilist" {
        let path = format!("/{}", path.trim_start_matches('/'));
        return anilist_image(state, &path).await;
    }
    if !SIZES.contains(&size) {
        return Err(AppError::BadRequest(format!(
            "unsupported image size {size}"
        )));
    }
    // Titles TMDB does not have carry AniList's pictures, under the same
    // sizes as TMDB's so every page asks for them the same way.
    let trimmed = path.trim_start_matches('/');
    if let Some(rest) = trimmed.strip_prefix("anilist/") {
        return anilist_image(state, &format!("/{}", cover_size(rest, size))).await;
    }
    if let Some(rest) = trimmed.strip_prefix("crunchyroll/") {
        if rest.contains("..") {
            return Err(AppError::BadRequest("bad image path".into()));
        }
        return remote_image(
            state,
            &format!("crunchyroll/{rest}"),
            &format!("https://img1.ak.crunchyroll.com/{rest}"),
        )
        .await;
    }
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if path.contains("..") {
        return Err(AppError::BadRequest("bad image path".into()));
    }

    let key = format!("{size}{path}");
    if let Some(found) = hit(&state.pool, &key).await? {
        return Ok(found);
    }
    let (content_type, bytes) = state.tmdb.fetch_image(size, &path).await?;
    keep(&state.pool, &key, &content_type, &bytes).await;
    Ok((content_type, bytes))
}

/// AniList keeps a cover in three sizes, in three folders with the same
/// file name: the smallest that is sharp at the size asked for.
fn cover_size(path: &str, size: &str) -> String {
    let folder = match size {
        "w92" => "small",
        "w154" | "w185" => "medium",
        _ => return path.to_string(),
    };
    path.replacen("/cover/large/", &format!("/cover/{folder}/"), 1)
}

/// Pictures from AniList's CDN, kept the same way: covers, banners, people
/// and the icons of streaming sites. Only those folders, so this is no
/// general relay either.
async fn anilist_image(state: &AppState, path: &str) -> AppResult<(String, Vec<u8>)> {
    let allowed = ["/character/", "/staff/", "/media/", "/link/"]
        .iter()
        .any(|folder| path.starts_with(folder));
    if !allowed || path.contains("..") {
        return Err(AppError::BadRequest("bad image path".into()));
    }
    remote_image(
        state,
        &format!("anilist{path}"),
        &format!("https://s4.anilist.co/file/anilistcdn{path}"),
    )
    .await
}

/// One picture from a fixed host, cached under `key`.
async fn remote_image(state: &AppState, key: &str, url: &str) -> AppResult<(String, Vec<u8>)> {
    if let Some(found) = hit(&state.pool, key).await? {
        return Ok(found);
    }
    let resp = state
        .http
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::AniList(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(AppError::NotFound("no such picture".into()));
    }
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|t| t.starts_with("image/"))
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::AniList(e.to_string()))?
        .to_vec();
    keep(&state.pool, key, &content_type, &bytes).await;
    Ok((content_type, bytes))
}

/// A cached picture, marked as served (at most once a day per picture).
async fn hit(pool: &SqlitePool, key: &str) -> AppResult<Option<(String, Vec<u8>)>> {
    let found: Option<(String, Vec<u8>)> =
        sqlx::query_as("SELECT content_type, bytes FROM image_cache WHERE path = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    if found.is_some() {
        let now = Utc::now();
        if let Err(e) =
            sqlx::query("UPDATE image_cache SET served_at = ? WHERE path = ? AND served_at < ?")
                .bind(now.to_rfc3339())
                .bind(key)
                .bind((now - Duration::hours(TOUCH_HOURS)).to_rfc3339())
                .execute(pool)
                .await
        {
            tracing::debug!(key, error = %e, "picture not marked as served");
        }
    }
    Ok(found)
}

/// Keep a picture just fetched. A failure only means it is fetched again
/// next time, so it is logged, not raised.
async fn keep(pool: &SqlitePool, key: &str, content_type: &str, bytes: &[u8]) {
    let now = Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query(
        "INSERT INTO image_cache (path, content_type, bytes, fetched_at, served_at, size)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (path) DO UPDATE SET
            content_type = excluded.content_type,
            bytes = excluded.bytes,
            fetched_at = excluded.fetched_at,
            served_at = excluded.served_at,
            size = excluded.size",
    )
    .bind(key)
    .bind(content_type)
    .bind(bytes)
    .bind(&now)
    .bind(&now)
    .bind(bytes.len() as i64)
    .execute(pool)
    .await
    {
        tracing::warn!(key, error = %e, "picture not cached");
    }
}

// --- keeping it to its budget -----------------------------------------------------

/// One cached picture, as eviction sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Cached {
    pub key: String,
    pub size: i64,
    pub served_at: DateTime<Utc>,
}

/// Which pictures to drop, given every picture least recently served first:
/// those idle since before `idle_before`, then more of the oldest until what
/// is left fits in nine tenths of `budget` (so the next few fetches do not
/// tip it over at once). Pure, so the decision is tested on its own.
pub fn plan_eviction(
    oldest_first: &[Cached],
    budget: i64,
    idle_before: DateTime<Utc>,
) -> Vec<String> {
    let mut total: i64 = oldest_first.iter().map(|c| c.size).sum();
    let target = budget / 10 * 9;
    let over = total > budget;
    let mut drop = Vec::new();
    for c in oldest_first {
        let idle = c.served_at < idle_before;
        if !idle && (!over || total <= target) {
            break;
        }
        total -= c.size;
        drop.push(c.key.clone());
    }
    drop
}

/// What one eviction pass did.
#[derive(Debug, Default, PartialEq)]
pub struct Evicted {
    pub pictures: usize,
    pub bytes: i64,
}

/// Bring the cache within its budget. Safe to run at any time and as often
/// as wanted: it only deletes rows it has just read as old, in small
/// batches, each its own statement, so an interrupted pass leaves a smaller
/// cache and the next pass carries on. A picture fetched meanwhile is the
/// newest there is and never chosen.
pub async fn evict(pool: &SqlitePool, budget: i64, now: DateTime<Utc>) -> AppResult<Evicted> {
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT path, COALESCE(size, length(bytes)), COALESCE(served_at, fetched_at)
           FROM image_cache",
    )
    .fetch_all(pool)
    .await?;
    let mut cached: Vec<Cached> = rows
        .into_iter()
        .map(|(key, size, at)| Cached {
            key,
            size,
            // Unreadable dates sort as the oldest there is.
            served_at: DateTime::parse_from_rfc3339(&at)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or(DateTime::UNIX_EPOCH),
        })
        .collect();
    // Ordered once the dates are read, so one that cannot be read really is
    // the oldest, rather than wherever its text happens to sort.
    cached.sort_by(|a, b| {
        a.served_at
            .cmp(&b.served_at)
            .then_with(|| a.key.cmp(&b.key))
    });
    let plan = plan_eviction(&cached, budget, now - Duration::days(MAX_IDLE_DAYS));
    let mut out = Evicted::default();
    for batch in plan.chunks(200) {
        let marks = vec!["?"; batch.len()].join(",");
        let sql = format!("DELETE FROM image_cache WHERE path IN ({marks})");
        let mut q = sqlx::query(&sql);
        for key in batch {
            q = q.bind(key);
        }
        q.execute(pool).await?;
        out.pictures += batch.len();
        out.bytes += cached
            .iter()
            .filter(|c| batch.contains(&c.key))
            .map(|c| c.size)
            .sum::<i64>();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + Duration::days(day)
    }

    fn pic(key: &str, size: i64, day: i64) -> Cached {
        Cached {
            key: key.into(),
            size,
            served_at: at(day),
        }
    }

    #[test]
    fn under_budget_only_idle_pictures_go() {
        let cache = [pic("a", 10, 1), pic("b", 10, 50), pic("c", 10, 300)];
        assert_eq!(plan_eviction(&cache, 1_000, at(10)), vec!["a"]);
        assert!(plan_eviction(&cache, 1_000, at(0)).is_empty());
    }

    #[test]
    fn over_budget_the_least_recently_served_go_until_nine_tenths_fit() {
        let cache = [
            pic("a", 40, 100),
            pic("b", 40, 200),
            pic("c", 40, 300),
            pic("d", 40, 400),
        ];
        // 160 against 100: down to 90 or less means the two oldest go.
        assert_eq!(plan_eviction(&cache, 100, at(0)), vec!["a", "b"]);
        // Exactly at the budget is within it.
        assert!(plan_eviction(&cache, 160, at(0)).is_empty());
    }

    #[test]
    fn an_empty_cache_or_a_huge_budget_drops_nothing() {
        assert!(plan_eviction(&[], 100, at(1_000)).is_empty());
        let cache = [pic("a", 10, 500)];
        assert!(plan_eviction(&cache, i64::MAX, at(10)).is_empty());
    }

    #[test]
    fn one_picture_larger_than_the_budget_goes() {
        let cache = [pic("big", 500, 900)];
        assert_eq!(plan_eviction(&cache, 100, at(0)), vec!["big"]);
    }
}
