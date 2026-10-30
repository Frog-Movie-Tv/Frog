//! Moving a library in and out of Frog.
//!
//! Parsing is deliberately the browser's job: a Letterboxd ZIP or an IMDb CSV
//! is read there (`frontend/src/lib/import/`) and arrives here as a
//! normalised list, which keeps the wizard responsive and this module free of
//! file-format plumbing. What the server owns is the hard part — deciding
//! which TMDB title each row means (`matching`), and writing the result
//! without trampling anything (`apply`). `export` writes the two backups.
//!
//! An import goes in two requests: match a batch of rows, let the person
//! settle what is in doubt, then apply the chosen titles. `docs/IMPORTS.md`
//! has the whole story.

mod apply;
mod export;
mod matching;

pub use apply::{apply_items, merged_status, Applied, ApplyItem, Conflict, EpisodeWatch};
pub use export::{library_csv, library_json};
pub use matching::{match_rows, Incoming, MatchResult, Placed, SeasonCount};

/// Matching and applying are capped per request, so one request cannot sit on
/// TMDB for ten minutes; the wizard sends a few hundred rows at a time and
/// shows progress.
pub const MAX_BATCH: usize = 200;
