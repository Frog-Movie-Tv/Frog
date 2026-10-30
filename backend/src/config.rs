use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;

/// Everything the process needs from the environment, read once at startup.
#[derive(Clone, Debug)]
pub struct Config {
    pub tmdb_api_key: String,
    pub database_url: String,
    pub bind: SocketAddr,
    pub static_dir: PathBuf,
    pub region: String,
    pub language: String,
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
    pub secure_cookies: bool,
    pub session_days: i64,
    /// Local hour (0-23) from which new episodes are announced on their air date.
    pub notify_hour: u32,
    /// AniList API client. All three are needed, or AniList stays hidden.
    pub anilist: Option<AniListClient>,
    /// MDBList, for IMDb, Rotten Tomatoes and Metacritic scores. Without a
    /// key those simply do not appear.
    pub mdblist: Option<MdbListClient>,
    /// fanart.tv, for title logos TMDB has none of. Without a key titles
    /// simply keep the logo TMDB gives, or none.
    pub fanart: Option<FanartClient>,
}

/// A free personal key from fanart.tv. Sent as a header, so it is in no
/// address and no log.
#[derive(Clone)]
pub struct FanartClient {
    pub key: String,
    /// Only tests point these elsewhere.
    pub api_url: String,
    pub assets_url: String,
}

/// Never the key: a config dump must not carry it.
impl std::fmt::Debug for FanartClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FanartClient")
            .field("api_url", &self.api_url)
            .finish_non_exhaustive()
    }
}

/// A free key from mdblist.com/preferences. It stays on the server: only
/// `ratings` reads it, and no answer or log carries it.
#[derive(Clone)]
pub struct MdbListClient {
    pub key: String,
    /// Only tests point this elsewhere.
    pub api_url: String,
    pub budget: Arc<Budget>,
}

/// Requests left today, as MDBList last said, and when that resets. Kept in
/// memory: a restart forgets it, and the next answer says it again.
#[derive(Debug)]
pub struct Budget {
    pub remaining: AtomicI64,
    pub resets_at: AtomicI64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            remaining: AtomicI64::new(i64::MAX),
            resets_at: AtomicI64::new(0),
        }
    }
}

impl std::fmt::Debug for MdbListClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdbListClient")
            .field("api_url", &self.api_url)
            .finish_non_exhaustive()
    }
}

/// An application registered at anilist.co/settings/developer. Its redirect
/// URL is fixed there, so one client serves exactly one public address.
#[derive(Clone, Debug)]
pub struct AniListClient {
    pub id: String,
    pub secret: String,
    pub redirect_uri: String,
    /// GraphQL endpoint and sign-in site. Only tests point these elsewhere.
    pub api_url: String,
    pub site_url: String,
    /// The Anime-IDs list that says which TVDB/IMDb entry each AniList id is.
    pub anime_ids_url: String,
    /// Fribb's anime-lists, which says which TMDB entry (and TMDB season)
    /// each AniList id is.
    pub anime_lists_url: String,
}

fn env_opt(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

fn env_or(key: &str, default: &str) -> String {
    env_opt(key).unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind: SocketAddr = env_or("FROG_BIND", "0.0.0.0:8080")
            .parse()
            .map_err(|e| anyhow::anyhow!("FROG_BIND is not a valid socket address: {e}"))?;

        Ok(Self {
            // Missing key is not fatal: the app still boots so you can reach the
            // settings screen and see a clear message instead of a dead container.
            tmdb_api_key: env_or("TMDB_API_KEY", ""),
            database_url: env_or("DATABASE_URL", "sqlite:./data/frog.db?mode=rwc"),
            bind,
            static_dir: PathBuf::from(env_or("FROG_STATIC_DIR", "./static")),
            region: env_or("FROG_REGION", "BE"),
            language: env_or("FROG_LANGUAGE", "en-US"),
            admin_username: env_opt("FROG_ADMIN_USERNAME"),
            admin_password: env_opt("FROG_ADMIN_PASSWORD"),
            secure_cookies: matches!(env_or("FROG_SECURE_COOKIES", "0").as_str(), "1" | "true"),
            session_days: env_or("FROG_SESSION_DAYS", "90").parse().unwrap_or(90),
            notify_hour: env_or("FROG_NOTIFY_HOUR", "18")
                .parse::<u32>()
                .ok()
                .filter(|h| *h < 24)
                .unwrap_or(18),
            anilist: match (
                env_opt("ANILIST_CLIENT_ID"),
                env_opt("ANILIST_CLIENT_SECRET"),
                env_opt("ANILIST_REDIRECT_URI"),
            ) {
                (Some(id), Some(secret), Some(redirect_uri)) => Some(AniListClient {
                    id,
                    secret,
                    redirect_uri,
                    api_url: env_or("ANILIST_API_URL", "https://graphql.anilist.co"),
                    site_url: env_or("ANILIST_SITE_URL", "https://anilist.co"),
                    anime_ids_url: env_or(
                        "ANIME_IDS_URL",
                        "https://raw.githubusercontent.com/Kometa-Team/Anime-IDs/master/anime_ids.json",
                    ),
                    anime_lists_url: env_or(
                        "ANIME_LISTS_URL",
                        "https://raw.githubusercontent.com/Fribb/anime-lists/master/anime-list-full.json",
                    ),
                }),
                _ => None,
            },
            mdblist: env_opt("MDBLIST_API_KEY").map(|key| MdbListClient {
                key,
                api_url: env_or("MDBLIST_API_URL", "https://api.mdblist.com"),
                budget: Arc::default(),
            }),
            fanart: env_opt("FANART_API_KEY").map(|key| FanartClient {
                key,
                api_url: env_or("FANART_API_URL", "https://webservice.fanart.tv/v3"),
                assets_url: env_or("FANART_ASSETS_URL", "https://assets.fanart.tv"),
            }),
        })
    }

    pub fn has_tmdb_key(&self) -> bool {
        !self.tmdb_api_key.is_empty()
    }
}
