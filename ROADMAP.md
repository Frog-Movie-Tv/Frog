# Roadmap

The working backlog. Claude reads this at the start of a session and updates
it when something ships. Top of **Next** is what to do next.

Current release: **0.4.0 "Lily Pad"** (2026-10-26). Next release:
**0.5.0 "Ribbit"**. Codenames after that, in order: Leapfrog, Bullfrog (1.0).

## In progress — the multi-user brief, handed over 2026-10-26

The owners' brief, in four parts, to be done **in order**. Nothing deploys
until Part 4's checklist passes. Part 4 supersedes the Oracle item that used to
sit at the top of **Next**.

Frog now has two owners: Mihai Constantin (GitHub: MihCons) and Zjef Schaeken
(GitHub: zschaeken). The repository moved to the **Frog-Movie-Tv**
organisation: `https://github.com/Frog-Movie-Tv/Frog`.

**Done in this session** (collecting under `## Upcoming — 0.4.1 Lily Pad —` in
RELEASE-NOTES.md; the owners asked for everything new to be named 0.4.1):

- Remote and every in-repo reference moved to the new organisation, including
  the GHCR image name in `docs/INSTALL.md` and the update check in Settings.
- Both owners credited in the README, in a "Made by" section on About, and in
  LICENSE.
- README covers the 0.4.0 features (full stats, year in review, new-episode
  alerts, shelves, AniList and MyAnimeList imports, two-factor sign-in).
- About showed the wrong release codename; it now reads Lily Pad, and the
  `/release` skill bumps it from now on.
- **Part 4 research done and cited in `docs/DEPLOY-ORACLE.md`.** Confirmed:
  Always Free A1 is now 2 OCPU / 12 GB (down from 4 / 24); idle reclamation
  needs CPU **and** network **and** memory all under 20% over 7 days, so the
  keepalive should hold ~2.4 GB of memory rather than burn CPU; the Ubuntu
  image's own iptables reject must be handled in `install.sh`; two plain
  hostnames need only HTTP-01, so stock Caddy with no DuckDNS plugin;
  GHCR container storage is free even for private images; release-please is
  current (17.11.2) and the plan stands.

**Blocked on a decision from the owners** — see "What the research changes" in
`docs/DEPLOY-ORACLE.md`:

1. **The repository is private.** Free arm64 runners are public-repository
   only, and private repos get 2,000 Actions minutes a month. Multi-arch
   builds under QEMU plus CI on every PR plus E2E will not fit, and the only
   fix is money, which the brief rules out. Making the repository public (it
   is MIT already) buys unlimited standard minutes and native arm64 runners.
2. **GitHub settings are still to configure.** `gh` 2.103 is installed on
   the laptop and signed in as zschaeken (scopes repo, workflow, read:org,
   gist), so branch protection, required reviews, CODEOWNERS enforcement,
   labels and the Projects board can now be set from here. The GitHub
   Releases for v0.2.0, v0.3.0 and v0.4.0 exist (notes from
   RELEASE-NOTES.md; v0.4.0 is Latest).

**Part 1 — multi-user. Backend done; no interface for it yet.**

Done, with `cargo fmt`, `clippy -D warnings` and 67 tests green:

- **Migration `0008_multi_user.sql`.** Every personal table gained a NOT NULL
  `user_id` with a foreign key to `app_user` and `ON DELETE CASCADE`:
  library_entry, episode_state, watch_event, app_setting, shelf,
  push_subscription, push_sent, recovery_code, session. Shared: title,
  episode, title_person, tmdb_cache, image_cache and the VAPID keys. Tables
  were rebuilt (SQLite cannot add a NOT NULL foreign key or change a primary
  key in place), and shelf_item is parked in a constraint-free copy while
  shelf is rebuilt so its ON DELETE CASCADE does not wipe it.
  **Existing data was preserved, not deleted:** the one account that was there
  becomes the owner and keeps its library. Verified on a fresh database and on
  a copy of the owner's real one — schema scoped, `foreign_key_check` and
  `integrity_check` clean, no orphaned rows, every row count unchanged.
  The owner's database was still at migration 0004, so 0005–0007 applied in
  the same run; 0006 adds its own film watches by design.
- **`Auth` carries identity.** It was a unit struct that only answered "is
  there a session"; it now holds `user_id`, `username` and `role`, so a
  handler cannot reach data without saying whose it is. `AdminAuth` refuses
  anyone below administrator in the route table itself. Roles are ordered, so
  checks read `role >= Role::Admin`. Sessions belong to an account, and a
  disabled account loses access on its very next request rather than at its
  next sign-in.
- **Every query scoped**, about 200 sites across the store, 14 route modules,
  the push sender and the background worker. Sign-in now looks an account up
  by username instead of assuming row 1, with the same message whichever part
  was wrong.
- **Invites** (`routes/invites.rs`). Single use, expiring, only the hash
  stored. Claiming is the same statement that checks it, so two people
  redeeming one link cannot both get in; a failed account creation puts it
  back. Only the owner may invite an administrator. First run still makes the
  owner, and setup closes for good afterwards. No open sign-up.
- **Login throttling per account and per address.** Both buckets are checked,
  so guessing one password from everywhere locks that account while one
  address cannot work through every account. `X-Forwarded-For` names the
  caller (Caddy sits in front in Part 4), falling back to the socket address.
- **Isolation tests** through the HTTP surface, not the store: a second
  account sees nothing of the first on library, library stats, stats, home,
  diary, calendar, shelves, shelves by id, shelves for a title, export and
  settings; and cannot rename, fill or delete another account's shelf, redate
  their diary, or disturb their entries, ratings or progress. Plus role
  checks, invite single-use, disabled accounts and the throttle.
- **`tests/scoping.rs`** reads the source and fails if any new query touches a
  personal table without naming an owner. Genuinely shared statements are
  listed with their reason, so an exception is a visible decision. Checked
  that it fails when a `user_id` is removed, which matters with two people
  writing queries in parallel.

Left for Part 1:

1. **Per-account routes:** change password, active sessions with "sign out
   everywhere" (`destroy_sessions_for` already exists and can spare the
   current session), export my data, delete my account.
2. **Admin routes and page:** users list with disable and role changes,
   invites, storage used, version, last backup, update status.
3. **Optional profile page**, off by default per account (the
   `profile_public` column exists), visible only to signed-in accounts on the
   same instance, showing what they are watching, favourites and stats.
4. **The whole interface:** a join screen for an invite link, account
   switching, the admin page, per-account settings in the UI, and the
   what's-new seen state moving into per-account settings.
5. **Re-check for single-user assumptions** now that the backend is scoped:
   the `FROG_ADMIN_USERNAME`/`PASSWORD` bootstrap makes an owner, which is
   right, but backups, imports and the screenshot and verify scripts all still
   assume one account.

Nothing about accounts is in RELEASE-NOTES.md yet, on purpose: there is no
interface for it, so there is nothing a user can do with it.

**Part 2 — two developers at once.** Not started. Protect main (no direct
pushes, PRs only, CI green plus one approval from the other owner, admin
override for emergencies); conventional commits; release-please for versions
and RELEASE-NOTES.md so neither of us edits it by hand; timestamped migration
filenames plus a CI check that migrations apply on a fresh database and on a
copy of the previous version's; pre-commit hooks for rustfmt, clippy,
prettier, eslint and svelte-check, with CI running the same; CODEOWNERS, PR
and issue templates, labels, a Projects board (Backlog / In progress / Review
/ Done); CLAUDE.md in the repo root covering conventions; one-command local dev
setup with two seeded demo users, documented in `docs/CONTRIBUTING.md`;
staging deploys on every merge to main, production only on a tagged release.

**Part 3 — clean-up before deploying.** Not started. Docker is the real way
Frog runs, and the production image must be the one CI built and tested; label
or remove the local non-Docker run modes; delete dead code, unused
dependencies and leftover comments; all configuration through environment
variables with one documented `.env.example`; a consistent structure documented
in CLAUDE.md and CONTRIBUTING.md; a small multi-arch image with a non-root
user, a healthcheck and `/healthz`; backend, isolation and Playwright tests all
green in CI.

**Part 4 — Oracle Cloud.** Research done (above). Code not started. Always
Free Ampere A1 on Ubuntu in eu-amsterdam-1, a public site over DuckDNS with
Caddy and automatic HTTPS, production and staging as separate compose projects
with separate databases, pull-based updates by systemd timer (staging follows
`:main`, production the latest release tag) with a backup first and a
`/healthz` rollback, nightly age-encrypted backups to a private repo with
30-day retention and a tested restore, a keepalive, SSH keys only, fail2ban,
security headers, noindex, no public sign-up, and a `deploy/` folder with
compose files, Caddyfile, `install.sh`, `update.sh`, `backup.sh` and
`restore.sh`. Never suggest Pay As You Go. No Watchtower.

**On finishing:** open the app as two users side by side and show the
isolation, demonstrate the PR workflow on a test branch, and summarise what
changed.

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
