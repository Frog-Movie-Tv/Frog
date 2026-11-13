# Frog — notes for Claude

Frog is a self-hosted film and TV tracker for a handful of invited people.
One Rust binary serves the API and the built SvelteKit app; one SQLite file
holds everything.

It runs as a public site on an Oracle Cloud free instance, behind Caddy with
automatic HTTPS, and updates itself by pulling the newest release image.
`docs/DEPLOY-ORACLE.md` is the whole story. It still runs on a laptop for
development, which is what `scripts/dev.ps1` is for.

**Start every session by reading [ROADMAP.md](ROADMAP.md)** — it lists what is
still to build and what is known to be broken, nothing else. Take an item off
when it ships. How things work lives here, under **How things work**.

## Hard rules

- **Zero cost.** No paid APIs, no AI/LLM features, no services that need a card,
  no runtime CDNs (fonts and icons are bundled). TMDB's free key, and the free
  MDBList and fanart.tv keys (both optional), are the only credentials. AniList
  and TVmaze are read without one. If something would cost money, say so and
  leave it out.
- **TMDB is the source of truth for what exists.** Which shows and episodes
  exist, their names, pictures, numbers and popularity come from TMDB. When an
  episode TMDB lists airs, its day and its moment, comes from AniList for
  anime and TVmaze for the rest (`airtime.rs`), matched to TMDB's episode;
  TMDB's day stays only for an episode neither has. Nothing else from them
  reaches the Airing page or an episode.
- **AniList is read only.** Frog never writes to it. `Api::graphql` refuses
  anything that is not a query; keep it that way.
- **Invitation only.** Several accounts, no open sign-up: an administrator
  makes a single-use link and that is the only way in. Nothing is visible to
  anyone without an account.
- **Every personal row names its owner.** Library, ratings, episodes, diary,
  shelves, settings, sessions, two-factor and notifications all carry a
  `user_id` (so do the extension's tokens and logs); only titles, episodes,
  credits, the TMDB and image caches and the push keys are shared. A query that reads or writes personal data without
  naming whose it is fails `backend/tests/scoping.rs`, and that is a real
  guard, not a style rule — it is what stops one account seeing another's.
- **Never print or commit `TMDB_API_KEY`, `MDBLIST_API_KEY`,
  or `FANART_API_KEY`.** They live in `.env`, which is
  gitignored. MDBList's key travels in the request's address, so errors from
  those requests are logged without it (`ratings::quiet`); fanart.tv's goes in
  a header.
- **Product voice everywhere the user reads** — UI copy, changelog, README.
  Short, confident, neutral, like Apple TV or Letterboxd. Never explain the
  implementation or the roadmap in the UI. Never write "alpha", "finally",
  "phase", or describe how something used to be broken.

## Layout

```
backend/            Rust: axum 0.8, sqlx 0.8 (SQLite), tokio
  src/routes/       HTTP only, one module per area: parse, name the account, call, shape
  src/catalog.rs    Frog's titles and their TMDB/AniList/IMDb ids (title_external), episodes
  src/store.rs      one account's library: entries, progress, up next, moving data between titles
  src/images.rs     the picture cache and its budget
  src/bulk.rs       one action on many titles; marking aired episodes
  src/release.rs    when an episode is out; what a show's status should be
  src/transfer/     imports (matching.rs, apply.rs) and exports (export.rs)
  src/anilist/      AniList: client, idmap, place, reconcile, library, local,
                    media, records, sync — docs/ANILIST.md has the map
  src/ratings.rs    scores from other sites, on the title itself
  src/tokens.rs     personal tokens and TokenAuth, the extension's only way in
  src/extension.rs  the extension: resolve a page, log, undo, the popup's list
  src/fanart.rs     title logos from fanart.tv where TMDB has none
  src/tvmaze.rs     TVmaze client: a show's episodes and their airstamps
  src/airtime.rs    the moment a TMDB episode airs, from AniList or TVmaze
  src/airing.rs     the Airing page: every show on TMDB this week, by popularity
  src/announce.rs   notifications: what is due for whom, and what it says
  src/tmdb/         TMDB client, cache, and the JSON → view mapping
  src/auth.rs       sessions, roles, the Auth and AdminAuth extractors
  src/jobs/         background worker (sessions, caches, shows, AniList, ops)
  migrations/       timestamped .sql files, embedded at compile time
  tests/            api, anilist, catalog, images, transfer, scoping, migrations;
                    common/ has the fixtures (title_seed, title_id)
extension/          the browser extension: WXT + Svelte 5, Manifest V3, Chromium and Firefox
  src/entrypoints/  content (every frame), background (the decisions), popup, options
  src/lib/          signals, rules, playback, queue: pure, tested with node --test
  e2e/              fixture pages, a stand-in Frog, Playwright against the built extension
frontend/           SvelteKit 3 + Svelte 5 runes, adapter-static SPA, Vite 8
  src/lib/components/   Icon, Select, Rating, Poster, Rail, Empty, …
  src/lib/components/settings/   one component per Settings group
  src/lib/import/       client-side readers: common, parsers, anime, netflix
  tests/            node --test unit tests (parsers, Netflix, review, release notes)
  scripts/          icons, changelog → JSON, screenshots, audit, e2e server
  e2e/smoke.spec.ts Playwright, desktop + Pixel 7
scripts/dev.ps1, dev.sh   one command: build, seed two demo accounts, run
scripts/seed-demo.mjs     the two demo accounts, for seeing isolation by eye
deploy/             compose, Caddyfile, install/update/backup/restore scripts
ARCHITECTURE.md     layers, modules, data model — read before moving code
docs/ANILIST.md     the AniList sync, module by module
docs/IMPORTS.md     imports and exports, and the rules they must keep
docs/CATALOG-IDENTITY.md  Frog's own title ids, provider ids, merges, addresses
docs/INSTALL.md     install, updating, backups
docs/CONTRIBUTING.md  how the two of us work on it
docs/DEPLOY-ORACLE.md the live deployment, and the research behind it
docs/EXTENSION.md   the browser extension: install, tokens, detection, site rules
RELEASE-NOTES.md        source of truth for release notes (parsed into the app)
```

## Where new code goes

[ARCHITECTURE.md](ARCHITECTURE.md) has the layers. In short:

- **Titles are Frog's.** Personal rows, links and API paths use `title.id`
  (`/t/{id}`, `/api/t/{id}/…`). A TMDB or AniList id is looked up with
  `catalog::find` / `catalog::ensure`, never stored on a personal row, never
  used as a title's identity. Frontend links come from `lib/titles.ts`.
- **Routes are HTTP only.** Check input, take the account from `Auth`, call a
  domain module or `store`, shape the JSON. A decision (who wins, what a
  row means, what a status should be) does not belong in a route.
- **Domain modules decide; persistence modules read and write.** Keep the
  decision as a pure function on plain values where you can (see
  `anilist/reconcile.rs`, `transfer/matching.rs::choose`) and test it
  without a database.
- **A domain that owns tables owns their SQL** (`anilist/records.rs` for
  `anilist_*`, `shelves.rs` for shelves). Library, title and episode reads
  that more than one area needs go in `store.rs`. **No SQL in a route**: a
  new area gets a module of its own beside the others (`stats.rs`,
  `timeline.rs`, `profiles.rs`, …).
- **Providers are reached through their client** (`tmdb`, `anilist::client`,
  `anilist::media`). Never call TMDB or AniList from a route or from `store`
  with a hand-built request.
- **Lower layers never import upper ones.** `store` and domain modules never
  use `crate::routes`; `anilist`'s modules depend one way (see
  docs/ANILIST.md).
- **Before adding a helper, look for the one that exists.** Status words,
  merging (`transfer::merged_status`), release rules (`release.rs`), CSV
  (`lib/import/common.ts`), downloads (`lib/download.ts`) each have one home.
- **Split a file when it does two unrelated jobs, not because it is long.**
  `anilist/place.rs` and `import/netflix.ts` are long and cohesive. No
  catch-all `utils` modules, and no trait or layer that has one
  implementation and no test that needs it.
- **Change the docs with the code.** A new module, table or data flow goes
  in ARCHITECTURE.md or the subsystem guide in the same pull request.
- These are defaults, not gates: if the existing structure is wrong for a
  new feature, change it and say why in the pull request.

## Commands

In Git Bash, put cargo on PATH first: `export PATH="$HOME/.cargo/bin:$PATH"`.

| What                                  | Command                                                                                                       |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Run it (port 8080, two demo accounts) | `.\scripts\dev.ps1` (`-Fresh` to start over)                                                                  |
| Run it without Docker, no seed        | `.\scripts\run-local.ps1` (development only)                                                                  |
| Backend checks                        | `cd backend && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test`         |
| Frontend types                        | `cd frontend && npm run check`                                                                                |
| Frontend unit tests                   | `cd frontend && npm test` — parsers, Netflix, the import review, the release notes' layout; no network        |
| Frontend build                        | `cd frontend && npm run build` (also writes `static/changelog.json`, which is not tracked)                    |
| End-to-end                            | `cd frontend && npx playwright test` — needs `npm run build` and a built backend first                        |
| Page audit                            | `cd frontend && npm run audit` — every page, both viewports, console errors, service worker                   |
| Screenshots                           | `cd frontend && npm run screenshots` — needs the TMDB key, ~8 min, 1.5× (`-- --scale=2 --out=dir` to compare) |
| Icons                                 | `cd frontend && npm run icons` — regenerates every icon from `scripts/mascots.mjs`                            |
| Extension                             | `cd extension && npm run check && npm test && npm run build && npx playwright test`                           |

Ports: the owner's instance uses 8080. E2E uses 8098/8099, the extension's
e2e 8071 (a stand-in Frog) and 8072 (fixture pages), audit 8095,
screenshots 8096, pwa-check 8094, and the verify scripts 8084–8093 and 8097 —
each with its own throwaway database.

## Gotchas that have already cost time

- **SvelteKit 3 differs from SvelteKit 2.** `$lib` is gone — use `#lib/…`
  (mapped in `package.json` `imports` and `tsconfig.json` `paths`).
  `$app/environment` is `$app/env`. `$service-worker` is gone — use
  `$app/manifest` and `$app/env`. There is no `svelte.config.js`; config goes in
  `sveltekit({…})` in `vite.config.ts`.
- **A shallow navigation does not move `page.url`.** The address goes to
  `page.shallow.url`, there and on Back, so a page that keeps its state in the
  address reads it through `lib/address.ts`. Test Back from a clicked link:
  a `page.goto` reloads the app and never meets this.
- **TypeScript is pinned to ^6** because svelte-check rejects 7.
- **axum 0.8 path params are `{name}`**, not `:name`.
- **Clippy runs with `-D warnings` in CI.** `type_complexity` fires on big
  `query_as` tuples — add a `type FooRow = (…);` alias with a comment naming the
  columns.
- **SQLite aggregates.** `SUM()` over INTEGER columns returns INTEGER, so
  `CAST(… AS REAL)` before decoding as `f64`. `MAX()`/`MIN()` always return one
  row that may hold NULL — decode with `fetch_one` into `(Option<T>,)`, never
  `fetch_optional` into `(T,)`.
- **Bash heredocs break on apostrophes in this environment.** For anything
  with a `'` in it, use the Write/Edit tools, or write a Python patch script to
  the scratchpad and run that.
- **SQLite cannot rebuild a referenced table inside a migration.** sqlx runs
  every migration in a transaction, where `PRAGMA foreign_keys = OFF` does
  nothing, so dropping and recreating a table others point at cascades into
  them. `db::rebuild_title` shows how it is done instead. Seed scripts that
  write titles straight into SQL must also write their `title_external` row.
- **Stop the running server before `cargo build --release`** — Windows locks
  `frog.exe` and the build fails with "Access is denied".
- **The Claude browser pane cannot register service workers.** It fails even
  for a one-line worker. Use `npm run audit`, which drives real Chromium.
- **Offline mode needs HTTPS.** A browser refuses to register the worker over
  plain HTTP, so it works on the deployed site and on localhost, but not over
  a bare IP. About shows the status.
- **The deployed Content-Security-Policy has broken the site three times.**
  It is sent by Caddy, so a dev build never sees it. `npm run audit` and the
  Playwright suite read it from `deploy/caddy/Caddyfile`
  (`frontend/scripts/caddy-csp.mjs`), serve every page with it and fail on
  anything it blocks. Anything the browser fetches from another origin, and
  any inline script, still has to be named there. A request that only happens
  on a click is checked only if the audit's `ACTIONS` presses that button.

## Conventions

- Comments explain _why_, not what. Match the density of the file you are in.
- Ratings are stored 0–100. Score formats are display only.
- Errors: database and internal faults show a generic message and log the
  detail; anything the user can act on keeps its own message.
- Icons come from `Icon.svelte` (Phosphor paths, inlined, regular and filled).
  Never use a unicode glyph as an icon; add the Phosphor icon's two paths to
  `ICONS` instead, with its Phosphor name over it.
- A theme is only tokens. Shape and layout live in components and the shared
  primitives in `app.css` (`.segmented`, `.list-pane`, `.card`, …), never in
  rules keyed on `data-theme`. A menu, sheet or dialog fills with `--bg-float`.
- Dropdowns use `Select.svelte`, not native `<select>`.
- Destructive actions use `toasts.undoable(…)`, not a confirm dialog.
- Empty states use `Empty` with `art`, a short title and one clear action.
- Every route sets `<svelte:head><title>Name · Frog</title></svelte:head>`.
- New API routes go behind the `Auth` extractor — or `AdminAuth`, which
  refuses anyone below administrator — and get added to the auth-wall test in
  `e2e/smoke.spec.ts`.
- Rails use fixed card widths; never `1fr` (one card would stretch to full width).

## Working together

Two owners work on this in parallel, often both with Claude Code.
`docs/CONTRIBUTING.md` is the full version; the rules that matter here:

- **Never commit to `main`.** It is protected. Branch `feat/…`, `fix/…`,
  `docs/…`, `chore/…`, open a pull request, get CI green and the other owner's
  approval.
- **Conventional Commits**, because release-please reads them. `feat`, `fix`,
  `perf`, `refactor` and `docs` reach the changelog; `test`, `build`, `ci`,
  `chore` and `style` do not. Since 1.0 a `fix` bumps the patch and a `feat` the
  **minor**.
- **A change people can notice brings its release note.** A line under Upcoming
  in `RELEASE-NOTES.md`, in product voice, in the same pull request. CI fails
  without it; `Release-Notes: none` in a commit says it reaches nobody.
- **Never edit a version or `CHANGELOG.md`.** Both are generated.
  `RELEASE-NOTES.md` is the hand-written, product-voice file the app reads.
- **New migrations get timestamped names** (`202610261730_thing.sql`, digits
  only: sqlx reads everything before the first `_` as a number, so a `T` in
  it fails the build), and an applied migration is never edited — sqlx refuses to start on a changed
  checksum. Add another one.
- **Every query touching personal data names its owner.**
  `backend/tests/scoping.rs` fails the build otherwise. Genuinely shared
  statements go in its `ALLOWED` list with a reason.
- **Install the hooks once per clone:** `git config core.hooksPath .githooks`.
  pre-commit runs rustfmt, prettier and eslint (staged files) and blocks `.env`, databases and edited
  migrations; pre-push runs clippy, tests and svelte-check.
- **Rebuild `--release` before Playwright.** The e2e server prefers the release
  binary, so a stale one silently tests old code.

## Shipping

Releases are automatic. Merging to `main` builds the `:main` image (nothing
live runs it: the server is production only); release-please keeps a release
pull request open, and merging that one tags the release, which is what
production follows.

Every release gets an image, whatever its size: the build never looks at the
version. Notes are written with each change under Upcoming, and
`scripts/draft-release-notes.mjs` gives them their version in the release pull
request. The `/release` skill is now only for the part a person has to do: a big
release's codename and notes, and checking the screenshots. Commit or push only
when the owner asks.

## Working style the owner expects

- Requests arrive as large batches. Work through them in the order given,
  finish each piece properly, and at the end say plainly what is done and what
  is not. Move anything unfinished into ROADMAP.md, worded as work to do.
- Verify before claiming: run the tests and the audit, and say if something
  could not be checked (a real Android install, the Docker build).

## How things work

What each part of Frog does and why, learned while building it. Read the
part you are about to touch; change it when the code changes.

- **Air times and notifications** (`backend/src/airtime.rs`, `tvmaze.rs`,
  `announce.rs`). `airtime::resolve` gives each TMDB episode its day and,
  when known, its moment as UTC: AniList's `airingSchedule` for anime
  (Animation, origin JP), matched by the day in Japan (±1 for a late-night
  slot) through `anime_map`'s season and offset, by number only when one
  airing answers; TVmaze for the rest, and for anime episodes AniList cannot
  place. `match_tvmaze` pairs episodes the way imports pair names: name (same
  words in any order; "Episode 12" says nothing), then day, then number, the
  surest pairs first, each used once, a tie placing nothing. TVmaze numbers
  talk shows differently, so no signal decides alone (Watch What Happens Live
  runs one behind TMDB). TVmaze writes a stand-in 12:00 UTC when it has no
  time (`airtime` empty): the day stands, the time is TBA. The provider's day
  goes in `air_date` and the moment in `air_at`, in `episode` and
  `airing_episode`, with `air_source` 'anilist' or 'tvmaze'; while it is set,
  TMDB refreshes and the Airing list leave both alone, and when a provider
  loses the episode TMDB's day comes back at the next refresh.
  `cargo run --example airtime_check -- <tmdb id> <season> all` prints a
  season's pairing.
  `refresh_due` works every listed or tracked show out every six hours;
  `airtime_show` keeps how a show is found, for good (a TVmaze miss is asked
  again after a week). `released!()` uses `air_at` when there is one.
  Notifications are checked every minute: an episode at its `air_at`, an
  episode or film with a day alone at midnight in the account's zone
  (`app_setting` `time_zone`: Settings → Watching's zone when chosen, else
  the browser's, sent with every subscribe). The service worker fetches the
  notification's picture itself, signed in, and hands it over as a data URL.
  `verify-push` points `TVMAZE_API_URL` nowhere, so its hand-set times stay.
  `cargo run --example airtime_check -- <tmdb id> <season> <episode>` shows
  what one episode resolves to.
- **The Airing page** (`airing.rs`, `routes/airing.rs`). Once a day the
  worker reads TMDB's `/discover/movie` for films released in `FROG_REGION`
  in the window (last week to four weeks ahead; 25 pages, in cinemas or
  digital), then `/discover/tv`, every page, then each show and its running
  seasons, with
  `Tmdb::fetch`, which is never cached: kept whole, 2,700 shows would fill the
  database. Only a few fields are kept. A day is asked for in the viewer's
  zone (`tz`); an episode with a moment falls on that moment's day there,
  else on TMDB's day. One entry per show or film a day, most popular first, 30 at a
  time; the page groups a day by part of the day in the viewer's zone. There
  is no manga on it: TMDB has none, and the page shows only TMDB's titles.
  The page never reaches another site.

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
  `docs/CONTRIBUTING.md` has the rule. One taken in a single release raises
  `db::SCHEMA_GENERATION` (2 since 1.5.0 dropped `title_airing`), so a failed
  update restores the snapshot instead. If the previous image still fails,
  `update.sh` puts back the plain snapshot it took just before the update.
  `db::heal_line_endings` forgives a checksum that differs only in line
  endings, which is what lets a database move between a Windows checkout and
  the server.
- **One long word cannot widen a page.** `body` sets `overflow-wrap: anywhere`,
  which every page inherits and which also lowers the least a flex or grid track
  shrinks to. The cost is that tight rows of Frog's own words can stack letter by
  letter, so those set `overflow-wrap: normal` (Settings → Home, What's new's
  version column). `verify-long-names` compares every page with the rule on and
  off, and text kept on one line with `nowrap` still needs `min-width: 0` on its
  column. The Playwright test `a very long name never widens a page on a phone`
  is the guard for anything new.
- **Verify scripts** live in `frontend/scripts/`, each starting its own server
  against a throwaway database: `verify-stats`, `verify-push`,
  `verify-shelves`, `verify-because`, `verify-anime`, `verify-leapfrog`
  (AniList is a stand-in server inside it), `verify-profile-stats` (no TMDB:
  it writes a small library straight into the database with `node:sqlite`),
  `verify-2fa`, `verify-diary`, `verify-import`, `verify-polish`,
  `verify-long-names` (real titles, ordinary and very long names, phones),
  `verify-pictures` (uploads in Chromium, as Safari with no WebP writer, and in WebKit; needs `npx playwright install webkit`),
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
- **Library filters** live in the address, read through `lib/address.ts`,
  because a shallow navigation leaves `page.url` where it was. The server
  answers items, status counts and genres from one request; a status counts every
  filter but itself, so its number is what choosing it shows. The page keeps its
  last answer in the module, so Back draws at once and the scroll lands.
- **Shelves have one place on Home**, the Your shelves row. A shelf used to be
  able to add a row of its own, which showed it twice; saved layouts lose those
  rows as they load (`introduceRows`).
- **Administration → Database is read only and keeps secrets back**
  (`backend/src/dbview.rs`). A new column named with hash, secret, token,
  key, p256dh or auth fails `tests/dbview.rs` until it goes in `HIDDEN` or,
  with the reason, in `PLAIN`. Hidden columns are never searched or sorted
  on, since either would let someone guess them.
- **Administration shows initials on purpose.** A picture is served only to those
  who can see the profile (`visible` in `routes/profiles.rs`), so drawing one
  there would show it for an account that has its profile off.
- **The Dark themes' accent stays at 4.37:1 on its buttons.** White and that
  theme's background are about 20:1 apart, so no single accent reaches 4.5
  against both, and it is also a text colour. Bold button text keeps it.
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
- **Edit mode** (`backend/src/bulk.rs`, `POST /api/library/bulk`) is one request
  per action. Remove, status and favourite answer with the entries as they were
  (`store::EntryRecord`); undo sends those back, and `restore_entries` writes
  them into the signed-in account whatever they say. A title page's remove goes
  the same way. Marking a show here is `bulk::tick_aired`, the same code as Mark
  all watched.
- **Your people** (`backend/src/activity.rs`, `/api/home/activity`) is its own
  request, so nobody who switched the row off reads other accounts. Fourteen
  days, three titles a person with the rest as a number on their last card,
  twelve cards in all. It reads the whole fortnight, so that number is exact. A show is Finished only when it stands completed and its
  last regular episode is in the run. `backend/tests/scoping.rs` also holds that
  `activity.rs` and `taste.rs` name no notes and go through `store::shown_to`.
- **Taste match** (`backend/src/taste.rs`) is `100 - 2 × the average difference`,
  from 5 shared titles up. The doubling is on purpose: people rate near the top,
  so the plain average reads high for anybody. A show rated by season counts at
  the average of its seasons.
- **Someone else's library** is `LibraryBrowser` with `owner`, reading 60 titles
  at a time from `/api/profiles/{name}/library`. Its items are
  `store::SharedItem`, which has no notes field: do not hand that route
  `LibraryItem`. Having the profile on is the only switch.
- **Keyed lists throw in production.** A repeated key in a Svelte `{#each}` takes
  the whole page down, not just a warning. Anything a provider or a saved setting
  feeds goes through `uniqueBy`, and `models` narrows TMDB's lists as well.
- **Episodes are always listed for a tracked show.** An import ensures them even
  with nothing to tick, a title page does behind the page, and the worker fills
  any left (`fill_missing_episodes`, 25 a tick). Without them a show has no next
  episode, and its total is only TMDB's count.
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
- **Pictures are made ready in the browser** (`lib/picture.ts`): avatars at 512
  pixels, banners at 1800 on the long side. JPEG stays JPEG and anything else
  becomes WebP, so transparency lasts; a browser with no WebP writer hands back
  PNG. Anything that moves goes up untouched, because a canvas would keep only
  the first frame: GIFs by their type, APNG and animated WebP by reading their
  bytes (`isAnimated`). The server only checks size and first bytes.
- **Characters are grapheme clusters** (`auth::characters`, `lib/names.ts`), so a
  flag or a family emoji is one. The browser's `maxlength` counts UTF-16 units,
  so `nameLimit` moves it with what is typed. A name past 256 code points is
  refused even under 20 characters, because sign-in would refuse it afterwards.
- **A name is checked and written under `state.names`.** The column is only
  unique as typed, and a case-insensitive index could fail on a database that
  already holds such a pair. One process makes one lock enough.
- **Sign-in's limit is not a username limit.** `USERNAME_MAX` (20) is for a name
  being chosen. Sign-in only refuses names past `SIGN_IN_NAME_MAX` (256), so an
  account older than the limit always gets in. Raise that one if an instance
  ever has a longer name.
- **Reading is `chapter_read`**, one row per chapter on the day it was read,
  owned by the library entry (it leaves with it, and undo and backups carry
  it). How far someone is stays `library_entry.progress`; only reads with a
  day are rows, so a first AniList sync adds none. The diary and Activity
  union it with `watch_event`; Stats reads it in its own Manga half
  (`routes/stats.rs::manga`), and films-and-shows figures never see it.
- **The trailer behind a title** (Settings → Watching → Play trailers) needs
  three things, each learned the hard way: a referrer
  (`referrerpolicy="strict-origin-when-cross-origin"`), or YouTube refuses
  most trailers; no `playlist` loop, which brings next and previous buttons
  (Frog loops by `seekTo` on state 0 instead); and a player that is never
  hidden, because a browser pauses a muted video nobody can see and YouTube
  flashes its button on resuming. So it plays under the title's picture,
  which lifts 3 s after the player says it is playing (`AMBIENT_SETTLE`), and
  starts 12 s in, past the studio logos (`AMBIENT_FROM`).
- **Manga search is ranked here** (`media::rank_manga`): AniList answers by
  synonym and description too, so only what a title answers is kept,
  synonyms after titles, light novels left out.
- **New Home rows** reach saved layouts once, through `homeRowsSeen` in
  `lib/settings.svelte.ts`. Add the id to `HOME_ROWS` and to the default list.

- **Characters come from what Frog has seen.** Search finds anime characters
  through AniList, and everyone else from the casts of titles somebody on this
  Frog has opened. TMDB has no character search.
- **A show marked Watching by hand, with nothing ticked,** stays in Continue
  watching as "Start S01E01", on purpose. So does one a tracker other than
  Netflix calls `watching`; `docs/IMPORTS.md` takes a tracker at its word.
- **Imports keep the day, not the time.** Every reader sets a watch to 20:00
  UTC on its day, so Trakt's exact time is not kept. Recorded in
  `docs/IMPORTS.md` and pinned by `tests/parsers.test.mjs`.
- **AniList-only titles lack what AniList cannot give**, accepted: no title
  logo for the hero, no episode synopses, streaming links for every country,
  AniList voice actors and studios as AniList people in Stats, and no episode
  names past AniList's episode count. Everything else works as for any title
  (`tests/anilist.rs`, `verify-leapfrog`).
- **Server messages are English only**, like the interface.
- **The browser extension** (`extension/`, `docs/EXTENSION.md`) knows no
  site. Each frame reports its own address, its iframes' addresses, JSON-LD,
  meta tags, the media session, its title and headings; `lib/signals.ts`
  turns those into clues and picks ids before text. An id read from an
  address shape alone (`/tv/123/1/2`) is weak: exact only once Frog's name for
  it agrees with the page, or a site rule says so. A confirmed or corrected
  match teaches a rule for that domain (`lib/rules.ts`); no domain is written
  in the code. Only resolve's fields leave the browser, never an address.
  The token reaches `/api/ext` and nothing else (`tokens::TokenAuth`, and
  `Auth` answers a bearer token with 403); a log is the title page's tick
  (`release::after_watch`), idempotent per token for six hours
  (`extension_log`). Releases attach it under fixed names (`frog-extension-firefox.xpi`,
  `frog-extension-chrome.zip`, `updates.json`), so `releases/latest/download`
  is always the newest: Settings → Connections links there and Firefox's
  `update_url` reads it. Releases sign the Firefox build as unlisted in
  `release-please.yml`, since a tag made with `GITHUB_TOKEN` starts no
  workflow. Playwright's own Chromium will not start on the owner's Windows
  machine ("side-by-side configuration"); `E2E_CHANNEL=msedge` runs the suite
  in Edge there, and CI uses Chromium.

## Decided against

- **Jellyfin scrobbling, and the generic webhook it needed.** It only sees
  what plays through a Jellyfin server; the owners watch through streaming
  sites, so it would track nothing. The browser extension covers streaming
  sites instead, anime arrives through the AniList sync, and the rest is
  ticked by hand.
- **Plex.** Its webhooks need a paid Plex Pass.
- **Interface languages.** English only, decided 2026-10-26.
- **Moving Netflix shows left On hold.** A show an import left On hold with
  nothing ticked stays On hold: paused is the person's word everywhere in
  Frog, and nothing changes it behind their back. Decided 2026-11-06.
- **Oracle Pay As You Go.** Always Free only, whatever the console suggests
  when A1 capacity runs out.
- **Staging.** 954 MB will not carry a second instance.
  `install.sh --with-staging` sets one up if it ever moves to a bigger machine.
