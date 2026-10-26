use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::config::Config;
use crate::tmdb::Tmdb;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub tmdb: Tmdb,
    pub config: Arc<Config>,
    /// Timestamps of recent failed sign-ins. In memory is the right place for
    /// this: one process, one user, and a restart clearing it is harmless.
    pub login_failures: Arc<Mutex<Vec<Instant>>>,
    /// For everything that is not TMDB: push services, for now.
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: Config) -> Self {
        let tmdb = Tmdb::new(
            pool.clone(),
            config.tmdb_api_key.clone(),
            config.language.clone(),
            config.region.clone(),
        );
        Self {
            pool,
            tmdb,
            config: Arc::new(config),
            login_failures: Arc::new(Mutex::new(Vec::new())),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent(concat!("Frog/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
        }
    }
}
