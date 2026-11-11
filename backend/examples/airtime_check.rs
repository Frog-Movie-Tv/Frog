//! DEVELOPMENT ONLY. The moment one of TMDB's episodes airs, as Frog would
//! work it out, against real TMDB, AniList and TVmaze.
//!
//!   cargo run --example airtime_check -- <TMDB show id> <season> <episode> [zone] [scratch.db]
//!
//! Reads TMDB_API_KEY from .env. The scratch database keeps the anime map and
//! the TMDB cache between runs; never point it at a real one.
use chrono::NaiveDate;
use chrono_tz::Tz;
use frog::{airtime, anilist, config::Config, db, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: airtime_check <TMDB show id> <season> <episode> [zone] [scratch.db]";
    let tmdb_id: i64 = args.first().expect(usage).parse()?;
    let season: i64 = args.get(1).expect(usage).parse()?;
    let episode: i64 = args.get(2).expect(usage).parse()?;
    let zone: Tz = args
        .get(3)
        .map_or("Europe/Brussels", String::as_str)
        .parse()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let path = args
        .get(4)
        .cloned()
        .unwrap_or_else(|| "airtime-check.db".into());

    let mut config = Config::from_env()?;
    config.database_url = format!("sqlite:{}?mode=rwc", path.replace('\\', "/"));
    let pool = db::connect(&config.database_url).await?;
    let state = AppState::new(pool, config);
    let api = anilist::Api::public(&state);
    anilist::ensure_map(&state, api.client).await?;

    // TMDB's own day for the episode.
    let raw = state.tmdb.season(tmdb_id, season).await?;
    let day = raw["episodes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["episode_number"].as_i64() == Some(episode))
        .and_then(|e| e["air_date"].as_str())
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());

    let at = airtime::air_timestamp(&state, tmdb_id, season, episode, day).await?;
    println!(
        "tv/{tmdb_id} S{season:02}E{episode:02}  TMDB day {}  →  {}",
        day.map_or("none".into(), |d| d.to_string()),
        match at {
            Some(at) => format!(
                "{}  ({} {})",
                airtime::stamp(at),
                at.with_timezone(&zone).format("%a %-d %b %Y %H:%M"),
                zone.name()
            ),
            None => "no time known (time TBA)".into(),
        }
    );
    Ok(())
}
