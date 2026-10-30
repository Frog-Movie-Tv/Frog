//! AniList, read only.
//!
//! Each account connects its own AniList through AniList's sign-in; the token
//! stays on the server. A sync reads the whole AniList list, places every
//! entry in Frog by id (never by name), and then, entry by entry, brings in
//! what changed there since the last sync. Nothing ever goes the other way:
//! Frog has no write to AniList and `client::Api::graphql` refuses one, so
//! what someone does in Frog stays in Frog.
//!
//! Placing is the hard part. AniList has one entry per season or cour; TMDB
//! often has one long season instead (Jujutsu Kaisen's second season is TMDB
//! season 1 from episode 25). The community Anime-IDs list says which TVDB
//! season and offset each AniList entry is; from that and the episode counts
//! of the seasons before it, the entry's absolute position is known, and TMDB's
//! own season lengths turn that into a TMDB season and episode. Anything that
//! cannot be placed for certain is left for the person to choose.
//!
//! | Module      | What it does                                              |
//! | ----------- | --------------------------------------------------------- |
//! | `client`    | Sign-in, GraphQL calls, AniList entries as values          |
//! | `idmap`     | The community id lists: AniList id → TMDB, TVDB, IMDb      |
//! | `place`     | Where an entry belongs on TMDB, and how sure that is       |
//! | `reconcile` | Who wins, entry by entry; pure, no I/O                     |
//! | `library`   | Frog's side of an entry: reading it, writing AniList's in  |
//! | `local`     | Entries TMDB lacks, kept as AniList-only titles            |
//! | `media`     | An AniList-only title's details, in TMDB's shape           |
//! | `records`   | `anilist_link`, `anilist_oauth`, `anilist_entry`           |
//! | `sync`      | One sync of one account, and when the worker runs one      |
//!
//! `docs/ANILIST.md` has the whole story.

mod client;
mod idmap;
mod library;
mod local;
pub mod media;
mod place;
mod reconcile;
pub mod records;
mod sync;

pub use client::{public_list, Api, Remote, View};
pub use library::{forget_place, set_part_rating};
pub use place::{
    clean, locate, main_run, place_tv, plain, Outcome, Placement, Target, CONFIDENT, RESOLVER,
};
pub use sync::{diagnose, due, is_syncing, sync_minutes, sync_user, Summary};

use sqlx::SqlitePool;

use crate::state::AppState;

/// One account's sync: whose it is, and the AniList it talks to with which
/// token.
pub(crate) struct Account<'a> {
    pub state: &'a AppState,
    pub api: &'a Api<'a>,
    pub token: &'a str,
    pub user_id: i64,
}

/// Have the next worker pass read this account's list, rather than waiting
/// for the timer. Best effort, since the timer comes round anyway.
pub async fn touch(pool: &SqlitePool, user_id: i64) {
    if let Err(e) = sqlx::query("UPDATE anilist_link SET dirty = 1 WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await
    {
        tracing::debug!(user_id, error = %e, "could not mark the AniList list for a sync");
    }
}
