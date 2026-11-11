# Frog — architecture

A self-hosted cinema for a handful of invited people: one Rust binary, one
SQLite file, one installable web app. Everything in it is free to build, run
and host.

Deeper guides: [the catalog](docs/CATALOG-IDENTITY.md),
[AniList](docs/ANILIST.md), [imports and exports](docs/IMPORTS.md), and the
[deployment](docs/DEPLOY-ORACLE.md).

---

## Decisions taken up front

| Question   | Answer                                                               | Why                                                                                                                                                                                                                                                                                                                                  |
| ---------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Users      | Several, invitation only                                             | Every personal table carries a `user_id` with a foreign key, and `backend/tests/scoping.rs` fails the build if a query forgets it. Isolation is enforced per request, not hidden in the interface.                                                                                                                                   |
| Profiles   | Opt in, instance only                                                | Off until an account turns it on, and visible only to other accounts on the same instance. On, others see the whole library and its ratings, their dated watching on Home and a taste match, never its notes (`store::SharedItem`, `activity::Summary`). Every route except `/healthz` and the auth endpoints sits behind a session. |
| Imports    | Letterboxd, Trakt, IMDb, TV Time, MyAnimeList, Netflix, Frog backups | All of them reduce to one intermediate shape, so a new source is a parser rather than a redesign. AniList is read on its own and never written to.                                                                                                                                                                                   |
| Scrobbling | Not built                                                            | It only sees what plays through a media server the owners do not use. Anime arrives through the AniList sync; the rest is ticked by hand, on purpose. Plex is out regardless: its webhooks need Plex Pass.                                                                                                                           |
| Hosting    | Oracle Cloud Always Free, Docker, Caddy                              | A public site on a free instance, HTTPS issued automatically, updating itself from the newest release image. See [docs/DEPLOY-ORACLE.md](docs/DEPLOY-ORACLE.md). It still runs on a laptop for development.                                                                                                                          |

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

| Layer       | Modules                                                                                                                                                                                                           | Does                                                                                                                                                                                                                      |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HTTP        | `routes/*`                                                                                                                                                                                                        | Parse and check the request, name the account (`Auth`, `AdminAuth`), call the layer below, shape the answer.                                                                                                              |
| Domain      | `catalog`, `transfer`, `anilist`, `release`, `airtime`, `airing`, `announce`, `reset`, `activity`, `taste`, `franchise`, `bulk`, `push`, `jobs`, `home`, `stats`, `shelves`, `timeline`, `profiles`, `two_factor` | Decisions and workflows: which title a provider id names, what an import means, who wins a sync, when an episode is out, what a show's status is, what to tell whom and when.                                             |
| Persistence | `catalog`, `store`, `anilist::records`, `images`, `db`, `account`, `admin`, `invites`, `prefs`, `studios`                                                                                                         | Reads and writes. `catalog` owns the shared titles, episodes and provider ids; `store` owns one account's library; each domain may own its own tables (`anilist::records` owns `anilist_*`, `images` owns `image_cache`). |
| Providers   | `tmdb`, `tvmaze`, `anilist::client`, `anilist::media`                                                                                                                                                             | Talking to other services, cached; nothing else calls them directly. `catalog::ensure` is how a provider's answer becomes a title.                                                                                        |
| Shared      | `auth`, `config`, `error`, `state`, `text`, `palette`, `throttle`, `totp`, `ops`                                                                                                                                  | Small and specific; no catch-all helpers module.                                                                                                                                                                          |

No route holds SQL. Each area's statements live in the module below it
(`account`, `admin`, `home`, `invites`, `prefs`, `profiles`, `shelves`,
`stats`, `studios`, `timeline`, `two_factor`), or in `store` and `catalog`
when more than one area reads them. See CLAUDE.md for the rule.

| Module                                                                                                                                                                                                                                                                                                                                      | Responsibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config`                                                                                                                                                                                                                                                                                                                                    | Reads the environment once. A missing TMDB key is a warning, not a crash, so you can still reach the UI and be told what is wrong.                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `db`                                                                                                                                                                                                                                                                                                                                        | Pool setup, WAL, busy timeout, migrations embedded in the binary, the line-ending fix that lets a database move between Windows and the server, and the one table rebuild SQLite cannot do inside a migration (`rebuild_title`). Sets the schema generation (`SCHEMA_GENERATION`, SQLite's `user_version`) that tells `deploy/update.sh` when the previous image can no longer run on the database.                                                                                                                                                                                                 |
| `auth`                                                                                                                                                                                                                                                                                                                                      | Argon2 password hashing, session tokens, roles, the `Auth` and `AdminAuth` extractors.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `tmdb`                                                                                                                                                                                                                                                                                                                                      | HTTP client, response cache, the curated mapping from TMDB's JSON to what a screen draws.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `catalog`                                                                                                                                                                                                                                                                                                                                   | Frog's titles and their provider ids (`title_external`): finding a title by any provider's id, making or refreshing one from a provider, episodes, credits, poster colour.                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `store`                                                                                                                                                                                                                                                                                                                                     | One account's library: entries, the filtered list and what its filters offer (`LibraryFilter`), progress, "up next", a title's history (`history`: started, finished, rewatches), the days another place knows (`set_known_dates`, and `replace_known_dates`                                                                                                                                                                                                                                                                                                                                        |
| for AniList's, which win), who else has a title (`others_with`), emptying it, and moving one account's data between titles (`move_title_data`), or every account's at once when a copy is folded into its title (`merge_copy`). Removing a title takes its watches and episode notes with it, held in `EntryRecord` so undo puts them back. |
| `images`                                                                                                                                                                                                                                                                                                                                    | The picture cache in SQLite, and its budget.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `bulk`                                                                                                                                                                                                                                                                                                                                      | One action on many titles of a library, one request each, and the marking of aired episodes it shares with "Mark all watched" (`tick_aired`, `When`). Removing, changing a status or favouriting answers with what the titles held (`store::EntryRecord`), which is what undo sends back.                                                                                                                                                                                                                                                                                                           |
| `activity`                                                                                                                                                                                                                                                                                                                                  | What everyone has been watching, the viewer included, for Home's Activity row and the Activity page. `summarise` adds up one person's watches on plain values, a card per title; `everything` pages through all of it, a title a day per card. Only the viewer (`store::myself`) and the accounts `store::shown_to` names, only dated watches of titles still in that library, and never notes.                                                                                                                                                                                                     |
| `taste`                                                                                                                                                                                                                                                                                                                                     | How alike two people rate things, and two libraries side by side. `score` and `compare` are the decisions on plain values; `between` and `library_of` are the reads.                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `franchise`                                                                                                                                                                                                                                                                                                                                 | A franchise across films and shows: TMDB collections hold only films, so a show joins one by name (`franchise_of`, `belongs`), and a show page finds the collection named after it.                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `release`                                                                                                                                                                                                                                                                                                                                   | When an episode counts as out, which of a day's episodes the calendar lists (`calendar_keeps`: a special airing beside a regular episode is its bonus, not a second entry), the status a show should have (`reconcile`), and whether Continue watching has anything to say about it (`continues`). For a title followed as a count, a manga's chapters, `set_count` moves the count and `counted_status` the status.                                                                                                                                                                                |
| `transfer`                                                                                                                                                                                                                                                                                                                                  | Imports (matching, applying) and exports.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `anilist`                                                                                                                                                                                                                                                                                                                                   | AniList sign-in, and reading each account's list into its library. Read only.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `tvmaze`                                                                                                                                                                                                                                                                                                                                    | TVmaze's public API, no key: a show by TMDB's external ids (`/lookup/shows`), and its episodes' season, number, day and `airstamp`. An empty `airtime` means no time, whatever the stand-in `airstamp` says. Spaced to stay under 20 calls in 10 seconds; a 429 waits.                                                                                                                                                                                                                                                                                                                              |
| `airtime`                                                                                                                                                                                                                                                                                                                                   | The moment one of TMDB's episodes airs, nothing else: AniList's `airingSchedule` for anime (Animation from Japan), matched by the day in Japan (±1) through `anime_map`, else by number when only one airing answers; TVmaze's `airstamp` for the rest, by season and number when the days agree, else by day. Neither: no time, and pages say "time TBA". `airtime_show` keeps how each TMDB show is found, for good. `refresh_due` works every listed and tracked show out again every six hours and writes `episode.air_at` and `airing_episode.air_at`; TMDB's `air_date` is never written.     |
| `airing`                                                                                                                                                                                                                                                                                                                                    | The Airing page. Once a day `refresh_list` reads TMDB's `/discover/tv` for every show with an episode from last week to four weeks ahead, uncached (`Tmdb::fetch`), and keeps the shows (`airing_show`) and those episodes (`airing_episode`); and `/discover/movie` for films released in `FROG_REGION` then, in cinemas or digital (`airing_film`, `film_release`). `day` reads one day for one viewer: an episode falls on its moment's day in the viewer's zone, else TMDB's day; one entry per show or film, most popular first, a page at a time, narrowed to shows, anime or films (`Kind`). |
| `announce`                                                                                                                                                                                                                                                                                                                                  | Notifications: what is due for one account now (`due`), in its time zone (`app_setting` `time_zone`, reported by the browser), and what the notification says (`Announcement::payload`). An episode goes out at its `air_at`, else at midnight on its day; a film on its release day. Once each (`push_sent`, `push_sent_title`).                                                                                                                                                                                                                                                                   |
| `push`                                                                                                                                                                                                                                                                                                                                      | Web Push: subscriptions and sending.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `jobs`                                                                                                                                                                                                                                                                                                                                      | The background worker.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `ops`                                                                                                                                                                                                                                                                                                                                       | Reads what the deploy scripts recorded in `ops.jsonl`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `palette`                                                                                                                                                                                                                                                                                                                                   | Pulls an accent colour out of a poster.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `text`                                                                                                                                                                                                                                                                                                                                      | Case and accent folding for search.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `reset`                                                                                                                                                                                                                                                                                                                                     | Password reset links: single-use, a day long, one per account, stored as a hash. A change to the account's password or role withdraws its link.                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `throttle`                                                                                                                                                                                                                                                                                                                                  | Counts failed attempts in memory. Keys are fixed-size hashes, a refused request stores nothing, the worker sweeps what has expired, and the map has a ceiling.                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `account`                                                                                                                                                                                                                                                                                                                                   | One account's own settings: name, password, devices, profile fields and uploaded pictures.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `admin`                                                                                                                                                                                                                                                                                                                                     | Administration's reads and writes: every account, roles, switching off, deleting, and how big the instance is. Who may act on whom stays in `routes::admin`.                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `invites`                                                                                                                                                                                                                                                                                                                                   | Invite links by the hash of their code: made, listed, withdrawn, and spent once (`claim` checks and spends in one statement).                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `prefs`                                                                                                                                                                                                                                                                                                                                     | Each account's preferences blob (`app_setting` `preferences`), and the odd option the server acts on (`auto_complete`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `two_factor`                                                                                                                                                                                                                                                                                                                                | Authenticator secrets and recovery codes: turning it on and off, and `second_factor_ok` at sign-in.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `profiles`                                                                                                                                                                                                                                                                                                                                  | What a profile shows of one account, to whoever `visible` lets see it: cards, ratings, figures, pictures.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `shelves`                                                                                                                                                                                                                                                                                                                                   | Shelves: hand-picked lists, watchlists and smart shelves (`Rules`, `evaluate`), their summaries, and every statement on `shelf` and `shelf_item`.                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `timeline`                                                                                                                                                                                                                                                                                                                                  | The calendar and the diary for one account, and moving a diary day (`redate`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `stats`                                                                                                                                                                                                                                                                                                                                     | What Stats counts, all time or one year, through one `Scope`; the manga half; and `narrow`, the days, streaks and people under a pick. `streaks` is shared with profiles.                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `home`                                                                                                                                                                                                                                                                                                                                      | Home in one request (continuing, waiting, this week, recent, favourites, shelves), and "Because you watched".                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `studios`                                                                                                                                                                                                                                                                                                                                   | Studios credited on titles Frog holds, and the TMDB logo of a studio AniList names (`studio_match`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `routes`                                                                                                                                                                                                                                                                                                                                    | One module per area.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |

### Sessions and accounts

A session token is 32 random bytes. Only its SHA-256 is stored, so a stolen
database file does not hand over live sessions. The cookie is `HttpOnly`,
`SameSite=Lax`, and `Secure` when `FROG_SECURE_COOKIES=1` (set that whenever
something in front terminates TLS, which Caddy does on the deployed site).

The first account is the owner, made from `FROG_ADMIN_USERNAME` /
`FROG_ADMIN_PASSWORD` at boot or from the setup screen, which refuses once
any account exists. Everyone after that joins through a single-use, expiring
invitation an administrator makes. There is no open sign-up.

Someone who forgets a password gets a single-use link from an administrator
(`/reset?code=…`), under the same rules as changing an account. `POST
/api/auth/reset` is public on purpose, since its user cannot sign in: the code is
128 random bits, only its hash is stored, attempts are throttled by address, and
every bad code gets one answer after the same password work. The owner has nobody
above them, so `frog owner-reset-link`, run on the server, makes theirs.
A rename goes through `Auth` and the current password; nothing is keyed by a
name, so only the addresses built from it change.

A new username takes 2 to 20 characters, counted as a person counts them, so a
flag or a family emoji is one. Sign-in never
applies that limit, because accounts older than it may have longer names; it
only refuses names past 256 characters, which it answers like a wrong password
without storing anything. An unknown name does the same password work as a real
one (`auth::spend_password_work`), so the time an answer takes does not say
which names exist, and it counts towards the same throttle for the same reason.

### The TMDB cache is the offline story

Every TMDB request is keyed by path plus query and stored in `tmdb_cache` with
a TTL that matches how fast that data actually changes:

| Endpoint            | Fresh for |
| ------------------- | --------- |
| search              | 1 hour    |
| movie details       | 7 days    |
| tv details, seasons | 12 hours  |
| person              | 7 days    |
| trending            | 6 hours   |

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
app_user ── session, recovery_code, invite, password_reset, app_setting,
            user_image, push_subscription, push_sent, push_sent_title

title (shared catalog; id is Frog's own) ──┬── episode ──┬── episode_state  (notes, per account)
  kind (film, show, manga), metadata,      │             └── watch_event    (the diary, per account)
  merged_into                              ├── library_entry ── chapter_read  (a manga's reads)
                                           ├── title_external (TMDB / AniList / IMDb ids; one primary)
                                           ├── library_entry (per account: status, rating, …)
                                           ├── season_rating (per account, per season or part)
                                           ├── shelf_item ── shelf (per account)
                                           └── title_person (credits, shared)

anilist_link · anilist_oauth · anilist_entry      per account
anime_map · anilist_media · anilist_chain         shared AniList knowledge
airtime_show                                      shared: a TMDB show's AniList and TVmaze ids
airing_show ── airing_episode · airing_film       shared: the Airing page's week, by TMDB id
airing_state                                      shared: when that list was last built
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
  source's own watches are ever taken back by it. AniList's first sync of
  an entry is a backlog and leaves its watches undated; what it moves on
  after that is dated to the sync, so it reaches the diary and Activity.
- **Manga are AniList's titles.** Kind `manga`, primary provider AniList,
  never on TMDB. Chapters stand in `episode_count`, volumes in
  `season_count`, and how far in someone is is a count
  (`library_entry.progress`) rather than ticked episodes, as for an
  AniList-only show with no episode list (`store::is_counted`). What was
  read on which day is `chapter_read`, a row per chapter owned by the
  library entry: the diary, Activity, Continue watching and the Manga half
  of Stats read it beside `watch_event`, never in place of it, so no film or
  episode figure ever counts a chapter.

Specials (season 0) never count towards completion. Marking a season watched
only adds rows for episodes with no watch yet, so it cannot silently inflate
your rewatch counts.

---

## Frontend

`frontend/`, SvelteKit 3 + Svelte 5 runes, built with `adapter-static` as a
pure SPA. The Rust server owns routing and falls back to `index.html`.

- **Filters live in the address.** Search and Library keep theirs there with a
  shallow navigation, so Back and a shared link land on the same list. A
  shallow address is in `page.shallow`, not `page.url`: read it through
  `lib/address.ts`.
- **One library page, two owners.** `LibraryBrowser` is your Library, and read
  only, in pages, someone else's (`/people/{name}/library`). The second is served
  as `store::SharedItem`, a type with no notes in it, so a new field on an entry
  stays private until someone names it there.
- **Links go by Frog's id.** `lib/titles.ts` (`titleHref`, `titleKey`) is
  the one place that decides a title's address: `/t/{id}` once Frog has it,
  the provider's address for a search result it does not.
- **Routes are pages, components are parts.** A route composes; a part with
  its own state and purpose is a component in `src/lib/components/`
  (Settings' groups are in `src/lib/components/settings/`). Shared logic is
  a module in `src/lib/`.
- **Design tokens in `app.css`.** Every colour, radius and spacing step is a
  custom property. Frog is drawn as glass: translucent panes over a light that
  sits fixed behind the page (`body::before`, built from three glows). Every
  appearance (Glass, the default, Glass Light, Pond, Pond Light, Dark, AMOLED,
  Light) is only a token set over the same components; the token contract is
  at the top of `app.css`. Panes (`--bg-elev`) are translucent and never
  blurred, since only the light is behind them. Anything that opens over the
  page uses `--bg-float`, which is solid, so a menu never shows what is under
  its words. Only the two bars are frosted (`--frost`): a blurred pane makes
  the browser draw again whatever scrolls under it. Each theme names its tone
  (`THEMES` in `settings.svelte.ts`), set as `data-tone` on the root for the
  few rules that differ between light and dark. A title page overrides
  `--accent` locally from its poster, and the theme editor writes custom
  properties at runtime with no component changes.
- **Shared primitives in `app.css`.** `.btn`, `.chip`/`.chips` (toggles that
  wrap), `.segmented` (one of a few, the chosen child `.on`), `.toggle-switch`,
  `.card` (a pane), `.list-pane` (rows in one pane with hairlines between),
  `.sticky-head` and `.page-head`. A page reaches for these before writing its
  own; what is only that page's stays in its component.
- **A long word cannot widen a page.** `app.css` sets `overflow-wrap: anywhere`
  on `body`, so a name with no spaces wraps wherever it lands, in flex and grid
  rows too. Rows of Frog's own words opt out where they would stack letters.
- **Fonts and icons are bundled.** Fontsource packages and Phosphor paths
  inlined in `Icon.svelte`, each as [regular, filled] with the Phosphor name
  over it; nothing is fetched from a CDN at runtime. Frog's own face asks for
  Apple's system face first, which only Apple devices have, then Inter.
- **View Transitions.** Cross-route fades plus a real shared-element morph: a
  poster in the library grid carries `view-transition-name`, and the same name
  lands on the hero poster of the title page.
- **Back closes what is on top.** Menus, sheets, dialogs and the command
  palette call `backCloses` (`lib/layers.svelte.ts`), which holds a
  `CloseWatcher` while the layer is open: Back on Android and Esc close the
  newest one. It never adds history steps — those raced the shallow
  navigations that keep Library's and Search's filters in the address.
- **Menus stay on screen.** `use:onScreen` (`lib/onscreen.ts`) nudges a
  dropdown back inside the window on a computer; on a phone `Select` and
  `Rating` draw as a sheet from the bottom over a scrim.
- **One context menu.** A right-click, or a hold on a touch screen, opens
  the menu in `ContextMenu.svelte`, mounted once in the layout and driven by
  `lib/menu.svelte.ts` (`use:contextMenu={source}` on the card). What a title
  or a shelf offers is built in `lib/menus.ts`, so a card in Library, on a
  shelf, on Home or in Search offers the same things in the same words; a
  page only says where the card is and what to read again afterwards. The
  actions reuse the bulk endpoint Library's edit bar uses, so each one comes
  with its undo.
- **Pip.** `Mascot.svelte` is the mascot as a whole frog on a lily pad: one
  inline SVG, posed by a class (`idle`, `lost`, `asleep`, `searching`,
  `happy`, `wave`, `dizzy`, `hop`) and dressed by the mascot chosen in
  Settings. It holds still when animations are off, or when the system asks
  for reduced motion. `Empty` picks a pose from its `art` (asleep for
  nothing yet, searching for nothing found, happy for all done, lost for a
  page that is not there, dizzy for a failure, waving on an empty Home);
  `Spinner` and the pull to refresh hop; sign-in and the welcome wave;
  `toasts.cheer` puts a jumping Pip beside the message when a show's last
  episode or a manga's last chapter goes in.
- **Profiles are tabs.** `ProfileShell` loads a profile once and draws
  `ProfileHeader` over each tab: Overview, Library (`LibraryBrowser` with
  `embedded`), Ratings and Compare.
- **Rating as a deck.** `routes/rate` deals the unrated titles of the library
  one card at a time, the finished ones unless Everything is chosen; which
  titles, in what order, and the keys a format gets are pure functions in
  `lib/rate.ts` (tested in `tests/rate.test.mjs`). It saves through
  `api.track` like a title page and needs no route of its own; the next two
  cards' scores load ahead. A dragged card moves once a frame and every card
  is its own layer, so a swipe never repaints one.
- **How a score is entered.** Every number format (a hundred points, ten, ten
  with a decimal) is slid to on `ScoreSlider`; five stars are filled in on
  `StarPicker`, empty to start, the left half of a star a half; smileys are
  three buttons. The same parts serve the deck and `Rating`. The scales and
  where a touch lands on the stars are `lib/score-entry.ts`, tested in
  `tests/score-entry.test.mjs`. `Score` shows a rating in the chosen format
  wherever one appears, and `Stars` draws five-star scores as icons, the half
  star clipped from the same star.
- **Stats in two halves**, as AniList keeps them: films and shows from
  `watch_event`, manga from `chapter_read` and the counts
  (`/api/stats/manga`), chosen with `?half=manga`.
- **Stats is a dashboard.** `/api/stats` sends, beside its sums, a row per
  title (`facts`: genres, year, rating, and minutes, films and episodes per
  month or year). A bar clicked on the page narrows everything else to the
  titles under it: `lib/stats-filter.ts` works each chart out from every pick
  but its own, tested in `tests/stats-filter.test.mjs`. Nothing picked, the
  page shows the server's sums as they are.
- **Shelves come in three kinds.** Hand-picked, smart, and watchlists: a
  watchlist is a hand-picked shelf with `shelf.watchlist` set, whose picker
  searches everything rather than the library. Any hand-picked shelf takes a
  pasted list (`lib/import/list.ts` reads it into import rows, the import's
  matcher finds them, `ShelfFromList` adds them in order).
- **Search follows typing.** Titles are asked `SEARCH_PAUSE` (60 ms) after a
  key; manga, characters, people and studios (`/api/search/more`) once typing
  pauses (`MORE_PAUSE`, 350 ms), because they reach AniList, which Frog asks
  a call at a time. That route asks AniList once per name for manga and
  characters together (`anilist::media::search_by_name`). Each new key
  cancels the requests it replaces (`AbortSignal`), so late answers never
  land and the server stops waiting on them.
- **Kinds have one home.** `lib/types.ts` holds what each kind is called
  (`KIND_LABEL`), its icon (`KIND_ICON`) and a status in its words
  (`statusLabel`: a manga is Reading, not Watching). A manga's page counts
  chapters with `ChapterCount`, which saves a run of taps as one request.
- **A rating is whole.** `store::whole_rating` keeps every saved rating a
  whole number from 0 to 100, which every format shows exactly, whoever sends
  it; imports clean theirs first (`lib/import/common.ts::cleanRating`), so an
  odd score never fails a title.
- **A null clears.** `LibraryPatch` reads a field sent as null as "clear it"
  and a field left out as "leave it" (`store::sent`); serde alone read both
  as left out, so Clear never cleared.
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

|                  |                                                                                                                                          |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| **0.1 Spawn**    | Accounts, TMDB search, tracking, episodes, the library wall, title and person pages, the PWA, one Docker image, CI.                      |
| **0.2 Ripple**   | The mascot and the Pond theme, Home split from Library, the command palette, the theme editor, server-side settings.                     |
| **0.3 Tadpole**  | Imports from four trackers, export, the calendar, the diary, About.                                                                      |
| **0.4 Lily Pad** | Full stats and the year in review, Web Push, shelves, recommendations, anime imports, two-factor.                                        |
| **0.5 Ribbit**   | Several accounts with enforced isolation, invitations, profiles, administration, AniList sync — and the whole thing deployed.            |
| **0.6 Leapfrog** | AniList both ways through its own sign-in, placed by id; Settings in groups; administrators delete accounts; fuller profiles.            |
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
key stays on the server. The worker refreshes what somebody tracks, up to a
hundred titles a pass (ten MDBList requests), and `/api/t/{id}/scores` gives a
title page its first look, and another look when the title is due and has
nothing from MDBList yet (`look_now`).
A title page shows AniList, IMDb, Rotten Tomatoes, Metacritic, Letterboxd
and MyAnimeList as percentages, each with its own mark (Simple Icons, CC0,
inlined in `Scores.svelte`). While MDBList has nothing for a title (its daily
limit, or no match yet), TMDB's own score stands in.

A title with no AniList id of its own gets AniList's score through the
MyAnimeList id MDBList gives. MDBList is asked by MyAnimeList id as `movie` or
`show`, never `any`, which it refuses. So is an anime whose borrowed TMDB id
MDBList does not answer for. A title checked while no MDBList key was set is
asked again within two days, not a month.

## Logos

A title's logo comes from TMDB. Where TMDB has none, `backend/src/fanart.rs`
looks further, in this order:

1. fanart.tv, for anything TMDB knows: films by TMDB's id, shows by the
   TheTVDB id in TMDB's external ids. An anime Frog keeps from AniList uses the TMDB
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
