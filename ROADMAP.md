# Roadmap

The working backlog. Claude reads this at the start of a session and updates
it when something ships. Top of **Next** is what to do next.

Current release: **0.4.0 "Lily Pad"** (2026-10-26). Next release:
**0.5.0 "Ribbit"**. Codenames after that, in order: Leapfrog, Bullfrog (1.0).

## Next

Ordered by what the owner asked for most recently. Take from the top.

1. **Oracle Cloud Always Free deployment** (Part 2 of the 0.4.0 brief; the
   owner postponed it on 2026-10-26 and will pick it up). CI builds
   multi-arch (amd64 + arm64) images to GHCR on tags, with GitHub Releases;
   a `deploy/` folder with compose + Caddy (HTTPS) + DuckDNS, `install.sh`,
   a systemd timer that updates to the newest image with a backup first and
   rolls back if `/api/health` fails; age-encrypted nightly backups pushed to
   a private repo; a keepalive so Oracle does not reclaim the idle VM;
   CSP/HSTS headers; an update check in Settings against the newest image;
   `docs/DEPLOY-ORACLE.md`. Set `TZ` (and make sure the image has tzdata) so
   `FROG_NOTIFY_HOUR` means local time on the server. Rules from the brief:
   zero cost, never suggest Oracle Pay As You Go, no Watchtower, secrets only
   in `/opt/frog/.env` (chmod 600) on the server, stop to ask only when a
   login is needed (Oracle, DuckDNS, GitHub token). Docker is not installed
   on the owner's laptop: verify images through GitHub Actions, or install
   Docker for a local arm64/QEMU test.
2. **First-run onboarding.** Pick a theme and mascot, then import or search.
3. **Pull-to-refresh on mobile.**
4. **AniList sync** (free GraphQL API, no key; import already exists).
5. **Jellyfin scrobbling** via its free webhook plugin, plus a generic webhook
   endpoint. Plex webhooks need Plex Pass, so not Plex.
6. Smaller items: customisable keyboard shortcuts, rename and recolour
   statuses, muted trailer autoplay, per-shelf layout defaults, a choice of
   notification hour in Settings (today it is `FROG_NOTIFY_HOUR`), shelves
   as Home rows.

Dropped by the owner (2026-10-26): interface language English / Nederlands.
The interface stays English only.

## Known issues

- New-episode notifications are verified through real FCM in Chrome, but not
  yet on a real Android phone, and tapping a notification is untested.
- Messages the server writes (validation errors, notification text) are
  English only, which is fine while the interface is English only.
- The Docker image has never been built locally (no Docker on the laptop).
  CI builds it; check the latest run on GitHub Actions.
- GitHub Releases for v0.2.0, v0.3.0 and v0.4.0 have not been created; the
  tags exist. Create them from the tags with the notes in CHANGELOG.md.
- The calendar screenshot comes out empty, because the demo titles have no
  upcoming episodes. Seed a currently airing show in `scripts/screenshots.mjs`.

## Notes for the next session

- **Migrations 0001–0007 have run on the owner's real database.** Never edit
  an applied migration (sqlx refuses to start on a changed checksum); add a
  new numbered file instead. That database recorded 0001–0004 with CRLF and
  0005–0007 with LF; `db::heal_line_endings` accepts checksums that differ
  only in line endings, so moving the database (to Oracle, say) or a fresh
  clone still starts. `.gitattributes` keeps migrations LF from now on.
- **Verify scripts** in `frontend/scripts/`, each with its own server and
  throwaway database (build the release binary first): `verify-stats.mjs`,
  `verify-push.mjs`, `verify-shelves.mjs`, `verify-because.mjs`,
  `verify-anime.mjs` (these need the TMDB key), `verify-2fa.mjs`,
  `verify-diary.mjs`, `verify-polish.mjs`, plus `npm run pwa-check` and
  `npm run audit`. Every script now stops its server and exits when done.
- **Web Push in tests:** headless Chrome discards displayed notifications and
  Playwright's `--disable-background-networking` cuts Chrome off from FCM.
  `verify-push.mjs` keeps background networking and records the worker's
  `showNotification` calls instead.
- **Stats credits** live in `title_person`, filled by `store::ensure_title`
  and backfilled by the worker 25 titles every 2 minutes.
- **New Home rows** reach saved layouts once through `homeRowsSeen` in
  `lib/settings.svelte.ts`; add the id to `HOME_ROWS` and the default list.

## Shipped

- **0.4.0 Lily Pad** — full stats with year filter and a shareable year in
  review card; Web Push for new episodes (own VAPID keys, per-show mute);
  hand-picked and smart shelves with drag to reorder; "Because you watched"
  rows on Home; AniList and MyAnimeList import; two-factor sign-in; "When did
  you watch this?" and a grouped diary with redating; phone tab bar with More
  and navigation settings; status colours; data saver; offline mode fixed;
  films marked watched now count in diary and stats.
- **0.3.0 Tadpole** — imports (Letterboxd, Trakt, IMDb, TV Time, Frog backup),
  JSON/CSV export, calendar, diary, About page, bundled typefaces, Lucide icon
  set, styled menus in the library, undo toasts, skeletons, empty states,
  caught-up state on Home, grouped airings, offline-mode reporting.
- **0.2.0 Ripple** — mascot and icons, Pond theme, Home split from Library,
  command palette, theme editor, server-side settings, privacy headers, login
  rate limiting, release notes page.
- **0.1.0 Spawn** — tracking, episodes, library, title and person pages,
  ratings, PWA, Docker, CI.
