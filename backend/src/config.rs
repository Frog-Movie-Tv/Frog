use std::net::SocketAddr;
use std::path::PathBuf;

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
                }),
                _ => None,
            },
        })
    }

    pub fn has_tmdb_key(&self) -> bool {
        !self.tmdb_api_key.is_empty()
    }
}
