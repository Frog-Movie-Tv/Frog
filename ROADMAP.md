# Roadmap

What is still to build, and what is known to be broken. Nothing else lives
here: how things work is in [CLAUDE.md](CLAUDE.md), and what shipped is in
[RELEASE-NOTES.md](RELEASE-NOTES.md). Take an item off when it ships.

**Release:** 1.9.0 Reed Frog. **Next:** 1.9.1. **Live at** a DuckDNS hostname on Oracle Cloud Always
Free, updating itself from each release (`docs/DEPLOY-ORACLE.md`).

## On the server

Owner steps; nothing in a release does them.

- **Give the space back to pictures.** Once the release that packs TMDB's
  cached answers has run a day, stop the instance, `VACUUM`, and raise
  `FROG_IMAGE_CACHE_MB` from 128 to 384 (`docs/DEPLOY-ORACLE.md`). Today the
  picture cache turns over within a day and 14% of `/img` requests take over
  250 ms. A `VACUUM` before then gives back only about 48 MB.

## To build

- **Ship Events as 1.10.0 Golden Frog before 1 December**, so Doomsday's
  countdown is on Home with two weeks to go (it shows from 1 November and
  opens 16 December here). Its pull request is open from `feat/events`;
  once it merges and the release goes out, delete `PROMPT-EVENTS.md`.
- **Check the premiere in Safari's engine and on a real phone.**
  `node scripts/verify-premiere.mjs --webkit` needs
  `npx playwright install webkit` first. Chrome held 17.1 ms at the 95th
  percentile on desktop and on an emulated Pixel 7; a real Pixel 7's figure
  is still to measure.
- **An event without a catch-up list has never been drawn.** Its page
  (About, trailer, cast, "On everyone's watchlist") and its hero's Add to
  watchlist only show with a pack that has `"catch_up": false`; make one
  with `/event` and look at it before relying on it.

- **Estimated days for undated history**, so the diary, streaks and badges
  see all of it. Needs a decision first: badges about a moment count only
  watches with a real date recorded as they happened, on purpose, so an
  estimate could only ever reach the diary and Stats, marked as estimated.
  Where it comes from (between the dated watches either side, or the
  entry's start and finish) is also to be chosen.
- **Tune the badge eligibility numbers on the owners' real libraries** (36
  hours, half the runtime, 10 titles a day): run `awards::eligible` over a
  restored production backup and count what each rule turns away. Needs
  the private age key, which only the owners hold.
- **Upgrade sqlx to 0.9.** Its bundled SQLite carries RUSTSEC-2026-0459,
  ignored in `backend/.cargo/audit.toml` until then (RUSTSEC-2023-0071 is
  `rsa` through `sqlx-mysql`, which is never built).
- **Drop the retired columns** `episode_state.rating` and
  `airing_state.shows` in the release after 1.9.1: nothing reads or writes
  them now, and a rollback to 1.9.1 must still find them.
  `anime_map.tvdb_*` stays: AniList placement asks TMDB's `/find` with it.
- **Origins for AniList-only titles.** World Tour and Subtitles On read
  TMDB's countries and language, so a title only AniList has counts for
  neither; AniList's `countryOfOrigin` could fill the country.

## Known issues

|                                             |                                                                                                                                                                                                                               |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Rebase merging doubles the changelog**    | "Rebase and merge" rewrites commit hashes, so release-please lists a change twice. Untick _Allow rebase merging_ in the repository settings.                                                                                  |
| **The backup repository grows without end** | About 18 MB a copy (710 MB on 2026-11-16), one nightly plus one per update: past GitHub's 1 GB in weeks. Keep a fixed number of copies, or send them somewhere that is not git. `docs/DEPLOY-ORACLE.md` step 7 has the reset. |
| **The database file does not shrink**       | SQLite reuses freed pages but never returns them. A `VACUUM` with the instance stopped gives the space back.                                                                                                                  |
| **Screenshots weigh 49 MB**                 | No lossless optimiser runs in the release step; the repository's history is 246 MB.                                                                                                                                           |
| **One AniList test is flaky**               | `a_tmdb_outage_is_retried_not_left_for_the_person` races other tests over account 1's sync flag; it passes on a rerun.                                                                                                        |
| **The Projects board does not exist**       | Needs `gh auth refresh -s project` from an owner before it can be made.                                                                                                                                                       |
