# Roadmap

What is still to build, and what is known to be broken. Nothing else lives
here: how things work is in [CLAUDE.md](CLAUDE.md), and what shipped is in
[RELEASE-NOTES.md](RELEASE-NOTES.md). Take an item off when it ships.

**Release:** 1.4.3. **Next codename:** Goliath. **Live at** a DuckDNS hostname on Oracle Cloud Always
Free, updating itself from each release (`docs/DEPLOY-ORACLE.md`).

## To build

1. **Undo a merge from the app.** Every move of one account's data between
   titles is in `title_merge` with what both sides held, but undoing one is
   SQL by hand. Decide first: a merge comes from an AniList placement, so an
   undo lasts only until the next sync while the placement stands.
2. **Series copies from AniList.** An AniList series the sync has not placed
   still gets a title of its own beside TMDB's. Place a series' first season
   the way films already are, once that can run without someone's token.
3. **Stats' people and streaks under a pick.** Picking a bar narrows every
   chart but actors, directors, studios, authors, days and streaks. Needs the
   credits and days per title in the facts the page filters.
4. **Netflix: the 405 undated episodes.** On the 4,054-line test file, 405
   episodes have names TMDB writes differently or not at all (Kuroko's
   Basketball, The Order) and are counted undated. Match them by a looser
   name, or by position within the season.
5. **Netflix "Part N" seasons named only "Episode 3".** Money Heist Part 1
   still waits for a person to place it.
6. **SQL out of the route files.** Move each route's queries into `store` or
   its domain module when that route is next touched: `account`,
   `two_factor` (beside `auth.rs`), `profiles`, `shelves`, `timeline`,
   `stats`, `titles`, `episodes`, `home`, `search`, `browse`, `admin`,
   `invites`, `auth`, `push`, `settings`.
7. **Redraw the last pre-Glass parts:** the Calendar's month grid and day
   panel, Stats' charts, a person's page, and the import review's rows.

## To check by hand

Nothing automated can stand in for these. An owner tries each and takes it
off, or turns it into a fix above.

- **A notification at air time on the Android phone:** the picture, Frog's
  icon and badge, and tapping it opening the title.
- **Holding a card on the phone** opens the menu as a sheet, and the tap that
  ends the hold does not open the title. A sideways swipe along Settings'
  tabs scrolls without a reload.
- **Pip on iOS Safari**, and a picture uploaded from an iPhone or a Mac.
- **The colour picker's sheet and the Stats bars by finger** on the phone.
- **AniList on the live list:** Settings → Connections, "Needs a check" and
  the "Matched" methods, manga included. And connecting AniList once from a
  laptop with the Frog-Dev client in `.env`.
- **Netflix's personal-data ZIP** (`CONTENT_INTERACTION/ViewingActivity.csv`),
  read from its documented columns but never from a real one; and the show
  threshold (3) on a second person's file.

## Known issues

| | |
| --- | --- |
| **Rebase merging doubles the changelog** | "Rebase and merge" rewrites commit hashes, so release-please lists a change twice. Untick *Allow rebase merging* in the repository settings. |
| **The backup repository grows without end** | About 3 MB a copy, one nightly plus one per update. `docs/DEPLOY-ORACLE.md` step 7 has the reset; nothing does it on a schedule. |
| **The database file does not shrink** | SQLite reuses freed pages but never returns them. A `VACUUM` with the instance stopped gives the space back. |
| **Screenshots weigh 13 MB** | No lossless optimiser runs in the release step. |
| **One AniList test is flaky** | `a_tmdb_outage_is_retried_not_left_for_the_person` races other tests over account 1's sync flag; it passes on a rerun. |
| **The Projects board does not exist** | Needs `gh auth refresh -s project` from an owner before it can be made. |
