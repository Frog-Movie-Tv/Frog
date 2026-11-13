# Roadmap

What is still to build, and what is known to be broken. Nothing else lives
here: how things work is in [CLAUDE.md](CLAUDE.md), and what shipped is in
[RELEASE-NOTES.md](RELEASE-NOTES.md). Take an item off when it ships.

**Release:** 1.6.0 Tomato Frog. **Next codename:** not chosen yet. **Live at** a DuckDNS hostname on Oracle Cloud Always
Free, updating itself from each release (`docs/DEPLOY-ORACLE.md`).

## To build

Nothing at the moment.

## To check by hand

Nothing automated can stand in for these. An owner tries each and takes it
off, or turns it into a fix above.

- **A notification at air time on the Android phone:** the picture, Frog's
  icon and badge, and tapping it opening the title.
- **Holding a card on the phone** opens the menu as a sheet, and the tap that
  ends the hold does not open the title. A sideways swipe along Settings'
  tabs scrolls without a reload.
- **Pip on iOS Safari**, and a picture uploaded from an iPhone or a Mac.
- **The badge editor by finger:** pinching the crop, magnifying while
  touching up, and two-finger moves on a real phone; and a badge given
  reaching the phone as a notification.
- **A vertical banner on a real phone**, and Full's ceiling on a very tall
  picture.
- **The colour picker's sheet and the Stats bars by finger** on the phone.
- **AniList on the live list:** Settings → Connections, "Needs a check" and
  the "Matched" methods, manga included. And connecting AniList once from a
  laptop with the Frog-Dev client in `.env`.
- **The browser extension on real sites:** Load unpacked in Brave, the signed
  `.xpi` in Zen installed from Frog's own Firefox button in one click, and Zen
  picking up the next release by itself, a token from Settings → Connections, then an episode on a
  site with a cross-origin player and one with nothing but a title, through
  to the diary. And the first release's signing, once `AMO_JWT_ISSUER` and
  `AMO_JWT_SECRET` are set in the repository (`docs/EXTENSION.md`).
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
