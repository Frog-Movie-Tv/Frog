# Roadmap

What is still to build, and what is known to be broken. Nothing else lives
here: how things work is in [CLAUDE.md](CLAUDE.md), and what shipped is in
[RELEASE-NOTES.md](RELEASE-NOTES.md). Take an item off when it ships.

**Release:** 1.8.0 Chorus Frog. **Next codename:** not chosen yet. **Live at** a DuckDNS hostname on Oracle Cloud Always
Free, updating itself from each release (`docs/DEPLOY-ORACLE.md`).

## To build

Left over from 1.7.0 Chorus Frog, each kept small on purpose so Frog did not
get busier:

- **Read "watched with" back from a Frog backup.** The JSON export writes
  `watched_with` (username per watch); an import ignores it. Bring it back
  only for usernames that exist on this Frog.
- **"With Sam" on Activity cards**, for viewers who may see both profiles
  (`store::shown_to`). The owner sees it on the title page and in the diary.
- **"Watching with…" from a title's own page.** Today it is in a card's
  right-click or long-press menu, in the "when did you watch it" dialog, and,
  once set, the title page's quiet "With Sam" line. A title page has no menu
  of its own to put it in without adding a button.
- **Select on a person's page**, as a studio has it (`PickBar.svelte`,
  `addMany`). Not wired in 1.7: the credits grid needs its own selection code.
- **A "watched most with" figure in Stats**, from `watch_with`.
- **A wrong TVmaze link, found and replaced.** A kept TVmaze id is never
  asked again, so The Daily Show (TMDB 2224) stays linked to TVmaze's Trevor
  Noah run, ended in 2022, and every episode reads time TBA. A link whose
  show has ended while TMDB's is running could be looked up afresh, the day
  schedules included.
- **Measure Rate your library on a large library.** It reads the whole
  library to deal (`api.library()`); not timed on a 600-title library for
  1.7 (no TMDB key on the machine it was built on).

Badges Frog gives on its own, left for later:

- **World Tour and Subtitles On badges** (titles from 15 countries, 25 not
  in English). Frog stores no country or language per title: keep TMDB's
  `origin_country` and `original_language` on `title` first.
- **Back From the Dead** (a dropped show picked up again) and **Watchlist
  Zero** (a watchlist of 20 or more emptied by watching). Frog keeps no
  history of status changes or of what left a watchlist and why.
- **A test for a "watched with" copy being judged on the original's time.**
  `together::add_to_mine` carries the original's `logged_at`; no test drives
  it end to end yet.
- **Tune the eligibility numbers on the owners' real libraries** (36 hours,
  half the runtime, 10 titles a day): run `awards::eligible` over a restored
  production backup and count what each rule turns away.

## To check by hand

Nothing automated can stand in for these. An owner tries each and takes it
off, or turns it into a fix above.

- **1.7 on a real phone:** the + status sheet, the people picker as a sheet
  (Recommend and Watched with), the bell and its count in the top bar, a long
  press on a studio's poster while not selecting, and the rating card's "…"
  sheet.
- **A recommendation and a "watched with you" reaching a phone** as push
  notifications, and tapping one opening Notifications.
- **Black Clover S2 E8 on the Android phone (Firefox), 2026-11-21 at 15:00
  Brussels.** E7 was announced at 14:00:40 UTC on 2026-11-14, to a device
  that has since gone; the phone subscribed again on 2026-11-15, and Mozilla's
  push service took a recommendation for it that day. If E8 does not show,
  the phone is holding it back: Android's notification setting for Firefox,
  or its battery restriction.
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
  picture. Opening one shows it whole, and the close button closes it.
- **Swiping rating cards on the Android phone:** the card follows the finger
  with no stutter, a throw skips, a short swipe springs back, and a tap on
  "Open the title" still opens it.
- **zschaeken as an owner:** once this release is live, set their role to
  Owner in Administration → People.
- **Air times on the live Airing page** a day after the release: far fewer
  time TBA rows. What is left should be streaming drops, shows TVmaze lists
  without a time, and shows TVmaze does not have.
- **The colour picker's sheet and the Stats bars by finger** on the phone.
- **The 1.9 batch on a real phone:** the Badges tab's cards and sorting,
  choosing a banner from any title in its sheet, each glow on Glass and
  Glass Light, the shelves dealing in, and Calendar's Month, Agenda and
  Diary switch. And a badge made in Administration → Badges with a
  condition reaching someone by itself once they pass it.
- **The November batch on a real phone:** swiping Home's hero and a
  notification away, the banner's Choose what shows sheet by finger, the
  shelves sheet, and Signed-in devices showing the phone and an iPad as
  such.
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
