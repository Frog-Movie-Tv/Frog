//! DEVELOPMENT ONLY. Where Frog would place every entry of a public AniList
//! list, and why, against real TMDB data, without touching any library.
//!
//!   cargo run --example anilist_check -- <AniList user> [scratch.db]
//!
//! Reads TMDB_API_KEY and the AniList client from .env. The scratch database
//! keeps the TMDB cache between runs; never point it at a real one.
use frog::{anilist, config::Config, db, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let mut args = std::env::args().skip(1);
    let user = args
        .next()
        .expect("usage: anilist_check <AniList user> [scratch.db]");
    let path = args.next().unwrap_or_else(|| "anilist-check.db".into());

    let mut config = Config::from_env()?;
    config.database_url = format!("sqlite:{}?mode=rwc", path.replace('\\', "/"));
    let pool = db::connect(&config.database_url).await?;
    let state = AppState::new(pool, config);

    let remotes = anilist::public_list(&state, &user).await?;
    let results = anilist::diagnose(&state, &remotes).await?;
    let mut tally = std::collections::BTreeMap::new();
    for r in &results {
        *tally
            .entry(r["outcome"].as_str().unwrap_or("?").to_string())
            .or_insert(0) += 1;
        println!("{r}");
    }
    eprintln!("{} entries: {tally:?}", results.len());
    Ok(())
}
