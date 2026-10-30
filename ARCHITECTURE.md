# Frog — architecture

A self-hosted cinema for a handful of invited people: one Rust binary, one
SQLite file, one installable web app. Everything in it is free to build, run
and host.

Deeper guides: [the catalog](docs/CATALOG-IDENTITY.md),
[AniList](docs/ANILIST.md), [imports and exports](docs/IMPORTS.md), and the
[deployment](docs/DEPLOY-ORACLE.md).

---

## Decisions taken up front

| Question | Answer | Why |
| --- | --- | --- |
| Users | Several, invitation only | Every personal table carries a `user_id` with a foreign key, and `backend/tests/scoping.rs` fails the build if a query forgets it. Isolation is enforced per request, not hidden in the interface. |
| Profiles | Opt in, instance only | Off until an account turns it on, and visible only to other accounts on the same instance. Every route except `/healthz` and the auth endpoints sits behind a session. |
| Imports | Letterboxd, Trakt, IMDb, TV Time, MyAnimeList, Netflix, Frog backups | All of them reduce to one intermediate shape, so a new source is a parser rather than a redesign. AniList syncs both ways on its own. |
| Scrobbling | Not built | It only sees what plays through a media server the owners do not use. Anime arrives through the AniList sync; the rest is ticked by hand, on purpose. Plex is out regardless: its webhooks need Plex Pass. |
| Hosting | Oracle Cloud Always Free, Docker, Caddy | A public site on a free instance, HTTPS issued automatically, updating itself from the newest release image. See [docs/DEPLOY-ORACLE.md](docs/DEPLOY-ORACLE.md). It still runs on a laptop for development. |

---

## Shape of the system

```
              Any browser, or the installed PWA
                              │  HTTPS
                              ▼
                    Caddy  —  TLS, security headers, CSP
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
│  background worker (tokio tasks)                         │
│   ├── every 30 min: expire sessions, prune the TMDB      │
│   │   cache, trim the picture cache to its budget,       │
│   │   refresh airing shows, announce episodes            │
│   ├── every 30 s: sync AniList accounts that are due     │
│   ├── every 2 min: backfill credits for older titles     │
│   └── every 5 min: tell administrators about failed      │
│       server jobs (ops.jsonl)                            │
│                                                          │
│  SQLite (WAL)  ──  /data/frog.db                         │
│   accounts · libraries · episodes · watches · caches     │
└──────────────────────────────────────────────────────────┘
          │  HTTPS, cached aggressively         │
          ▼                                     ▼
  api.themoviedb.org                   graphql.anilist.co
```

### Why it is one process

A separate job runner would mean a second container, a queue and a way to
talk between them. For a tracker this size every job is a `tokio::spawn` and
a sleep. The worker is its own module (`jobs/`), so it can move out without
touching the routes if it ever has to.

### Why SQLite and not Postgres

The working set is a few people's watch histories. Postgres would add a
container, a connection string to get wrong and a backup story. SQLite in
WAL mode handles the worker and the HTTP handlers concurrently, and a backup
is one file.

---

## Backend

`backend/`, Rust 2021, axum 0.8 + sqlx 0.8 + tokio.

The layers, from the outside in. A layer may use the ones below it, never
the ones above.

| Layer | Modules | Does |
| --- | --- | --- |
| HTTP | `routes/*` | Parse and check the request, name the account (`Auth`, `AdminAuth`), call the layer below, shape the answer. |
| Domain | `catalog`, `transfer`, `anilist`, `release`, `push`, `jobs` | Decisions and workflows: which title a provider id names, what an import means, who wins a sync, when an episode is out, what a show's status is. |
| Persistence | `catalog`, `store`, `anilist::records`, `images`, `db` | Reads and writes. `catalog` owns the shared titles, episodes and provider ids; `store` owns one account's library; each domain may own its own tables (`anilist::records` owns `anilist_*`, `images` owns `image_cache`). |
| Providers | `tmdb`, `anilist::client`, `anilist::media` | Talking to other services, cached; nothing else calls them directly. `catalog::ensure` is how a provider's answer becomes a title. |
| Shared | `auth`, `config`, `error`, `state`, `text`, `palette`, `totp`, `ops` | Small and specific; no catch-all helpers module. |

Simple, single-area reads still live in their route module (most of
`routes/stats.rs`, `routes/timeline.rs`, `routes/shelves.rs`,
`routes/profiles.rs`, `routes/two_factor.rs`). A query moves out when a
second caller needs it or when it carries a decision. ROADMAP lists what is
left. See CLAUDE.md for the rule.

| Module | Responsibility |
| --- | --- |
| `config` | Reads the environment once. A missing TMDB key is a warning, not a crash, so you can still reach the UI and be told what is wrong. |
| `db` | Pool setup, WAL, busy timeout, migrations embedded in the binary, the line-ending fix that lets a database move between Windows and the server, and the one table rebuild SQLite cannot do inside a migration (`rebuild_title`). Sets the schema generation (`SCHEMA_GENERATION`, SQLite's `user_version`) that tells `deploy/update.sh` when the previous image can no longer run on the database. |
| `auth` | Argon2 password hashing, session tokens, roles, the `Auth` and `AdminAuth` extractors. |
| `tmdb` | HTTP client, response cache, the curated mapping from TMDB's JSON to what a screen draws. |
| `catalog` | Frog's titles and their provider ids (`title_external`): finding a title by any provider's id, making or refreshing one from a provider, episodes, credits, poster colour. |
| `store` | One account's library: entries, progress, "up next", emptying it, and moving one account's data between titles (`move_title_data`). |
| `images` | The picture cache in SQLite, and its budget. |
| `release` | When an episode counts as out, and the status a show should have (`reconcile`). |
| `transfer` | Imports (matching, applying) and exports. |
| `anilist` | AniList sign-in and two-way sync. |
| `push` | Web Push: subscriptions and sending. |
| `jobs` | The background worker. |
| `ops` | Reads what the deploy scripts recorded in `ops.jsonl`. |
| `palette` | Pulls an accent colour out of a poster. |
| `text` | Case and accent folding for search. |
| `routes` | One module per area. |

### Sessions and accounts

A session token is 32 random bytes. Only its SHA-256 is stored, so a stolen
database file does not hand over live sessions. The cookie is `HttpOnly`,
`SameSite=Lax`, and `Secure` when `FROG_SECURE_COOKIES=1` (set that whenever
something in front terminates TLS, which Caddy does on the deployed site).

The first account is the owner, made from `FROG_ADMIN_USERNAME` /
`FROG_ADMIN_PASSWORD` at boot or from the setup screen, which refuses once
any account exists. Everyone after that joins through a single-use, expiring
invitation an administrator makes. There is no open sign-up.

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
Rows more than 30 days past their expiry are pruned by the worker.

### Images never hotlink

`/img/{size}/{path}` fetches from `image.tmdb.org` (and, for AniList-only
titles, AniList's and Crunchyroll's image hosts) once and stores the bytes in
SQLite (`crate::images`). The service worker can cache posters for offline,
canvas reads are same-origin, and a second viewing costs the provider
nothing. Sizes are allowlisted so the endpoint cannot become an open image
relay.

The cache is held to a budget, `FROG_IMAGE_CACHE_MB` (512 by default). Every
worker tick drops pictures nobody has looked at for 180 days, then, while it
is over budget, the least recently served until it is under nine tenths of
it. A picture is marked as served at most once a day, and a dropped one is
simply fetched again the next time a page wants it. Only `image_cache` rows
are ever deleted; SQLite reuses the freed pages, so the file does not
shrink. A library of a few hundred titles takes roughly 0.5 MB a title
across poster, backdrop and stills.

### Poster-driven colour

When a title is first fetched, the backend pulls its `w92` poster, quantises it
into 4×4×4 RGB buckets, and scores each bucket by population **and** saturation,
ignoring near-black, near-white and grey. Averaging a poster always produces
mud; this picks the colour a person would actually name. The three resulting
hexes are stored on the row, so the page paints tinted on first render with no
flash and no client-side work.

### Data model

```
app_user ── session, recovery_code, invite, app_setting, user_image,
            push_subscription, push_sent

title (shared catalog; id is Frog's own) ──┬── episode ──┬── episode_state  (notes, per account)
  kind, metadata, merged_into              │             └── watch_event    (the diary, per account)
                                           ├── title_external (TMDB / AniList / IMDb ids; one primary)
                                           ├── library_entry (per account: status, rating, …)
                                           ├── season_rating (per account, per season or part)
                                           ├── shelf_item ── shelf (per account)
                                           └── title_person (credits, shared)

anilist_link · anilist_oauth · anilist_entry      per account
anime_map · anilist_media · anilist_chain         shared AniList knowledge
tmdb_cache · image_cache                          shared caches
title_merge                                       per account: every move between titles
```

Decisions worth naming:

- **Frog owns its catalog.** A title is Frog's row and Frog's id; TMDB,
  AniList and IMDb ids are kept beside it in `title_external`, one provider
  id to one title, one primary provider per title. Personal rows point at
  Frog's ids only. Pages and the API address titles as `/t/{id}`; a
  provider's address (`/title/tv/1399`, `/title/anilist/21`) opens a title
  and moves to its own. [docs/CATALOG-IDENTITY.md](docs/CATALOG-IDENTITY.md)
  has the rules for merges and providers that lose a title.
- **Watches and opinions are separate.** `watch_event` is append-only
  history; notes live in `episode_state`, ratings in `library_entry` (films
  and whole shows) and `season_rating` (a season, or an AniList part of
  one). Rewatching does not clobber an opinion. Episodes are not rated;
  older episode ratings stay stored but unused (see ROADMAP).
- **Ratings are always 0–100 internally.** The score format (100 point, 10
  point decimal, 10 point, 5 stars, 3 smileys) is a display concern only, so
  switching it never rewrites or rounds data.
- **Watches carry a source** (`manual`, `import`, `anilist`). Only a
  source's own watches are ever taken back by it.

Specials (season 0) never count towards completion. Marking a season watched
only adds rows for episodes with no watch yet, so it cannot silently inflate
your rewatch counts.

---

## Frontend

`frontend/`, SvelteKit 3 + Svelte 5 runes, built with `adapter-static` as a
pure SPA. The Rust server owns routing and falls back to `index.html`.

- **Links go by Frog's id.** `lib/titles.ts` (`titleHref`, `titleKey`) is
  the one place that decides a title's address: `/t/{id}` once Frog has it,
  the provider's address for a search result it does not.
- **Routes are pages, components are parts.** A route composes; a part with
  its own state and purpose is a component in `src/lib/components/`
  (Settings' groups are in `src/lib/components/settings/`). Shared logic is
  a module in `src/lib/`.
- **Design tokens in `app.css`.** Every colour, radius and spacing step is a
  custom property. Light, dark and AMOLED are three token sets. A title page
  overrides `--accent` locally from its poster, and the theme editor writes
  custom properties at runtime with no component changes.
- **Fonts and icons are bundled.** Fontsource packages and Lucide paths
  inlined in `Icon.svelte`; nothing is fetched from a CDN at runtime.
- **View Transitions.** Cross-route fades plus a real shared-element morph: a
  poster in the library grid carries `view-transition-name`, and the same name
  lands on the hero poster of the title page.
- **Optimistic episode ticks.** The checkmark is the most-used control in the
  app, so it updates immediately and rolls back if the request fails.
- **Service worker.** App shell and hashed assets cache-first; posters
  cache-first; API reads network-first with a cached fallback; API writes
  network-only. `/api/auth/*` is never cached — a stale "authenticated" would
  show library chrome to someone signed out.
- **Imports are parsed here**, not on the server; see
  [docs/IMPORTS.md](docs/IMPORTS.md).

---

## How it was built

What each release added is in [RELEASE-NOTES.md](RELEASE-NOTES.md); what is
left is in [ROADMAP.md](ROADMAP.md).

| | |
| --- | --- |
| **0.1 Spawn** | Accounts, TMDB search, tracking, episodes, the library wall, title and person pages, the PWA, one Docker image, CI. |
| **0.2 Ripple** | The mascot and the Pond theme, Home split from Library, the command palette, the theme editor, server-side settings. |
| **0.3 Tadpole** | Imports from four trackers, export, the calendar, the diary, About. |
| **0.4 Lily Pad** | Full stats and the year in review, Web Push, shelves, recommendations, anime imports, two-factor. |
| **0.5 Ribbit** | Several accounts with enforced isolation, invitations, profiles, administration, AniList sync — and the whole thing deployed. |
| **0.6 Leapfrog** | AniList both ways through its own sign-in, placed by id; Settings in groups; administrators delete accounts; fuller profiles. |
| **0.7 Treefrog** | One search for titles, people, characters, studios and members; AniList placed by id, name and prequel chain; trailers; streaming links. |

---

## What is deliberately not here

- **No AI or LLM anywhere.** Recommendations are weighted sums over TMDB's
  own graph.
- **No scrobbling at all.** It only sees what plays through a media server;
  anime arrives through the AniList sync and the rest is ticked by hand.
- **No paid APIs, analytics, error reporting or font services.** TMDB's free
  key is the only credential; AniList's sign-in is free too.
- **No paid host.** It runs on a free Oracle instance, and on your own
  machine just as well. See [docs/DEPLOY-ORACLE.md](docs/DEPLOY-ORACLE.md).

## Ratings

`backend/src/ratings.rs` keeps what other sites score a title in
`title_rating`, on Frog's own title and matched only by provider id: TMDB's,
AniList's through `title_external` or `anime_map`, and MyAnimeList's through
AniList. MDBList carries IMDb, Rotten Tomatoes, Metacritic and the rest; its
key stays on the server. The worker refreshes what somebody tracks, a few
titles at a time, and `/api/t/{id}/scores` gives a title page its first look.
A title page shows four of them as percentages: AniList, IMDb, Rotten
Tomatoes and Metacritic. While MDBList has nothing for a title (its daily
limit, or no match yet), TMDB's own score stands in beside AniList's.

## Logos

A title's logo comes from TMDB. Where TMDB has none, `backend/src/fanart.rs`
looks further, in this order:

1. fanart.tv, for anything TMDB knows: films by TMDB's id, shows by TheTVDB's
   (from TMDB's external ids). An anime Frog keeps from AniList uses the TMDB
   entry `anime_map` gives it, TMDB's own logo first.
2. The show it belongs to. AniList names a special's parent and a season's
   prequel; the chain is followed up to five steps back until one has a logo
   in Frog, on TMDB or on fanart.tv. That is how a third season nobody has
   mapped yet still shows the show's logo.

It only fills an empty `title.logo_path`: fanart.tv's files are stored as
`/fanart/<file>` and served through `images` like every other picture, a
parent's TMDB logo by TMDB's own path. TMDB wins: `catalog` writes
`COALESCE(new, old)`. `title_logo_check` (shared) says when a title may be
asked again, a month after nothing was found. The worker fills tracked titles
a batch every ten minutes; a title page opened for the first time waits a
moment for its own. Without `FANART_API_KEY` step 1 is skipped and the rest
still runs.
