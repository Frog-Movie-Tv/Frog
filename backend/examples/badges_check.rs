//! DEVELOPMENT ONLY. Works out every account's automatic badges on a copy of
//! a database and says what each would get, how many of its watches may earn
//! a moment, and how long it took.
//!
//!   cargo run --release --example badges_check -- <copy.db>
//!
//! It writes awards and notifications into the file: only ever a copy.
use std::time::Instant;

use chrono::Utc;
use frog::{awards, config::Config, db, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: badges_check <copy.db>");
    let url = format!("sqlite:{}?mode=rw", path.replace('\\', "/"));
    let config = Config {
        tmdb_api_key: String::new(),
        database_url: url.clone(),
        bind: "127.0.0.1:0".parse()?,
        static_dir: std::env::temp_dir(),
        region: "BE".into(),
        language: "en-US".into(),
        admin_username: None,
        admin_password: None,
        secure_cookies: false,
        session_days: 7,
        anilist: None,
        fanart: None,
        tvmaze_url: "http://127.0.0.1:9".into(),
        extension_releases: "http://127.0.0.1:9".into(),
        mdblist: None,
    };
    let pool = db::connect(&url).await?;
    let state = AppState::new(pool, config);
    awards::give::seed(&state.pool).await?;
    let waiting = awards::give::waiting(&state.pool).await?;
    if !waiting.is_empty() {
        println!(
            "waiting for a name: {}",
            waiting
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let accounts: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, username FROM app_user WHERE disabled = 0 ORDER BY id")
            .fetch_all(&state.pool)
            .await?;
    for (id, name) in accounts {
        let started = Instant::now();
        let tz = frog::announce::time_zone(&state.pool, id).await;
        let facts = awards::facts::read(&state.pool, id, tz, Utc::now()).await?;
        let read = started.elapsed();
        let ok = awards::eligible::eligible(&facts.logged(), tz, Utc::now());
        let settled = awards::run(&state, id, true).await?;
        println!(
            "{name}: {} watches, {} titles, {} may earn a moment; read in {read:?}, all in {:?}",
            facts.watches.len(),
            facts.entries.len(),
            ok.len(),
            started.elapsed()
        );
        println!(
            "  given {}: {}",
            settled.given.len(),
            settled.given.join(", ")
        );
        // What the Badges tab waits for.
        let started = Instant::now();
        let progress = awards::progress(&state.pool, id).await?;
        println!(
            "  progress on {} badges in {:?}",
            progress.len(),
            started.elapsed()
        );
    }
    Ok(())
}
