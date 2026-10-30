# A catalog Frog owns

Every film and show is Frog's own title, with Frog's own id. Providers —
TMDB, AniList, IMDb — are names the title is known by, kept beside it.
`backend/src/catalog.rs` is the code; this is the reasoning.

## The model

```
title                        one row per film or show Frog knows
  id            INTEGER PK   Frog's id; never reused, never changes
  kind          'movie'|'tv'
  name, dates, artwork …     the metadata Frog shows, from the primary provider
  merged_into   → title.id   set when the title turned out to be another one

title_external               who else knows a title, and as what
  title_id      → title.id
  provider      'tmdb' | 'anilist' | 'imdb'
  external_id   TEXT         "movie/550", "tv/1399"  |  "21"  |  "tt0113277"
  is_primary    0 | 1        this provider supplies the metadata
  gone_at       TEXT         the provider no longer has it
  PRIMARY KEY (provider, external_id)
  at most one is_primary = 1 per title; IMDb is never primary
```

- **Personal data points at Frog's ids** — `library_entry`, `watch_event`,
  `episode_state`, `season_rating`, `shelf_item`, `anilist_entry`,
  `title_merge`, profile pins (`app_user.pins_json` holds `{ title_id }`).
  Nothing personal stores a provider's id.
- **One provider id names one title.** It is the table's key. `catalog::link`
  refuses to point an id somewhere else; it never moves one silently.
- **One primary provider per title**, chosen by Frog: the one the title was
  made from. Today that is TMDB for everything TMDB has and AniList for anime
  TMDB lacks. A title with no provider at all is allowed by the schema and
  opens with what Frog kept; nothing creates one yet.
- **TMDB ids are only unique per kind**, so they are written as TMDB's own
  path: `movie/550`, `tv/1399`.
- **IMDb ids come with TMDB's detail** and are kept as aliases, so an import
  that names a title by IMDb id finds it.

## What happens when a provider changes its mind

- **The provider no longer has a title** (TMDB answers 404, AniList says no
  such anime): `catalog::mark_gone` sets `gone_at`. The title, everyone's
  history and its page stay; the page opens with what Frog kept.
- **Two titles turn out to be one** (an AniList-only title TMDB later finds,
  or a person's correction): `store::move_title_data` moves one account's
  data at a time, in one transaction, logged in `title_merge`. Once nobody
  keeps the old title it gets `merged_into`. Its provider ids stay on it, and
  `catalog::find` follows `merged_into`, so an old AniList id or link still
  lands on the survivor. `catalog::externals` lists a survivor's own ids and
  those of titles merged into it; backups carry both.
- **A provider id changes**: it is a new id. The old row keeps `gone_at`.

## Episodes

Episodes are Frog's rows (`episode.id`), numbered as the title's primary
provider numbers them. There is no `episode_external`: where one provider's
episode is in another's numbering is the AniList placement, which is per
account (`anilist_entry`, with manual choices that must stay personal) and
backed by shared community lists (`anime_map`). Nothing shared would be
reliable enough to store as catalog data yet. `episode.tmdb_episode_id` is
kept as metadata only.

## Addresses

| | |
| --- | --- |
| `/t/{id}`, `/api/t/{id}/…` | A title by Frog's id. Every link the app draws, and every action. |
| `/title/{kind}/{id}`, `/api/titles/{kind}/{id}` | A title as a provider names it: `movie/550`, `tv/1399`, `anilist/21`. Search results, credits and recommendations use these until opened; the page opens the title (adding it to the catalog if it is new) and replaces its address with `/t/{id}`. |
| `/title/tv/-21` | How AniList-only titles were addressed before. Still opens them. |

## Backups

`frog-library` version 2 gives each title its Frog `id` and `ids`
(`tmdb`, `anilist`, `imdb`), and keeps every version 1 field (`tmdb_id` is
null for an anime TMDB lacks). Imports go by provider ids, then names. A Frog
id in a file is never taken to mean a title in this database: it only means
something to the Frog that wrote it. Version 1 files still import.

## How the old databases got here

`202610301000_catalog_identity.sql` creates `title_external`, gives every
title its provider rows (`tmdb_id > 0` → TMDB, `< 0` → AniList,
`imdb_id` → IMDb alias) and turns profile pins into Frog ids. Then
`db::rebuild_title`, right after the migrations, rebuilds `title` without
`tmdb_id`, `anilist_id` and `imdb_id`.

That step is Rust, not SQL, because SQLite only drops a column in a UNIQUE
constraint by rebuilding the table, a rebuild of a table others reference
needs foreign keys off, and SQLite ignores that inside the transaction sqlx
runs every migration in. With them on, dropping the old table would cascade
into every library. `rebuild_title` runs on one connection with foreign keys
off, in its own transaction; it copies ids as they are, carries the id
sequence over (so no id is ever reused), checks the row count and
`foreign_key_check`, and rolls back on any difference. It only runs while
`title` still has `tmdb_id`, so it is idempotent.

Before copying, it repeats the migration's backfill for rows that have no
provider yet. That covers the one window where an older binary could write:
the migrations ran, the rebuild failed and rolled back, and `update.sh` put
the previous image back on that database. Titles that binary adds get their
provider rows and its `{ kind, tmdb_id }` pins are converted. If a title
still has no primary provider after that, the rebuild refuses and leaves
`title` as it was.

**This release is not additive.** A previous image cannot run on the new
`title` table, and it would not notice: `/healthz` only asks the database
for `SELECT 1`. So Frog keeps a schema generation in SQLite's
`user_version` (`db::SCHEMA_GENERATION`, 1 from this release). It goes up
only when a release leaves a schema the one before cannot read.
`deploy/update.sh` reads it from the pre-update snapshot and again after a
failed update. If it went up, it does not try the previous image on that
database: it restores the snapshot and starts the previous image on that.
`deploy/tests/update-rollback.sh` runs every path (healthy, additive
failure, failure past the previous generation, both images failing) against
stand-ins for Docker, in CI. This is a deliberate exception to the
two-release rule in `docs/CONTRIBUTING.md`, taken because Frog is before 1.0.

The server keeps its own copy of the deploy scripts (`install.sh` copies
them to `/opt/frog/deploy`), so the new `update.sh` has to be copied there
by hand before the first image with this release is pulled. The older
script would start the previous image on the rebuilt table, call it healthy
and delete the snapshot.

## Tests

- `backend/tests/catalog.rs` — a database from the release before, with TMDB
  titles, an AniList-only title, a merged one, IMDb ids, two accounts,
  shelves, ratings, an AniList placement and pins: migrated, every personal
  row identical, ids and id sequence kept, foreign keys intact, every title
  with exactly one primary, every provider id resolving, pins converted,
  idempotent. A fresh database. The constraints refusing ambiguous mappings.
  Merging titles of different providers, account by account. A provider
  losing a title.
- `backend/tests/migrations.rs` — the older AniList upgrade path.
- `backend/tests/transfer.rs` — imports by provider ids, never by another
  Frog's ids; backup version 2 field for field.
- `backend/tests/anilist.rs` — AniList-only titles adopted by TMDB titles,
  old AniList links following.
- `frontend/e2e/smoke.spec.ts` — `/t/{id}` behind the wall, bad addresses.

## Not decided

- Whether a person can choose a title's primary provider.
- Whether Frog ever makes a title no provider has (a home video, a festival
  short). The schema allows it.
- TVDB as a provider. `anime_map` reads TVDB ids for placement; adding
  `'tvdb'` to the provider check is a migration when something needs it.
