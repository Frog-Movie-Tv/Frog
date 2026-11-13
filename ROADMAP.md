# Roadmap

What is still to build, and what is known to be broken. Nothing else lives
here: how things work is in [CLAUDE.md](CLAUDE.md), and what shipped is in
[RELEASE-NOTES.md](RELEASE-NOTES.md). Take an item off when it ships.

**Release:** 1.5.0 Goliath. **Next codename:** Tomato Frog. **Live at** a DuckDNS hostname on Oracle Cloud Always
Free, updating itself from each release (`docs/DEPLOY-ORACLE.md`).

## To build

Left from the badges and profiles batch (branch `feat/badges-and-profiles`):

- **Ratings:** test every rating control by mouse, touch and keyboard in each
  score format; clearing by tapping the current score, the popover's place on
  a phone, quick taps saving out of order, a window listener per episode row,
  and the `★` glyph. A Playwright test for each fix.
- **The title page's picture grows and shrinks** when a status or rating is
  saved. Reproduce in the running app and fix.
- **Remove the colour editor:** `ThemeEditor.svelte`, `customTheme` and
  `savedThemes`; old settings must load cleanly.
- **Finish:** seed a demo account with a colour, a vertical banner and a
  badge; run the full backend suite, `npm run build`, a release build,
  `npx playwright test` and `npm run audit`; look at both layouts on a phone.

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

|                                             |                                                                                                                                              |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| **Rebase merging doubles the changelog**    | "Rebase and merge" rewrites commit hashes, so release-please lists a change twice. Untick _Allow rebase merging_ in the repository settings. |
| **The backup repository grows without end** | About 3 MB a copy, one nightly plus one per update. `docs/DEPLOY-ORACLE.md` step 7 has the reset; nothing does it on a schedule.             |
| **The database file does not shrink**       | SQLite reuses freed pages but never returns them. A `VACUUM` with the instance stopped gives the space back.                                 |
| **Screenshots weigh 13 MB**                 | No lossless optimiser runs in the release step.                                                                                              |
| **One AniList test is flaky**               | `a_tmdb_outage_is_retried_not_left_for_the_person` races other tests over account 1's sync flag; it passes on a rerun.                       |
| **The Projects board does not exist**       | Needs `gh auth refresh -s project` from an owner before it can be made.                                                                      |
