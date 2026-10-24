use sqlx::SqlitePool;
use std::sync::Arc;

use crate::config::Config;
use crate::tmdb::Tmdb;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub tmdb: Tmdb,
    pub config: Arc<Config>,
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
        }
    }
}
