# Roadmap

The working backlog. Claude reads this at the start of a session and updates
it when something ships. Top of **Next** is what to do next.

Current release: **0.3.0 "Tadpole"**. Next release: **0.4.0 "Lily Pad"**.
Codenames after that, in order: Ribbit, Leapfrog, Bullfrog (1.0).

## In progress — 0.4.0 "Lily Pad", handed over 2026-10-26

The owner's 0.4.0 review (Part 1: bugs, findability, features, polish; Part 2:
Oracle Cloud deployment) is about half done. Everything below is **committed
and pushed to main, but not released** — the version is still 0.3.0 and the
`## Upcoming — Lily Pad —` section of CHANGELOG.md holds the draft notes.
Resume here.

**Done and verified in real Chrome** (scripts in `frontend/scripts/`):
- Offline mode: three real bugs fixed (auth status never cached, so offline sent
  you to sign-in; precache paths never matched; app shell not precached).
  `npm run pwa-check` passes: registers, installable per Chrome, every page
  works with the server stopped. The "unknown error when fetching the script"
  message the owner quoted is produced by the Claude app's built-in browser for
  any worker; real Chrome 155 registers fine.
- Command palette on one line: the real cause was a clash with the global
  `.field` class in app.css (column direction). Regression test added.
- Diary: bulk marking asks "When did you watch this?" (today / release dates /
  a date / don't know); undated watches (migration 0003) count but stay out of
  the diary; a day of one show is one grouped, expandable entry; "Change date"
  fixes existing entries (`POST /api/diary/redate`). "Mark all watched" on
  title pages. `node scripts/verify-diary.mjs` passes. 4 new backend tests.
- Season header fits 375px; skeleton rows while episodes load.
- Distinct status colours (`--status-*` per theme) in badges, stats, filters.
- Import links: Settings → Your data, Library ⋯ menu, empty library.
- Phone tab bar: 4 chosen tabs + More sheet. Settings → Navigation reorders,
  hides, and picks tabs. Data saver setting. Open on: Calendar, Diary.
- No native `<select>` left anywhere (0.3.0's notes overclaimed this).
- Hero: stronger scrim, accent label, readable on Pond Light; "Start watching"
  when nothing is watched. Calendar: titles in desktop cells, legend, day panel
  beside the grid. Mascot captions removed.
- `node scripts/verify-polish.mjs`: all checks pass, More sheet included.
- AniList (by username, live GraphQL) and MyAnimeList (XML / .gz) import.
  Tracker titles like "Season 2", "2nd Season", "Part 2", "II" map to a TMDB
  season and episode offset; seasons TMDB folds into one long season are
  carried on from the earlier seasons' counts (Jujutsu Kaisen S2 → S1 E25+);
  "The Final Season" asks for a number. Episodes are ticked undated, a planned
  later season no longer demotes a show you are watching. `node
  scripts/verify-anime.mjs` passes against real TMDB; 2 backend tests.

- Two-factor sign-in (TOTP, migration 0004): Settings → Security turns it
  on with a server-rendered QR and a typed key, confirms with a code, shows
  10 recovery codes (hashed, single use). Login asks for the code after the
  password; codes cannot be replayed; failures count toward the login rate
  limit. "Keep me signed in" is back on the login form (it was never sent,
  so every session died with the browser). Login shows the mascot.
  `node scripts/verify-2fa.mjs` passes in Chrome; RFC 6238 vectors and a full
  flow test in the backend.

**Left for Part 1**, in order:

1. **Full stats + shareable year in review** (was being designed when work
   stopped; no code written yet). Plan: new `routes/stats.rs` serving
   `GET /api/stats?year=YYYY` (year optional) with totals, years that have
   dated watches, minutes per month, genres (`json_each(title.genres_json)`),
   release decades, ratings in buckets of 10, longest and current streak of
   days with a dated watch (`watch_event.undated = 0`), most watched show, and
   top actors / directors (movies: crew job Director; TV: created_by) /
   studios (production_companies). Credits are not stored locally, so add
   migration 0005 with a `title_person (title_id, role, tmdb_id, name,
   profile_path, ord)` table plus `title.people_synced_at`, fill it in
   `store::ensure_title` from the detail JSON (it already appends credits),
   and backfill older titles in the worker (`jobs/mod.rs`, a few per tick).
   Year in review: client-side canvas PNG (1080×1920) using `/img/...` so the
   canvas is not tainted; share with `navigator.share({ files })`, else
   download. Keep the existing `/api/library/stats` for the library header.
2. **Web Push** when a followed episode airs, with on/off per show. VAPID keys
   generated on first run and stored in SQLite; needs HTTPS.
3. **Custom + smart shelves** (`shelf`, `shelf_item` tables already exist).
4. **"Because you watched X"** row on Home from TMDB recommendations.
5. **Interface language English / Nederlands.** Then: run
cargo fmt/clippy/test, npm check/build, playwright, audit, pwa-check; write the
0.4.0 changelog (only claim what was verified); ship with /release.

**Part 2 not started**: CI multi-arch GHCR images + releases, deploy/ folder
(compose + Caddy + install.sh + systemd update timer with backup/healthz
rollback + age-encrypted backups to a private repo + keepalive), CSP/HSTS,
update check vs newest image, docs/DEPLOY-ORACLE.md. Docker is not installed
on the owner's laptop, so the local arm64/QEMU test needs Docker installed, or
verify via GitHub Actions instead. The owner's full Part 2 brief: zero cost,
never suggest Oracle Pay As You Go, no Watchtower, secrets only in
`/opt/frog/.env` (chmod 600) on the server, and stop to ask only when a login
is needed (Oracle, DuckDNS, GitHub token).

**Verify scripts** (each starts its own throwaway server; build first):
`node scripts/verify-anime.mjs` (needs the TMDB key), `verify-2fa.mjs`,
`verify-diary.mjs`, `verify-polish.mjs`, and `npm run pwa-check`.

## Next

Ordered by what the owner asked for most recently. Take from the top.

1. **Replace the last native selects.** `TrackControls.svelte` (status) and
   `settings/+page.svelte` still use `<select>`; swap for `Select.svelte`.
2. **Full stats page.** Genres, decades, ratings distribution, watch time per
   month, top actors and directors. Needs new aggregate queries; follow the
   SQLite notes in CLAUDE.md.
3. **Year in review image.** A shareable Wrapped-style card rendered to a PNG
   with canvas, client-side.
4. **Web Push.** VAPID keys generated on first run and stored in SQLite, a
   subscribe button in Settings, and the worker sending a notification when a
   followed episode's air date arrives. Free; needs HTTPS (Tailscale Serve).
5. **Shelves.** Manual shelves with drag-and-drop ordering, and smart shelves
   built from rules (status, rating, genre, kind, progress — e.g. "unfinished
   shows rated 8+"). The `shelf` and `shelf_item` tables already exist.
6. **"Because you liked X" rows on Home.** TMDB recommendations for highly
   rated titles, weighted by rating, excluding anything already tracked.
7. **First-run onboarding.** Pick a theme and mascot, then import or search.
8. **Interface language: English / Nederlands.**
9. **Data saver** (smaller image sizes everywhere).
10. **Reorder and hide navigation tabs.**
11. **Pull-to-refresh on mobile.**
12. **AniList sync** (free GraphQL API, no key).
13. **Jellyfin scrobbling** via its free webhook plugin, plus a generic webhook
    endpoint. Plex webhooks need Plex Pass, so not Plex.
14. Smaller items: customisable keyboard shortcuts, rename and recolour
    statuses, muted trailer autoplay, per-shelf sort and layout defaults.

## Known issues

- The Docker image has never been built locally (no Docker on the laptop).
  CI builds it; check the latest run on GitHub Actions.
- GitHub Releases for v0.2.0 and v0.3.0 have not been created; the tags exist.
  Create them from the tags with the notes in CHANGELOG.md.
- The calendar screenshot comes out empty, because the demo titles have no
  upcoming episodes. Seed a currently airing show in `scripts/screenshots.mjs`.

## Shipped

- **0.3.0 Tadpole** — imports (Letterboxd, Trakt, IMDb, TV Time, Frog backup),
  JSON/CSV export, calendar, diary, About page, bundled typefaces, Lucide icon
  set, styled menus in the library, undo toasts, skeletons, empty states,
  caught-up state on Home, grouped airings, offline-mode reporting.
- **0.2.0 Ripple** — mascot and icons, Pond theme, Home split from Library,
  command palette, theme editor, server-side settings, privacy headers, login
  rate limiting, release notes page.
- **0.1.0 Spawn** — tracking, episodes, library, title and person pages,
  ratings, PWA, Docker, CI.
