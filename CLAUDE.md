# Frog — notes for Claude

Frog is a self-hosted film and TV tracker for a handful of invited people.
One Rust binary serves the API and the built SvelteKit app; one SQLite file
holds everything.

It runs as a public site on an Oracle Cloud free instance, behind Caddy with
automatic HTTPS, and updates itself by pulling the newest release image.
`docs/DEPLOY-ORACLE.md` is the whole story. It still runs on a laptop for
development, which is what `scripts/dev.ps1` is for.

**Start every session by reading [ROADMAP.md](ROADMAP.md)** — it says what is
done, what is next, and what is known to be broken. Update it when you finish
something.

## Hard rules

- **Zero cost.** No paid APIs, no AI/LLM features, no services that need a card,
  no runtime CDNs (fonts and icons are bundled). TMDB's free key is the only
  credential. If something would cost money, say so and leave it out.
- **Invitation only.** Several accounts, no open sign-up: an administrator
  makes a single-use link and that is the only way in. Nothing is visible to
  anyone without an account.
- **Every personal row names its owner.** Library, ratings, episodes, diary,
  shelves, settings, sessions, two-factor and notifications all carry a
  `user_id`; only titles, episodes, credits, the TMDB and image caches and the
  push keys are shared. A query that reads or writes personal data without
  naming whose it is fails `backend/tests/scoping.rs`, and that is a real
  guard, not a style rule — it is what stops one account seeing another's.
- **Never print or commit `TMDB_API_KEY`.** It lives in `.env`, which is
  gitignored.
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
  src/release.rs    when an episode is out; what a show's status should be
  src/transfer/     imports (matching.rs, apply.rs) and exports (export.rs)
  src/anilist/      AniList: client, idmap, place, reconcile, library, local,
                    media, records, sync — docs/ANILIST.md has the map
  src/tmdb/         TMDB client, cache, and the JSON → view mapping
  src/auth.rs       sessions, roles, the Auth and AdminAuth extractors
  src/jobs/         background worker (sessions, caches, shows, AniList, ops)
  migrations/       timestamped .sql files, embedded at compile time
  tests/            api, anilist, catalog, images, transfer, scoping, migrations;
                    common/ has the fixtures (title_seed, title_id)
frontend/           SvelteKit 3 + Svelte 5 runes, adapter-static SPA, Vite 8
  src/lib/components/   Icon, Select, Rating, Poster, Rail, Empty, …
  src/lib/components/settings/   one component per Settings group
  src/lib/import/       client-side readers: common, parsers, anime, netflix
  tests/            node --test unit tests (parsers, Netflix)
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
  `anilist_*`). Library, title and episode reads that more than one area
  needs go in `store.rs`. A simple read only one route uses may stay in
  that route; move it out the moment a second caller wants it.
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

| What | Command |
| --- | --- |
| Run it (port 8080, two demo accounts) | `.\scripts\dev.ps1` (`-Fresh` to start over) |
| Run it without Docker, no seed | `.\scripts\run-local.ps1` (development only) |
| Backend checks | `cd backend && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| Frontend types | `cd frontend && npm run check` |
| Frontend unit tests | `cd frontend && npm test` — parsers and Netflix, no network |
| Frontend build | `cd frontend && npm run build` (also writes `static/changelog.json`, which is not tracked) |
| End-to-end | `cd frontend && npx playwright test` — needs `npm run build` and a built backend first |
| Page audit | `cd frontend && npm run audit` — every page, both viewports, console errors, service worker |
| Screenshots | `cd frontend && npm run screenshots` — needs the TMDB key, ~8 min, 1.5× (`-- --scale=2 --out=dir` to compare) |
| Icons | `cd frontend && npm run icons` — regenerates every icon from `scripts/mascots.mjs` |

Ports: the owner's instance uses 8080. E2E uses 8098/8099, audit 8095,
screenshots 8096, pwa-check 8094, and the verify scripts 8085–8086, 8088–8093 and 8097 —
each with its own throwaway database.

## Gotchas that have already cost time

- **SvelteKit 3 differs from SvelteKit 2.** `$lib` is gone — use `#lib/…`
  (mapped in `package.json` `imports` and `tsconfig.json` `paths`).
  `$app/environment` is `$app/env`. `$service-worker` is gone — use
  `$app/manifest` and `$app/env`. There is no `svelte.config.js`; config goes in
  `sveltekit({…})` in `vite.config.ts`.
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

- Comments explain *why*, not what. Match the density of the file you are in.
- Ratings are stored 0–100. Score formats are display only.
- Errors: database and internal faults show a generic message and log the
  detail; anything the user can act on keeps its own message.
- Icons come from `Icon.svelte` (Lucide paths, inlined). Never use a unicode
  glyph as an icon; add the path to `ICONS` instead.
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
  `chore` and `style` do not. On `0.x` a `feat` bumps the **patch**.
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

The `/release` skill is now only for the part a person has to do: writing the
`RELEASE-NOTES.md` entry in product voice with a pond codename, and checking
the screenshots. Commit or push only when the owner asks.

## Working style the owner expects

- Requests arrive as large batches. Work through them in the order given,
  finish each piece properly, and at the end say plainly what is done and what
  is not. Move anything unfinished into ROADMAP.md.
- Verify before claiming: run the tests and the audit, and say if something
  could not be checked (a real Android install, the Docker build).
