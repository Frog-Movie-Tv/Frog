pub mod models;

use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::error::{AppError, AppResult};

pub const IMAGE_BASE: &str = "https://image.tmdb.org/t/p";
const API_BASE: &str = "https://api.themoviedb.org/3";

/// How long a cached response stays fresh. Metadata for a finished film barely
/// changes; a show that is still airing, and the discovery lists, change often.
pub mod ttl {
    pub const SEARCH_MINUTES: i64 = 60;
    pub const MOVIE_HOURS: i64 = 24 * 7;
    pub const TV_HOURS: i64 = 12;
    pub const SEASON_HOURS: i64 = 12;
    pub const PERSON_HOURS: i64 = 24 * 7;
    pub const TRENDING_HOURS: i64 = 6;
}

#[derive(Clone)]
pub struct Tmdb {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    key: String,
    language: String,
    region: String,
    pool: SqlitePool,
}

impl Tmdb {
    pub fn new(pool: SqlitePool, key: String, language: String, region: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("frog/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("reqwest client builds with rustls");

        Self {
            inner: Arc::new(Inner {
                http,
                key,
                language,
                region,
                pool,
            }),
        }
    }

    pub fn region(&self) -> &str {
        &self.inner.region
    }

    pub fn is_configured(&self) -> bool {
        !self.inner.key.is_empty()
    }

    /// v4 read tokens are JWTs and go in the Authorization header; the older v3
    /// keys are a query parameter. Accepting both means whichever one TMDB
    /// shows you on the settings page works.
    fn is_bearer(&self) -> bool {
        self.inner.key.matches('.').count() == 2
    }

    fn cache_key(&self, path: &str, params: &[(&str, String)]) -> String {
        let mut key = format!("{path}?lang={}", self.inner.language);
        for (k, v) in params {
            key.push_str(&format!("&{k}={v}"));
        }
        key
    }

    async fn cached(&self, key: &str) -> Option<Value> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT body, expires_at FROM tmdb_cache WHERE cache_key = ?")
                .bind(key)
                .fetch_optional(&self.inner.pool)
                .await
                .ok()
                .flatten();

        let (body, expires_at) = row?;
        let fresh = chrono::DateTime::parse_from_rfc3339(&expires_at)
            .map(|e| e > Utc::now())
            .unwrap_or(false);
        if !fresh {
            return None;
        }
        serde_json::from_str(&body).ok()
    }

    /// Returns a stale cached body regardless of TTL. Used when TMDB is
    /// unreachable, so a flaky connection degrades to slightly old data instead
    /// of an error page.
    async fn stale(&self, key: &str) -> Option<Value> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT body FROM tmdb_cache WHERE cache_key = ?")
                .bind(key)
                .fetch_optional(&self.inner.pool)
                .await
                .ok()
                .flatten();
        serde_json::from_str(&row?.0).ok()
    }

    async fn store(&self, key: &str, body: &Value, ttl: Duration) {
        let now = Utc::now();
        let _ = sqlx::query(
            "INSERT INTO tmdb_cache (cache_key, body, fetched_at, expires_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (cache_key) DO UPDATE SET
                body = excluded.body,
                fetched_at = excluded.fetched_at,
                expires_at = excluded.expires_at",
        )
        .bind(key)
        .bind(body.to_string())
        .bind(now.to_rfc3339())
        .bind((now + ttl).to_rfc3339())
        .execute(&self.inner.pool)
        .await;
    }

    pub async fn get(
        &self,
        path: &str,
        params: &[(&str, String)],
        ttl: Duration,
    ) -> AppResult<Value> {
        let key = self.cache_key(path, params);
        if let Some(hit) = self.cached(&key).await {
            return Ok(hit);
        }
        if !self.is_configured() {
            // No key, but we may still have something cached from before.
            return match self.stale(&key).await {
                Some(v) => Ok(v),
                None => Err(AppError::TmdbUnconfigured),
            };
        }

        let mut url = format!("{API_BASE}{path}?language={}", self.inner.language);
        for (k, v) in params {
            url.push_str(&format!("&{}={}", k, urlencoding::encode(v)));
        }
        if !self.is_bearer() {
            url.push_str(&format!(
                "&api_key={}",
                urlencoding::encode(&self.inner.key)
            ));
        }

        let mut req = self.inner.http.get(&url);
        if self.is_bearer() {
            req = req.bearer_auth(&self.inner.key);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%path, error = %e, "TMDB unreachable, falling back to cache");
                return self
                    .stale(&key)
                    .await
                    .ok_or_else(|| AppError::Tmdb(e.to_string()));
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            let detail = detail.chars().take(300).collect::<String>();
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(AppError::NotFound(format!("TMDB has nothing at {path}")));
            }
            if let Some(v) = self.stale(&key).await {
                tracing::warn!(%path, %status, "TMDB error, serving stale cache");
                return Ok(v);
            }
            return Err(AppError::Tmdb(format!("{status}: {detail}")));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Tmdb(format!("bad JSON from TMDB: {e}")))?;

        self.store(&key, &body, ttl).await;
        Ok(body)
    }

    pub async fn search_multi(&self, query: &str, page: u32) -> AppResult<Value> {
        self.get(
            "/search/multi",
            &[
                ("query", query.to_string()),
                ("page", page.to_string()),
                ("include_adult", "false".into()),
            ],
            Duration::minutes(ttl::SEARCH_MINUTES),
        )
        .await
    }

    pub async fn movie(&self, id: i64) -> AppResult<Value> {
        self.get(
            &format!("/movie/{id}"),
            &[
                (
                    "append_to_response",
                    "credits,videos,images,watch/providers,recommendations,similar,external_ids,release_dates".into(),
                ),
                ("include_image_language", format!("{},null", self.lang2())),
            ],
            Duration::hours(ttl::MOVIE_HOURS),
        )
        .await
    }

    pub async fn tv(&self, id: i64) -> AppResult<Value> {
        self.get(
            &format!("/tv/{id}"),
            &[
                (
                    "append_to_response",
                    "credits,videos,images,watch/providers,recommendations,similar,external_ids"
                        .into(),
                ),
                ("include_image_language", format!("{},null", self.lang2())),
            ],
            Duration::hours(ttl::TV_HOURS),
        )
        .await
    }

    pub async fn season(&self, tv_id: i64, season: i64) -> AppResult<Value> {
        self.get(
            &format!("/tv/{tv_id}/season/{season}"),
            &[],
            Duration::hours(ttl::SEASON_HOURS),
        )
        .await
    }

    /// Look a title up by another database's id: `tvdb_id` or `imdb_id`.
    pub async fn find(&self, external_id: &str, source: &str) -> AppResult<Value> {
        self.get(
            &format!("/find/{external_id}"),
            &[("external_source", source.to_string())],
            Duration::hours(ttl::MOVIE_HOURS),
        )
        .await
    }

    pub async fn person(&self, id: i64) -> AppResult<Value> {
        self.get(
            &format!("/person/{id}"),
            &[("append_to_response", "combined_credits,images".into())],
            Duration::hours(ttl::PERSON_HOURS),
        )
        .await
    }

    pub async fn trending(&self) -> AppResult<Value> {
        self.get(
            "/trending/all/week",
            &[],
            Duration::hours(ttl::TRENDING_HOURS),
        )
        .await
    }

    fn lang2(&self) -> String {
        self.inner
            .language
            .split('-')
            .next()
            .unwrap_or("en")
            .to_string()
    }

    /// Fetch an image straight from the TMDB CDN. No API key involved.
    pub async fn fetch_image(&self, size: &str, path: &str) -> AppResult<(String, Vec<u8>)> {
        let url = format!("{IMAGE_BASE}/{size}{path}");
        let resp = self
            .inner
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Tmdb(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AppError::NotFound(format!("no image at {size}{path}")));
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Tmdb(e.to_string()))?
            .to_vec();

        Ok((content_type, bytes))
    }
}
