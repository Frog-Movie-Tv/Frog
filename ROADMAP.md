# Roadmap

The working backlog. Claude reads this at the start of a session; whoever
finishes something updates it. **Next** is ordered — take from the top.

## Where Frog is

| | |
| --- | --- |
| **Release** | 0.7.0 "Treefrog" |
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

1. **Back up somewhere other than the server — on the server.** The scripts
   are ready (`backup-repo.sh`, the deploy key from `install.sh`, backups
   without the caches) and a full cycle passed in Docker on 2026-10-29:
   backup pushed over a deploy key, volume wiped, restored from a fresh
   clone. What is left needs an owner on the server: create the private
   repository, add the key, run `backup-repo.sh`. `docs/DEPLOY-ORACLE.md`
   step 7 has the commands.
2. **Restore once, for real, on the server.** Step 10 restores the copy in
   the repository into a throwaway `restoretest` instance beside production.
   The same commands passed in Docker; not yet run on the server.
3. **A development AniList client.** A client has one redirect URL, and the
   one in use points at production, so AniList sign-in cannot finish on a
   laptop. Register a second client with
   `http://127.0.0.1:8080/api/anilist/callback` for the local `.env`.
4. **AniList: check the overhaul on the live list.** On a copy of the owner's
   list (221 entries) 213 are matched, 7 wait as not out yet, and one film
   special ("ONE PIECE FILM: GOLD - episode 0") has nothing on TMDB. Once it
   is deployed, open Settings → Connections and look through "Needs a check"
   and the "Matched" methods for anything that reads wrong.
5. **AniList-only titles, what AniList cannot give.** They have everything
   else now (see the note below), but no title logo for the hero, no episode
   synopses, and streaming links with no country (AniList does not say
   where a service carries a show, so they show for everyone). AniList
   voice actors and studios are AniList people in Stats, apart from the same
   person on TMDB. Episode names and thumbnails are left out whenever the
   streaming site numbers past AniList's episode count.
6. **Old episode ratings that disagreed.** Seasons whose episodes were all
   rated the same took that as their season rating
   (`202610291800_anilist_resilience.sql`). The rest, and every episode of an
   anime show AniList already scores, stay in `episode_state` untouched and
   are not shown anywhere. The API no longer writes episode ratings. Show
   them read-only, or let them go.
7. **A merge you can undo from the app.** Every move of one account's data
   from one title to another is in `title_merge` with what both sides held
   before, but undoing one is still by hand in SQL.
8. **More streaming links.** Services without a known search address (Rakuten
   TV, Streamz, VRT MAX, GoPlay…) still open TMDB's watch page. Add each one
   to `frontend/src/lib/providers.ts` once its search URL is confirmed.
9. **The screenshots, the rest.** There is still no shelves shot.
10. **The Projects board.** Backlog / In progress / Review / Done. Needs
   `gh auth refresh -s project` from an owner before it can be created.
11. **Netflix: real dates on every episode.** Episodes TMDB finds by name
   are ticked exactly, on the day Netflix gives. Seasons Netflix numbers
   itself are not looked up (that would mean reading every season of every
   show), so their episodes are still the first N, undated, and such a show
   reads "last watched today". Looking those up too, in the background after
   the import, would date everything. Films keep only their latest date;
   earlier watches of the same film are a rewatch flag, not entries.
12. **Netflix: untested shapes.** The personal-data ZIP
   (`CONTENT_INTERACTION/ViewingActivity.csv`) is read from its documented
   columns but has never been tried on a real one. Netflix's "Part N"
   seasons are placed by their episode names; parts whose episodes are only
   called "Episode 3" (Money Heist Part 1) still wait for a person.
13. **Smaller things.** Customisable keyboard shortcuts, renaming and
   recolouring statuses, muted trailer autoplay, per-shelf layout defaults.

## Known issues

| | |
| --- | --- |
| **Rebase merging doubles the changelog** | "Rebase and merge" rewrites commit hashes, so a change can land twice and release-please lists both. Untick *Allow rebase merging* in the repository settings. |
| **Backups are only on the server** | See Next 1. Until then a lost instance is a lost history. |
| **The backup repository grows without end** | Git keeps every backup it was sent. About 3 MB a copy for 611 titles, one nightly plus one per update. `docs/DEPLOY-ORACLE.md` step 7 has the reset; doing it automatically, say yearly, is not built. |
| **Characters come from what Frog has seen** | Search finds anime characters through AniList, and everyone else from the casts of titles somebody on this Frog has opened. TMDB has no character search. |
| **No staging** | 954 MB will not carry a second instance. `install.sh --with-staging` sets one up if it ever moves to a bigger machine. |
| **Notifications untested on a real phone** | Verified through FCM in Chrome; not on Android, and tapping one is untested. |
| **Server messages are English only** | Validation errors and notification text. Fine while the interface is English only. |
| **One AniList test is flaky** | `a_tmdb_outage_is_retried_not_left_for_the_person` failed once in a full `cargo test` on 2026-10-29 and passed alone and on the rerun. It races the other tests over account 1's sync flag. |

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

- **Server jobs report to Frog through `ops.jsonl`** beside the database.
  `update.sh` and `backup.sh` append one line per outcome through
  `deploy/ops-log.sh` (kinds: backup, update, rollback, restore). `crate::ops`
  reads it for Administration's Server section. A watcher every 5 minutes
  pushes each new failure to administrators once; a failure that repeats the
  previous one of its kind (the timer retrying a broken release) stays quiet.
  Each administrator's mark is `app_setting` key `ops_seen`. The scripts
  reach the server only by re-running `install.sh` from a checkout, never
  with an image update.

- **Migrations are never edited once applied** — sqlx compares checksums and
  refuses to start, which takes a running instance down on its next deploy.
  Add another file. New ones get timestamped names, **digits only** before the
  first `_` (`202610271200_anilist.sql`): sqlx reads that part as a number and
  a `T` in it fails the build.
  **Migrations stay additive**: a failed update rolls back to the previous
  image, which skips migrations it does not know (`set_ignore_missing`) and
  has to run on the newer schema. A destructive change takes two releases;
  `docs/CONTRIBUTING.md` has the rule. If the previous image still fails,
  `update.sh` puts back the plain snapshot it took just before the update.
  `db::heal_line_endings` forgives a checksum that differs only in line
  endings, which is what lets a database move between a Windows checkout and
  the server.
- **The deployed Content-Security-Policy has broken the site three times** —
  the inline startup script, the update check, the old AniList import (the
  browser no longer talks to AniList; the server does). It comes from Caddy.
  The audit and Playwright now read it from `deploy/caddy/Caddyfile`, serve
  every page with it (the offline worker is blocked there, since it would
  fetch pages out of the policy's reach) and fail on anything it blocks. The
  audit also presses the two buttons that reach another origin (Check for
  updates, a trailer). Their first run caught a fourth break, already live: a
  JetBrains Mono subset under 4 KB was inlined as `data:font`, which
  `font-src 'self'` refuses, so fonts now always ship as files. The header's
  `script-src 'unsafe-inline'` stays: the inline script's hash changes with
  every build, and the app's own meta policy already narrows it to that one
  script.
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
- **AniList sync** (`backend/src/anilist/`): `place.rs` decides where an
  entry goes, `mod.rs` syncs. Evidence is gathered in this order and stops at
  the first confident answer (confidence 70+): Fribb's anime-lists (TMDB
  season and offset; a spot two entries claim scores 50), Kometa's Anime-IDs
  (TVDB numbering turned into TMDB's), a film or unreleased entry by name,
  the prequel chain counted on from the first season, the air date across
  related shows (specials only ever in season 0), then the series by every
  title and synonym AniList has. Every answer is checked against TMDB's air
  dates. Below 70 nothing is applied: the entry is "not confident" with a
  note and up to six candidates. Each placement records its method, note and
  the `RESOLVER` version; a sync re-places automatic ones from an older
  version, takes back only the ticks it gave (never ones another entry
  covers) and counts them as repaired. Manual choices are never moved. An
  entry of unknown length runs to the next entry's start or the end of the
  show, across TMDB seasons. Each entry keeps a snapshot of both sides from
  the last sync; whichever side differs from its snapshot changed and wins.
  A Frog title never on AniList is not added there, and removing one in Frog
  never removes it on AniList — both on purpose.
- **AniList, kept running** (also `anilist/mod.rs`). The worker looks every
  30 s and syncs an account `ANILIST_SYNC_MINUTES` (default 3) after its last
  finished sync, or 45 s after a change in Frog. The list is read in chunks
  of 500 and a sync fails whole if any chunk fails. Requests retry a dropped
  connection, a 5xx or an unreadable answer with jittered backoff (4 tries),
  and wait out a 429 as long as Retry-After says (65 s at most). `synced_at`
  moves only on success; a failure sets `retry_at` (1 min doubling to 60) and
  a refused token sets `needs_reconnect`, which only a reconnect clears.
  An entry TMDB cannot place is kept as an **AniList-only title**: a `title`
  row with `tmdb_id = -anilist_id` and `anilist_id` set, so every
  `(kind, tmdb_id)` lookup and address works; `store::is_local` tells them
  apart and nothing sends one to TMDB. Its progress is
  `library_entry.progress`, a count. It is looked for again on a schedule
  (`tries`, `next_try_at`: an hour doubling to three days, twelve hours when
  not out yet; at once when AniList's episodes, format, state or start date
  change, or on Sync now). Once placed, `store::move_title_data` moves that
  account's entry, shelves and film watch to the TMDB title in one
  transaction, logs it in `title_merge`, and points the old title on with
  `merged_into` once nobody keeps it. Progress lowered on AniList takes back
  only `source = 'anilist'` ticks. Entries gone from the list get `gone_at`
  and nothing else; an entirely empty answer marks nothing.
- **AniList-only titles look like TMDB ones** (`backend/src/anilist/media.rs`).
  One AniList query (plus its airing schedule, 50 a page) is rewritten into
  TMDB's detail shape, so every reader in `tmdb/models.rs` works unchanged:
  cover, banner, synopsis (HTML and Markdown stripped, spoilers dropped),
  genres in TMDB's names plus Animation, score, runtime, voice cast,
  director, creator and writer, main studio, YouTube trailer, streaming
  links with their own address (`Provider.url`), recommendations and
  related works (to the TMDB title when the anime lists map it), and season
  1 episodes in AniList's numbering, dated from the schedule (else only the
  first and last, from the start and end dates). Cached in `tmdb_cache`:
  12 h while airing, a week once finished; the worker refreshes airing ones.
  AniList people and studios are negative ids too, with their own person
  and studio pages. Pictures are `/anilist/…` and `/crunchyroll/…` paths that
  the image route fetches from those two hosts only, so Caddy's CSP needs
  nothing new. An AniList address whose show the anime lists map to TMDB
  answers `moved_to` instead of making a copy, unless a sync already keeps it.
  With episodes listed, the sync ticks and syncs them like any show's
  (`sync_local_episodes`), so Home, the calendar, statuses and season
  ratings all apply; when TMDB later places the entry, watches logged in Frog
  move to the episode at the same place in its run.
- **Released and status** (`backend/src/release.rs`): an episode is out when
  its date has passed, or, undated, when a later one has aired — the
  `released!()` SQL fragment, used by progress, Home, bulk marking and the
  export. `desired_status` keeps paused and dropped, makes anything with
  nothing out planned, unwatched released episodes or caught up on a running
  show watching, and the rest completed. `reconcile` runs after every tick,
  every sync and every show refresh.
- **Season ratings** live in `season_rating` (user, title, season, first
  episode). A season split into AniList parts is rated part by part; a unit
  is rateable once every episode of it is out. The show's own rating is no
  longer pushed to AniList. Episodes are no longer rated (see Next 6).
- **`cargo run --example anilist_check -- <user> [scratch.db]`** places a
  public AniList list without writing anywhere and prints one JSON line per
  entry: the quickest way to see what the matcher does with a real list.
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
- **The library filter matches `title.search_name`**, the name and original
  name with case and accents folded out (`text::fold`), against the typed
  term folded the same way. SQL cannot fold accents, so `upsert_title`
  writes it and `db::fill_search_names` fills any row without one at
  startup. Anything that inserts titles another way gets one on the next
  start, and until then matches by its plain lower-cased name.
- **A page that cannot load never shows the server's message.**
  `lib/failure.ts` picks the words from the status (404 and 400 are the
  page's own "isn't on TMDB" line; 502, 503 and offline have their own), and
  the server keeps the detail in its log. Unknown addresses land on
  `routes/+error.svelte`.
- **Up next** (`store::next_episode`, shared by Home and the title page) is
  the first unwatched episode after where you are: the latest dated watch,
  or the furthest undated one when that lies later in the show. Only once
  nothing after it is left does it go back to the first gap. On the
  611-title Netflix library this moved 17 of 204 shows in progress, each to
  the episode after the furthest watched, and changed nothing else.
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
