# Roadmap

The working backlog. Claude reads this at the start of a session; whoever
finishes something updates it. **Next** is ordered — take from the top.

## Where Frog is

| | |
| --- | --- |
| **Release** | 0.6.0 "Leapfrog"; 0.7.0 "Treefrog" is in its pull request |
| **Next codenames** | Bullfrog (1.0) |
| **Live at** | a DuckDNS hostname, behind Caddy |
| **Hosted on** | Oracle Cloud Always Free, `VM.Standard.E2.1.Micro` — 2 vCPU, 954 MB, amd64, Amsterdam |
| **Updates** | by itself: a timer pulls the newest release every 15 minutes, backs up first, rolls back on an unhealthy `/healthz` |
| **Owners** | [Mihai Constantin](https://github.com/MihCons) and [Zjef Schaeken](https://github.com/zschaeken) |

Frog holds several accounts, each with its own library. `main` is protected,
releases are cut by release-please, and the image CI builds is the image
production runs. `docs/DEPLOY-ORACLE.md` is the deployment;
`docs/CONTRIBUTING.md` is how the two of you work on it.

## Next

1. **Back up somewhere other than the server.** `BACKUP_REPO_DIR` is blank, so
   nightly backups land in `/opt/frog/backups` and nowhere else — lose the
   instance and they go with it. Needs a private repository and a deploy key.
2. **Restore once, for real.** `restore.sh` has never been run. Do it against
   a throwaway instance before it is ever needed in anger.
3. **A development AniList client.** A client has one redirect URL, and the
   one in use points at production, so AniList sign-in cannot finish on a
   laptop. Register a second client with
   `http://127.0.0.1:8080/api/anilist/callback` for the local `.env`.
4. **AniList specials.** Entries that TMDB files under season 0 (OVAs,
   specials) are never placed automatically; they wait under "not matched
   yet". Fribb's list gives season 0 and an offset for some of them.
5. **More streaming links.** Services without a known search address (Rakuten
   TV, Streamz, VRT MAX, GoPlay…) still open TMDB's watch page. Add each one
   to `frontend/src/lib/providers.ts` once its search URL is confirmed.
6. **The screenshots, the rest.** There is still no shelves shot.
7. **The Projects board.** Backlog / In progress / Review / Done. Needs
   `gh auth refresh -s project` from an owner before it can be created.
8. **Netflix: real dates on every episode.** Episodes TMDB finds by name
   are ticked exactly, on the day Netflix gives. Seasons Netflix numbers
   itself are not looked up (that would mean reading every season of every
   show), so their episodes are still the first N, undated, and such a show
   reads "last watched today". Looking those up too, in the background after
   the import, would date everything. Films keep only their latest date;
   earlier watches of the same film are a rewatch flag, not entries.
9. **Netflix: untested shapes.** The personal-data ZIP
   (`CONTENT_INTERACTION/ViewingActivity.csv`) is read from its documented
   columns but has never been tried on a real one. Netflix's "Part N"
   seasons are placed by their episode names; parts whose episodes are only
   called "Episode 3" (Money Heist Part 1) still wait for a person.
10. **Smaller things.** Customisable keyboard shortcuts, renaming and
   recolouring statuses, muted trailer autoplay, per-shelf layout defaults.

## Known issues

| | |
| --- | --- |
| **Rebase merging doubles the changelog** | "Rebase and merge" rewrites commit hashes, so a change can land twice and release-please lists both. Untick *Allow rebase merging* in the repository settings. |
| **Backups are only on the server** | See Next 1. Until then a lost instance is a lost history. |
| **Characters come from what Frog has seen** | Search finds anime characters through AniList, and everyone else from the casts of titles somebody on this Frog has opened. TMDB has no character search. |
| **No staging** | 954 MB will not carry a second instance. `install.sh --with-staging` sets one up if it ever moves to a bigger machine. |
| **Notifications untested on a real phone** | Verified through FCM in Chrome; not on Android, and tapping one is untested. |
| **Server messages are English only** | Validation errors and notification text. Fine while the interface is English only. |

## Not doing

- **Jellyfin scrobbling, and the generic webhook it needed.** It only sees
  what plays through a Jellyfin server; the owners watch through streaming
  sites, so it would track nothing. Anime arrives through the AniList sync and
  the rest is ticked by hand, on purpose.
- **Plex.** Its webhooks need a paid Plex Pass.
- **Interface languages.** English only, decided 2026-10-26.
- **Oracle Pay As You Go.** Always Free only, whatever the console suggests
  when A1 capacity runs out.

## Notes that save time

- **Migrations are never edited once applied** — sqlx compares checksums and
  refuses to start, which takes a running instance down on its next deploy.
  Add another file. New ones get timestamped names, **digits only** before the
  first `_` (`202610271200_anilist.sql`): sqlx reads that part as a number and
  a `T` in it fails the build.
  `db::heal_line_endings` forgives a checksum that differs only in line
  endings, which is what lets a database move between a Windows checkout and
  the server.
- **The deployed Content-Security-Policy has broken the site three times** —
  the inline startup script, the update check, the old AniList import (the
  browser no longer talks to AniList; the server does). It comes
  from Caddy, so nothing local ever sees it: not a dev build, not the audit,
  not Playwright. Anything fetched from another origin has to be named in
  `deploy/caddy/Caddyfile`.
- **Rebuild `--release` before Playwright.** The e2e server picks whichever
  binary is newer, and a stale release build silently tests old code.
- **Verify scripts** live in `frontend/scripts/`, each starting its own server
  against a throwaway database: `verify-stats`, `verify-push`,
  `verify-shelves`, `verify-because`, `verify-anime`, `verify-leapfrog`
  (AniList is a stand-in server inside it), `verify-profile-stats` (no TMDB:
  it writes a small library straight into the database with `node:sqlite`),
  `verify-2fa`, `verify-diary`, `verify-polish`, plus `npm run pwa-check` and
  `npm run audit`. The ones that reach TMDB need a valid key; with a bad one
  every TMDB page answers 502.
- **Web Push in tests:** headless Chrome throws away displayed notifications,
  and Playwright's `--disable-background-networking` cuts it off from FCM.
  `verify-push.mjs` keeps background networking and records the worker's
  `showNotification` calls instead.
- **AniList sync** (`backend/src/anilist.rs`) places entries in this order:
  Fribb's anime-lists (TMDB id, TMDB season and offset, used only when no
  other entry claims the same spot), then Kometa's Anime-IDs (TVDB season and
  offset, turned into TMDB numbering through absolute positions), then an
  exact name and year on TMDB (animation only, unambiguous only), then
  AniList's own prequel chain counted on from the first season. Both lists
  reload weekly; the prequel chain is cached in `anilist_chain`. Each entry
  keeps a snapshot of both sides from the last sync; whichever side differs
  from its snapshot changed and wins. Lists are re-read every 5 minutes. A
  Frog title never on AniList is not added there, and removing one in Frog
  never removes it on AniList — both on purpose.
- **Netflix import** (`frontend/src/lib/import/netflix.ts`) groups plays into
  one row per show and season before matching, then `settleNetflix` finishes
  them with TMDB's answer: titles sent one by one fold back into their show,
  named seasons ("STONE OCEAN", "Egghead Arc ①") are placed only when a TMDB
  season has that name, and a show is completed only when every season was
  seen to the end. Shows are split by season marker first, then by where
  titles branch across the whole file, never by "the first colon". When a
  season is in doubt the server looks the episode names up across the
  show's seasons (`place_episodes` in `transfer.rs`); a name placed there
  settles the season, the exact episode and its date, and proves the match.
  `npm test` runs it against `tests/fixtures/netflix-viewing-history.csv`,
  real titles with made-up dates; `node --test` reads the TypeScript directly
  through `tests/ts-paths.mjs`.
- **Imported shows get no title-level diary entry.** `apply` only writes one
  for films; a show's watches are its episodes, and a title-level watch
  would count the show as a film in Stats.
- **Stats count only titles in the library.** Watches of removed titles stay
  in the database (the diary keeps them) but leave every total.
- **Stats credits** live in `title_person`, filled by `store::ensure_title`
  and backfilled by the worker, 25 titles every 2 minutes.
- **New Home rows** reach saved layouts once, through `homeRowsSeen` in
  `lib/settings.svelte.ts`. Add the id to `HOME_ROWS` and to the default list.

## Shipped

**0.7.0 Treefrog** — One search for titles, people, characters, studios and
members, with studio pages. AniList placed by TMDB id from Fribb's list, by
exact name and by prequel chain, re-read every 5 minutes, leftovers kept out
of the way. Trailers play (they needed a referrer). Streaming logos open the
service. Completed shows tick their aired episodes and offer the specials.
Settings that did nothing (date style, complete automatically) work, and one
that never could is gone. Phones get three posters across. Emptying a library,
four mascots and the 0.6.1 fixes ride along.

**0.6.0 Leapfrog** — AniList both ways through AniList's own sign-in, placed by
id rather than by name, with anything uncertain left for the person to choose;
the username import it replaces is gone. Stats, library totals and profiles
count exactly the library. Settings in six groups with their own addresses.
Administrators delete accounts. Profiles with a picture, banner, name, about
text, four pins and fuller totals.

**0.5.0 Ribbit** — Frog for more than one person. An account each, with every
personal row scoped to its owner and a test that fails the build if a query
forgets. Invitation-only joining, single-use and expiring. Per-account
settings, devices and two-factor; change your password, sign a device out,
take a copy of everything, delete your account. Optional profiles, off until
turned on. Administration for accounts, roles and invitations. AniList sync.
And the whole of it put on the internet: Docker image built for both
architectures, Caddy with automatic HTTPS, pull-based updates with backup and
rollback, encrypted nightly backups, `main` protected behind CI and review,
and releases cut by release-please.

**0.4.0 Lily Pad** — full stats with a year filter and a shareable year in
review; Web Push for new episodes with per-show mute; hand-picked and smart
shelves; "Because you watched" on Home; AniList and MyAnimeList import;
two-factor sign-in; "When did you watch this?" and a grouped, redatable diary;
the phone tab bar; status colours; data saver.

**0.3.0 Tadpole** — imports from Letterboxd, Trakt, IMDb and TV Time; JSON and
CSV export; calendar; diary; About; bundled typefaces; the Lucide icon set;
undo toasts, skeletons and empty states.

**0.2.0 Ripple** — the mascot and icons, the Pond theme, Home split from
Library, the command palette, the theme editor, server-side settings, privacy
headers, login rate limiting, release notes.

**0.1.0 Spawn** — tracking, episodes, library, title and person pages,
ratings, PWA, Docker, CI.
