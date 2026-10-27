# Frog — architecture

A self-hosted personal cinema: one Rust binary, one SQLite file, one installable
web app. Everything in it is free to build, run and host.

---

## Decisions taken up front

| Question | Answer | Why |
| --- | --- | --- |
| Users | Several, invitation only | Every personal table carries a `user_id` with a foreign key, and `backend/tests/scoping.rs` fails the build if a query forgets it. Isolation is enforced per request, not hidden in the interface. |
| Profiles | Opt in, instance only | Off until an account turns it on, and visible only to other accounts on the same instance. Every route except `/healthz` and the auth endpoints sits behind a session. |
| Imports | Letterboxd, Trakt, IMDb, TV Time, MyAnimeList, AniList | All of them reduce to one intermediate shape, so a new source is a parser rather than a redesign. AniList also syncs on its own. |
| Scrobbling | Not built | It only sees what plays through a media server the owners do not use. Anime arrives through the AniList sync; the rest is ticked by hand, on purpose. Plex is out regardless: its webhooks need Plex Pass. |
| Hosting | Oracle Cloud Always Free, Docker, Caddy | A public site on a free instance, HTTPS issued automatically, updating itself from the newest release image. See [docs/DEPLOY-ORACLE.md](docs/DEPLOY-ORACLE.md). It still runs on a laptop for development. |

---

## Shape of the system

```
              Any browser, or the installed PWA
                              │  HTTPS
                              ▼
                    Caddy  —  TLS, security headers
                              │
                              ▼
┌──────────────────────────────────────────────────────────┐
│  frog  —  one process, one container                     │
│                                                          │
│  axum router                                             │
│   ├── /api/*     JSON, session-cookie auth               │
│   ├── /img/*     image proxy + cache                     │
│   └── /*         static SvelteKit build, SPA fallback    │
│                                                          │
│  background worker (tokio task)                          │
│   ├── expire sessions                                    │
│   ├── prune stale TMDB cache                             │
│   └── refresh airing shows every 6h                      │
│                                                          │
│  SQLite (WAL)  ──  /data/frog.db                         │
│   accounts · library · episodes · watches · caches       │
└──────────────────────────────────────────────────────────┘
                              │  HTTPS, cached aggressively
                              ▼
                      api.themoviedb.org
```

### Why it is one process

A separate job runner would mean a second container, a queue and a way to talk
between them. For a tracker this size the whole job list is "re-check a few
hundred shows every six hours", which is a `tokio::spawn` and a sleep. The
worker is a module (`jobs/`), so if phase 4 ever outgrows that, it moves out
without touching the routes.

### Why SQLite and not Postgres

The entire working set is one person's watch history. Postgres would add a
container, a connection string to get wrong and a backup story. SQLite in WAL
mode handles the worker and the HTTP handlers concurrently, and "back up your
library" becomes "copy one file".

---

## Backend

`backend/`, Rust 2021, axum 0.8 + sqlx 0.8 + tokio.

| Module | Responsibility |
| --- | --- |
| `config` | Reads the environment once. A missing TMDB key is a warning, not a crash, so you can still reach the UI and be told what is wrong. |
| `db` | Pool setup, WAL, busy timeout, migrations embedded in the binary. |
| `auth` | Argon2 password hashing, session tokens, the `Auth` extractor that gates routes. |
| `tmdb` | HTTP client, response cache, the curated mapping from TMDB's JSON to what a screen draws. |
| `palette` | Pulls an accent colour out of a poster. |
| `store` | Every read and write of your own data. Routes stay thin. |
| `routes` | One module per area; parse, call `store`, serialise. |
| `jobs` | The background worker. |

### Sessions

A session token is 32 random bytes. Only its SHA-256 is stored, so a stolen
database file does not hand over live sessions. The cookie is `HttpOnly`,
`SameSite=Lax`, and `Secure` when `FROG_SECURE_COOKIES=1` (set that whenever
something in front terminates TLS, which Caddy does on the deployed site).

First run creates the account, either from `FROG_ADMIN_USERNAME` /
`FROG_ADMIN_PASSWORD` at boot or from the setup screen. The setup endpoint
refuses once an account exists, so the only window in which an exposed instance
could be claimed by someone else is the few seconds before you set it up.

### The TMDB cache is the offline story

Every TMDB request is keyed by path plus query and stored in `tmdb_cache` with
a TTL that matches how fast that data actually changes:

| Endpoint | Fresh for |
| --- | --- |
| search | 1 hour |
| movie details | 7 days |
| tv details, seasons | 12 hours |
| person | 7 days |
| trending | 6 hours |

When TMDB is unreachable or returns an error, Frog serves the stale row rather
than an error page. A title you have opened once keeps working on a train.

### Images never hotlink

`/img/{size}/{path}` fetches from `image.tmdb.org` once and stores the bytes in
SQLite. Three things fall out of that: the service worker can cache posters for
offline, canvas reads are same-origin, and a second viewing costs TMDB nothing.
Sizes are allowlisted so the endpoint cannot become an open image relay.

The cache grows with your library — roughly 50–80 KB per title at `w342`. A
thousand titles is well under 100 MB. Phase 2 adds an eviction job.

### Poster-driven colour

When a title is first fetched, the backend pulls its `w92` poster, quantises it
into 4×4×4 RGB buckets, and scores each bucket by population **and** saturation,
ignoring near-black, near-white and grey. Averaging a poster always produces
mud; this picks the colour a person would actually name. The three resulting
hexes are stored on the row, so the page paints tinted on first render with no
flash and no client-side work.

### Data model

```
app_user (exactly one row)        session
title ──┬── library_entry         (status, rating, favourite, notes, dates)
        ├── episode ──┬── episode_state   (rating, notes — survives rewatches)
        │             └── watch_event     (the diary; one row per viewing)
        └── shelf_item ── shelf    (tables exist now, used in phase 3)
tmdb_cache   image_cache   app_setting
```

Two decisions worth naming:

- **Watches and opinions are separate.** `watch_event` is append-only history;
  `episode_state` holds what you thought. Rewatching does not clobber a rating,
  and the diary can show "third time" without inventing one.
- **Ratings are always 0–100 internally.** The score format (100 point, 10
  point decimal, 10 point, 5 stars, 3 smileys) is a display concern only, so
  switching it never rewrites or rounds data.

Specials (season 0) never count towards completion. Marking a season watched
only adds rows for episodes with no watch yet, so it cannot silently inflate
your rewatch counts.

---

## Frontend

`frontend/`, SvelteKit 3 + Svelte 5 runes, built with `adapter-static` as a
pure SPA. The Rust server owns routing and falls back to `index.html`.

- **Design tokens in `app.css`.** Every colour, radius and spacing step is a
  custom property. Light, dark and AMOLED are three token sets. A title page
  overrides `--accent` locally from its poster. That is why phase 3's theme
  editor is a matter of writing custom properties at runtime and needs no
  component changes.
- **View Transitions.** Cross-route fades plus a real shared-element morph: a
  poster in the library grid carries `view-transition-name`, and the same name
  lands on the hero poster of the title page.
- **Optimistic episode ticks.** The checkmark is the most-used control in the
  app, so it updates immediately and rolls back if the request fails.
- **Service worker.** App shell and hashed assets cache-first; posters
  cache-first; API reads network-first with a cached fallback; API writes
  network-only. `/api/auth/*` is never cached — a stale "authenticated" would
  show library chrome to someone signed out.

---

## How it was built

Four rounds, all shipped. What each one added is in
[RELEASE-NOTES.md](RELEASE-NOTES.md); what is left is in
[ROADMAP.md](ROADMAP.md).

| | |
| --- | --- |
| **0.1 Spawn** | Accounts, TMDB search, tracking, episodes, the library wall, title and person pages, the PWA, one Docker image, CI. |
| **0.2 Ripple** | The mascot and the Pond theme, Home split from Library, the command palette, the theme editor, server-side settings. |
| **0.3 Tadpole** | Imports from four trackers, export, the calendar, the diary, About. |
| **0.4 Lily Pad** | Full stats and the year in review, Web Push, shelves, recommendations, anime imports, two-factor. |
| **0.5 Ribbit** | Several accounts with enforced isolation, invitations, profiles, administration, AniList sync — and the whole thing deployed. |

---

## What is deliberately not here

- **No AI or LLM anywhere.** Recommendations are weighted sums over TMDB's
  own graph.
- **No scrobbling at all.** It only sees what plays through a media server;
  anime arrives through the AniList sync and the rest is ticked by hand.
- **No paid APIs, analytics, error reporting or fonts.** The font stack is the
  system's. TMDB's free key is the only credential.
- **No paid host.** It runs on a free Oracle instance, and on your own
  machine just as well. See [docs/DEPLOY-ORACLE.md](docs/DEPLOY-ORACLE.md).
