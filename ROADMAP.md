# Roadmap

The working backlog. Claude reads this at the start of a session and updates
it when something ships. Top of **Next** is what to do next.

Current release: **0.3.0 "Tadpole"**. Next release: **0.4.0 "Lily Pad"**.
Codenames after that, in order: Ribbit, Leapfrog, Bullfrog (1.0).

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
