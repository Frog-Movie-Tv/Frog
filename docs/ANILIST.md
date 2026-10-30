# AniList

How Frog keeps each account's AniList list and its Frog library in step.
For where it sits in the whole system, see [ARCHITECTURE.md](../ARCHITECTURE.md).

## The modules

`backend/src/anilist/`, one concern each. Arrows are "uses"; nothing points
back up, and only `sync` and `local` talk to AniList on a sync's behalf.

| Module | Owns | Talks to |
| --- | --- | --- |
| `client.rs` | Sign-in (authorize URL, code exchange, viewer), the paced and retried GraphQL call, reading a list, saving an entry, `Remote` and `View` | AniList |
| `idmap.rs` | The two community id lists in `anime_map`, and `anilist_media` | GitHub raw files, SQLite |
| `place.rs` | Where an entry belongs on TMDB and how sure that is: `Target`, `Placement`, `RESOLVER`, `CONFIDENT` | TMDB (through `state.tmdb`), AniList relations, SQLite caches |
| `reconcile.rs` | **Who wins.** Change detection between snapshots, the mode (merge or follow), what is sent back, incoming status and rating, a show's one status. Pure: no I/O | nothing |
| `library.rs` | Frog's side of an entry: which episodes it covers (`run_ids`), how Frog sees it (`frog_view`), writing AniList's side into a title the caller already has (`apply`), taking back a wrong placement (`unplace`), settling a show | SQLite through `store`, `catalog` and `release` |
| `local.rs` | Entries TMDB lacks, kept as AniList-only titles, and moving them across once TMDB has them (`adopt_local`) | `library`, `client` |
| `media.rs` | An AniList-only title's details rewritten into TMDB's shape, people and studios | AniList |
| `records.rs` | Every read and write of `anilist_link`, `anilist_oauth` and `anilist_entry` | SQLite |
| `sync.rs` | One sync of one account (`sync_user`), the worker's `due` list, `diagnose` | all of the above |

`routes/anilist.rs` only checks input and calls `records`, `forget_place`
and `sync_user`.

## One sync

1. `records::link_token` — whose list, with which token.
2. `idmap::ensure_map` — the id lists, at most a week old.
3. `client::read_list` — the whole list, 500 at a time; any failed chunk
   fails the sync, because a list read halfway looks like entries removed.
4. `records::mark_gone` — entries no longer on the list get `gone_at`.
5. For each entry (`sync::Run::entry`):
   - ignored or detached → nothing;
   - not placed, nothing new, not due for another look → kept as it is
     (`local::sync_local`);
   - `decide`: a manual or current placement stays; otherwise `place::place`.
     Unplaced → `leave_unplaced` (note, candidates, next look) and kept as an
     AniList-only title;
   - `sync_placed`: move if the place changed (`unplace`), adopt an
     AniList-only title, compare both sides with their snapshots
     (`reconcile::Changes`), apply AniList's side (`library::apply`) when it
     wins, send Frog's side (`reconcile::outgoing` → `client::push`) when it
     wins, save both snapshots.
6. `settle_shows` — one status per show once all of its seasons are in, then
   Frog's snapshot of each of its entries.
7. `records::sync_succeeded` or `sync_failed` (with backoff).

A failure about one entry is counted in `Summary.failed` and the rest go on.
TMDB or AniList out of reach (`is_outage`) stops the sync; nothing is marked
failed for it, and it is tried again with backoff.

## Invariants

- Watches from AniList are `source = 'anilist'`. Only those are ever taken
  back; a watch logged in Frog is history and stays.
- A first sync, a re-placed entry and an adopted title only add: nothing on
  either side is lowered or removed.
- Paused and dropped are the person's word; a merge never overrides them
  (`transfer::merged_status`).
- Manual placements are never moved by the matcher.
- Every statement names the account (`tests/scoping.rs`).

## Tests

- `reconcile.rs` unit tests: every who-wins decision on plain values.
- `place.rs`, `client.rs`, `media.rs` unit tests: names, ids, parsing.
- `backend/tests/anilist.rs`: a stand-in AniList server and cached TMDB
  answers, end to end — both directions, placement by every method,
  outages, a refused token, overlapping syncs, AniList-only titles adopted
  later, isolation between accounts.
- `cargo run --example anilist_check -- <user>` against a real public list.

## Behaviour in detail

- **AniList sync** (`backend/src/anilist/`): `place.rs` decides where an
  entry goes, `sync.rs` runs a sync and `reconcile.rs` decides who wins. Evidence is gathered in this order and stops at
  the first confident answer (confidence 70+): Fribb's anime-lists (TMDB
  season and offset; a spot two entries claim scores 50), Kometa's Anime-IDs
  (TVDB numbering turned into TMDB's), a film or unreleased entry by name,
  the prequel chain counted on from the first season, the air date across
  related shows (specials only ever in season 0), then the series by every
  title and synonym AniList has. Every answer is checked against TMDB's air
  dates. Below 70 nothing is applied: the entry is "not confident" with a
  note and up to six candidates. Each placement records its method, note and
  the `RESOLVER` version; a sync re-places automatic ones from an older
  version, takes back only the ticks it gave (never ones another entry
  covers) and counts them as repaired. Manual choices are never moved. An
  entry of unknown length runs to the next entry's start or the end of the
  show, across TMDB seasons. Each entry keeps a snapshot of both sides from
  the last sync; whichever side differs from its snapshot changed and wins.
  A Frog title never on AniList is not added there, and removing one in Frog
  never removes it on AniList — both on purpose.
- **AniList, kept running** (`anilist/sync.rs`, `anilist/client.rs`). The worker looks every
  30 s and syncs an account `ANILIST_SYNC_MINUTES` (default 3) after its last
  finished sync, or 45 s after a change in Frog. The list is read in chunks
  of 500 and a sync fails whole if any chunk fails. Requests retry a dropped
  connection, a 5xx or an unreadable answer with jittered backoff (4 tries),
  and wait out a 429 as long as Retry-After says (65 s at most). `synced_at`
  moves only on success; a failure sets `retry_at` (1 min doubling to 60) and
  a refused token sets `needs_reconnect`, which only a reconnect clears.
  An entry TMDB cannot place is kept as an **AniList-only title**: a catalog
  title whose primary provider is AniList (`catalog::ensure_local_title`;
  `title_external` row `anilist <id>`), opened at `/t/{id}` like any title
  and never sent to TMDB. Its progress is
  `library_entry.progress`, a count. It is looked for again on a schedule
  (`tries`, `next_try_at`: an hour doubling to three days, twelve hours when
  not out yet; at once when AniList's episodes, format, state or start date
  change, or on Sync now). Once placed, `store::move_title_data` moves that
  account's entry, shelves and film watch to the TMDB title in one
  transaction, logs it in `title_merge`, and points the old title on with
  `merged_into` once nobody keeps it; its AniList id then resolves to the
  TMDB title (`catalog::find`). Progress lowered on AniList takes back
  only `source = 'anilist'` ticks. Entries gone from the list get `gone_at`
  and nothing else; an entirely empty answer marks nothing.
- **AniList-only titles look like TMDB ones** (`backend/src/anilist/media.rs`).
  One AniList query (plus its airing schedule, 50 a page) is rewritten into
  TMDB's detail shape, so every reader in `tmdb/models.rs` works unchanged:
  cover, banner, synopsis (HTML and Markdown stripped, spoilers dropped),
  genres in TMDB's names plus Animation, score, runtime, voice cast,
  director, creator and writer, main studio, YouTube trailer, streaming
  links with their own address (`Provider.url`), recommendations and
  related works (to the TMDB title when the anime lists map it), and season
  1 episodes in AniList's numbering, dated from the schedule (else only the
  first and last, from the start and end dates). Cached in `tmdb_cache`:
  12 h while airing, a week once finished; the worker refreshes airing ones.
  The detail and list items are marked `"source": "anilist"`, so their ids
  read as AniList's (`SearchItem.anilist_id`), never as TMDB's. AniList
  people and studios are still negative ids, with their own person and
  studio pages; they are not catalog titles. Pictures are `/anilist/…` and `/crunchyroll/…` paths that
  the image route fetches from those two hosts only, so Caddy's CSP needs
  nothing new. An AniList address (`/title/anilist/{id}`) whose show the
  anime lists map to TMDB opens that TMDB title instead of making a copy,
  unless a sync already keeps the AniList one.
  With episodes listed, the sync ticks and syncs them like any show's
  (`local.rs`), so Home, the calendar, statuses and season
  ratings all apply; when TMDB later places the entry, watches logged in Frog
  move to the episode at the same place in its run.
- **Season ratings** live in `season_rating` (user, title, season, first
  episode). A season split into AniList parts is rated part by part; a unit
  is rateable once every episode of it is out. The show's own rating is no
  longer pushed to AniList. Episodes are no longer rated (see ROADMAP, Next).
- **`cargo run --example anilist_check -- <user> [scratch.db]`** places a
  public AniList list without writing anywhere and prints one JSON line per
  entry: the quickest way to see what the matcher does with a real list.
