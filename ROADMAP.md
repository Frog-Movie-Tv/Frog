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

1. **Ship the catalog release with a snapshot at hand.** Frog's own title
   ids (`docs/CATALOG-IDENTITY.md`) are built and tested: fresh databases, a
   copy of the release before, an older binary writing between the
   migration and the rebuild, and every `update.sh` path against stand-ins
   for Docker (`deploy/tests/update-rollback.sh`). It is not an additive
   release, and it has not met the live database. Before it reaches a
   server, an owner has to:
   - copy the new deploy scripts to each server
     (`sudo cp deploy/*.sh /opt/frog/deploy/` from this release's checkout).
     The copy there predates the schema generation and would put the
     previous image back on the rebuilt table and call it healthy;
   - run a manual backup and see it arrive in `Frog-Movie-Tv/Frog-Backups`
     (`sudo /opt/frog/deploy/backup.sh production manual`);
   - know that production is the first live database it meets: there is no
     staging, merging to `main` changes nothing live, and merging
     release-please's pull request does. Afterwards read the update log
     and open a few titles, an AniList-only one among them.
2. **AniList sign-in on a laptop, once by hand.** The Frog-Dev client
   (redirect `http://127.0.0.1:8080/api/anilist/callback`) is in the local
   `.env`, and AniList accepts its id and secret (checked against its token
   endpoint on 2026-10-30). The production block above it used to win,
   because the first value in `.env` is the one kept; it is commented out
   now. What no script can do: run `.\scripts\dev.ps1`, connect AniList in
   Settings → Connections, and see it come back connected.
3. **AniList: check the overhaul on the live list.** On a copy of the owner's
   list (221 entries) 213 are matched, 7 wait as not out yet, and one film
   special ("ONE PIECE FILM: GOLD - episode 0") has nothing on TMDB. That
   was a copy, not the live list, and the automated tests do not stand in
   for it. Once the catalog release is deployed, an owner opens
   Settings → Connections and reads "Needs a check" and the "Matched"
   methods for anything that reads wrong.
4. **Old episode ratings that disagreed.** Seasons whose episodes were all
   rated the same took that as their season rating
   (`202610291800_anilist_resilience.sql`). The rest, and every episode of an
   anime show AniList already scores, stay in `episode_state` untouched and
   are not shown anywhere. The API no longer writes episode ratings. Show
   them read-only, or let them go.
5. **A merge you can undo from the app.** Every move of one account's data
   from one title to another is in `title_merge` with what both sides held
   before, but undoing one is still by hand in SQL.
6. **More streaming links.** Services without a known search address (Rakuten
   TV, Streamz, VRT MAX, GoPlay…) still open TMDB's watch page. Add each one
   to `frontend/src/lib/providers.ts` once its search URL is confirmed.
7. **The screenshots, the rest.** There is still no shelves shot.
8. **The Projects board.** Backlog / In progress / Review / Done. Needs
   `gh auth refresh -s project` from an owner before it can be created.
9. **Netflix: the dates still missing.** On the 4,054-line file 3,145 of
   3,550 episodes land on their day. The other 405 have names TMDB writes
   differently or not at all (Kuroko's Basketball, The Order), so they are
   counted from the start of the season, undated. Films keep only their
   latest date; earlier watches of the same film are a rewatch flag, not
   entries. Re-importing over an older import dates what it finds and leaves
   the old count's other ticks undated (26 on this file), since an import
   never removes a watch.
10. **Netflix: untested shapes.** The personal-data ZIP
   (`CONTENT_INTERACTION/ViewingActivity.csv`) is read from its documented
   columns but has never been tried on a real one. Netflix's "Part N"
   seasons are placed by their episode names; parts whose episodes are only
   called "Episode 3" (Money Heist Part 1) still wait for a person.
11. **What the readers still leave out.** A Frog backup restores which
   episodes were watched but not when, because the export does not write
   their dates, and only a film's first watch. A Trakt episode marks its show
   in progress without ticking that episode, though the file says which one.
   Letterboxd's ZIP may hold a `deleted/` folder that would be read as
   library; no real export has been tried.
12. **Finish the data-ownership pass.** The AniList, import, catalog and
   emptying-a-library code no longer lives in routes. Every route below was
   read for the catalog change on 2026-10-30: each now goes by Frog's title
   id, and the TMDB ids left in them are TMDB's own (its cache, credits,
   people and recommendations). SQL still in route
   files, all single-route reads or writes, to move when each is next
   touched: `routes/account.rs` (account, password, sessions, profile and
   pictures, deleting the account), `routes/two_factor.rs` (secrets and
   recovery codes; belongs beside `auth.rs`), `routes/profiles.rs` (profile
   cards and totals), `routes/shelves.rs` (shelves and smart-shelf rules),
   `routes/timeline.rs` (calendar, diary, redating), `routes/stats.rs`,
   `routes/titles.rs` (heatmap, a season's episodes and states),
   `routes/episodes.rs` (ticks and notes), `routes/home.rs`,
   `routes/library.rs` (the filtered list), `routes/search.rs`,
   `routes/browse.rs`, `routes/admin.rs`, `routes/invites.rs`,
   `routes/auth.rs`, `routes/push.rs`, `routes/settings.rs`. Persistence is
   not centralised yet.
13. **Smaller things.** Customisable keyboard shortcuts, renaming and
   recolouring statuses, muted trailer autoplay, per-shelf layout defaults.
14. **Finish the interface overhaul** (`feat/ui-overhaul`). Done: the top
   bar with a search field, Home (shelf covers and a New shelf tile first,
   "Airing this week" as cards, two "Because you watched" rows at most),
   Shelves, Search (categories as tabs, filters beside the results), the
   title page (one meta line, AniList / IMDb / Rotten Tomatoes / Metacritic
   as percentages with their marks, TMDB standing in while MDBList has
   nothing, one family of controls, crew and where to watch beside the
   episodes), Administration's layout, one page header on Library,
   Calendar, Diary, Stats and Import. Still to do the same way: Settings,
   People and profiles, a person's page, Calendar and Diary bodies, Stats,
   What's new and About; one pass over dialogs, menus and toasts so they
   share a surface; and new screenshots, since every page has changed.
   An earlier try put the sections in a left sidebar; the owner preferred
   the bar on top.

8. **Left from the UX pass.** Each small, none urgent:
   - A person's filmography leads with talk-show and documentary appearances;
     sort acting credits by popularity, or put them first.
   - About shows the browser's raw error when the offline worker cannot
     install; say it in a sentence and keep the detail behind a disclosure.
   - Deleting a shelf leaves its row id in the saved Home layout, and undo
     restores the shelf under a new id, so it drops off Home.
   - Administration shows initials, not the uploaded picture.
   - Undoing an unticked episode restores its latest watch only; earlier
     rewatches of that episode stay cleared.

## Known issues

| | |
| --- | --- |
| **Rebase merging doubles the changelog** | "Rebase and merge" rewrites commit hashes, so a change can land twice and release-please lists both. Untick *Allow rebase merging* in the repository settings. |
| **The backup repository grows without end** | Git keeps every backup it was sent. About 3 MB a copy for 611 titles, one nightly plus one per update. `docs/DEPLOY-ORACLE.md` step 7 has the reset; doing it automatically, say yearly, is not built. |
| **AniList-only titles, what AniList cannot give** | Accepted, not planned. No title logo for the hero and no episode synopses (AniList has neither). Streaming links show for every country, since AniList does not say where a service carries a show. AniList voice actors and studios are AniList people in Stats, apart from the same person on TMDB. Episode names and thumbnails are left out when the streaming site numbers past AniList's episode count. Everything else (detail, artwork, episodes, sync, ratings, shelves, Stats) works as for any title and is covered by `tests/anilist.rs` and `verify-leapfrog`. |
| **Characters come from what Frog has seen** | Search finds anime characters through AniList, and everyone else from the casts of titles somebody on this Frog has opened. TMDB has no character search. |
| **No staging** | 954 MB will not carry a second instance. `install.sh --with-staging` sets one up if it ever moves to a bigger machine. |
| **Notifications untested on a real phone** | Verified through FCM in Chrome; not on Android, and tapping one is untested. |
| **Server messages are English only** | Validation errors and notification text. Fine while the interface is English only. |
| **Imports keep the day, not the time** | Every reader sets a watch to 20:00 UTC on its day, so Trakt's exact time is not kept. Recorded in `docs/IMPORTS.md` and pinned by `tests/parsers.test.mjs`; keeping exact times needs its own requirements and tests. |
| **The database file does not shrink** | The picture cache is held to `FROG_IMAGE_CACHE_MB`, but SQLite only reuses freed pages. A one-off `VACUUM` (with the instance stopped) gives the space back if it ever matters. |
| **Screenshots still weigh 13 MB** | Down from 20 MB by shooting at 1.5×. A lossless optimiser in the release step would take more off; none is part of the build. |
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

- **Ratings** (`backend/src/ratings.rs`) sit in `title_rating` on Frog's own
  title, matched only by provider id. MDBList's free tier is 1,000 requests a
  day: the worker reads 20 titles per half hour, in batches of 10, and stops
  at 200 left so opening a title still works. A title is asked about again
  after 3, 14 or 45 days by age. An AniList-only title borrows TMDB's score and
  logo through `anime_map` without being linked to TMDB: the sync decides that.
- **Search filters** match a name here, over three of TMDB's pages, because
  TMDB can only filter when there is no name (`/discover`). A status filter
  without "Not in my library" goes through `shelves::evaluate` instead.
- **What is followed** is one SQL fragment, `release::FOLLOWED`, shared by the
  calendar, Airing this week and notifications.

- **Titles are Frog's** (`backend/src/catalog.rs`,
  `docs/CATALOG-IDENTITY.md`). `title.id` is the identity; provider ids are
  rows in `title_external`. `/t/{id}` is a title's page and API; a
  provider's address opens it and moves there. Seed and verify scripts open
  titles by TMDB address first (`titleApi` in each), and SQL-written fixtures
  add their `title_external` row (`backend/tests/common`'s `title_seed`).
- **Backups leave the server.** Every nightly and pre-update backup is
  age-encrypted and pushed to the private `Frog-Movie-Tv/Frog-Backups` with
  the server's deploy key (`/opt/frog/backup-key`; the organisation allows
  deploy keys since 2026-10-30). The server holds only the public half of the
  age key; the owners keep the private half, and without it no backup opens.
  A real restore of the newest copy from that repository into a throwaway
  instance was done on 2026-10-30: 2 accounts, 193 library entries and 6,909
  watches, the same as production. `docs/DEPLOY-ORACLE.md` steps 7 and 10.

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
  `verify-2fa`, `verify-diary`, `verify-import`, `verify-polish`,
  `verify-accounts` (invites,
  isolation, devices, switching off), `verify-pull` (pull to refresh with
  real touch input), plus `npm run pwa-check` and
  `npm run audit`. The ones that reach TMDB need a valid key; with a bad one
  every TMDB page answers 502.
- **Web Push in tests:** headless Chrome throws away displayed notifications,
  and Playwright's `--disable-background-networking` cuts it off from FCM.
  `verify-push.mjs` keeps background networking and records the worker's
  `showNotification` calls instead.
- **AniList** — placing, syncing, AniList-only titles, season ratings and
  the `anilist_check` example: [docs/ANILIST.md](docs/ANILIST.md).
- **Released and status** (`backend/src/release.rs`): an episode is out when
  its date has passed, or, undated, when a later one has aired — the
  `released!()` SQL fragment, used by progress, Home, bulk marking and the
  export. `desired_status` keeps paused and dropped, makes anything with
  nothing out planned, unwatched released episodes or caught up on a running
  show watching, and the rest completed. `reconcile` runs after every tick,
  every sync and every show refresh.
- **Imports and exports** — matching, conflict modes, Netflix, the backup
  formats: [docs/IMPORTS.md](docs/IMPORTS.md).
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
- **Last watched counts dated watches only.** An undated watch holds the day
  it was recorded, so counting it made a show read "today" after an import.
  A title with only undated watches has no last-watched day; Home's Continue
  watching then falls back on `updated_at`.
- **Lists of titles are built from one reading** (`store::ItemFacts`): five
  queries for the whole account, then `item` per title. Library and shelves go
  through `build_items`; Home loads it once and shares it across its rows. On
  the 611-title library a 399-title shelf went from about 640 ms to 96 ms and
  Home from 145 ms to 86 ms, field for field the same JSON. A six-title shelf
  went from 13 ms to 38 ms, accepted to keep one code path. `build_item` stays
  for the pages about one title. Its totals also cover shelf titles, because a
  title can stay on a hand-picked shelf after leaving the library.
- **Very long shows were measured and left alone** (2026-10-30). Seasons
  already draw only when opened: One Piece (1,180 episodes) shows 25. Its extra
  elements are the episode heatmap, one per episode, 1,262 of 2,387. Drawing
  that only near the screen halves the elements but moved no time past the
  noise: at 4× CPU the main thread blocked 1,838 ms before and 1,453 ms after,
  while Breaking Bad, which it does not touch, went 914 ms to 569 ms. Desktop
  showed nothing. Removing the heatmap outright reached only 1,250 ms, and
  deferring just moves the work to the middle of a scroll, so it was not
  shipped. Compare against an untouched show before trusting any number here.
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
