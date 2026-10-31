# Imports and exports

Bringing a library in from another tracker, and taking copies of it out.
Imports can touch everything an account has, so this is one of the two
places (with AniList) where behaviour is pinned down by tests before any
change.

## The shape of an import

```
browser                                   server
──────────────────────────────            ─────────────────────────────────
file ─► reader (lib/import/parsers.ts,    POST /api/import/match  ─► transfer::match_rows
        netflix.ts, anime.ts)                 which TMDB title, and why not sure
     ─► Incoming rows (lib/import/common.ts)
     ─► dedupe, preview, the person
        settles what is in doubt          POST /api/import/apply  ─► transfer::apply_items
     ─► apply, 200 rows at a time             writes the library, adds watches
```

Parsing stays in the browser: the wizard stays responsive and the server
never deals with file formats. Everything lands in one shape, `Incoming`, so
a new source is a new reader and nothing else.

## Where things are

| | |
| --- | --- |
| `frontend/src/lib/import/common.ts` | `Incoming`, CSV reading, date and year helpers, `newRow` |
| `frontend/src/lib/import/parsers.ts` | One reader per format (Letterboxd, IMDb, Trakt, TV Time, Frog backup), unzipping, `dedupe` |
| `frontend/src/lib/import/anime.ts` | Anime trackers: season and part from a title (`animeTitle`), MyAnimeList XML |
| `frontend/src/lib/import/netflix.ts` | Netflix viewing history: grouping plays into shows and seasons, then settling them with TMDB's answer |
| `frontend/src/lib/import/rows.ts` | Turning settled rows into what `apply` takes |
| `frontend/src/lib/import/review.ts` | Sorting rows for review: the kind of problem, grouping by title, what can be kept in bulk, what was barely watched |
| `backend/src/transfer/matching.rs` | Which TMDB title a row means: ids, then names, by year, spelling and season; episodes by name |
| `backend/src/transfer/apply.rs` | Conflict modes, `merged_status`, ticking seasons and episodes, film diary entries |
| `backend/src/transfer/export.rs` | `frog-library` JSON (version 2) and the CSV sheet |
| `backend/src/routes/transfer.rs` | Batch size, the account, the download headers |

## Rules that must not change by accident

- **Identity:** an import names titles by provider ids (TMDB, then IMDb,
  then AniList) and by name; `apply` lands on whatever title the catalog
  has for that id, following merges. A Frog id in a backup is never used:
  it only means something to the Frog that wrote the file.
- **Matching:** a TMDB id, then an IMDb id, then an AniList id, win outright. Then names in
  order (title, then alternatives); animation first for anime rows; the
  preferred kind first; a show with the row's season beats a namesake; then
  the same name in the same year, the same casing, the same letters. A year
  more than one off (and not a later season), a near name, or a namesake of
  the other kind is said in `note`.
- **Conflicts:** `skip` leaves a tracked title exactly as it is. `overwrite`
  takes the file's status, rating (an empty one too) and favourite.
  `merge` only moves the status forward, fills an empty rating and adds a
  favourite; paused and dropped are never overridden.
- **Watches are only ever added.** An episode already watched keeps its
  watch. The one thing a later import can change about it: a watch an earlier
  import left undated takes the day this one gives, and nothing else moves.
  Progress without dates is undated (in totals, not in the diary).
  A dated film is one diary entry, once, with its rewatch flag; a finished
  film with no date gets one undated watch.
- **A count of zero is not a missing count.** A finished row with no
  `progress` ticks its whole season; `progress: 0` ticks nothing, which is how
  a finished anime special stays unticked.
- **A row about one episode or season says nothing of the show.** A Trakt entry
  for an episode makes the show `watching`, never `completed`, and its rating
  is not the show's.
- **Shows never get a title-level watch.**
- **Every show imported has its episodes listed**, whether or not anything is
  ticked. Without them it has no next episode and only TMDB's count to go by.
- **A Netflix show with nothing to tick arrives as planned.** A show's own name
  with no episode, or episodes with no season to place them in, say it was
  opened, not how far anyone got. Giving it a season in the review ticks those
  episodes, and `apply` then moves it to `watching`. A tracker that says
  `watching` itself is taken at its word, as above.
- **A planned later season** does not send a show you are part-way through
  back to planned.
- **Dates:** readers keep the day of a timestamp and set it to 20:00 UTC; the
  time of day a tracker records is not kept. Trakt gives exact times and
  they are dropped too. This is the documented, tested behaviour
  (`tests/parsers.test.mjs`); keeping exact times would be a change with its
  own requirements, not a fix.
- **Exports** are a contract with files people already hold: add fields,
  never rename or remove them. Backup version 2 adds `id` (Frog's) and
  `ids` (`tmdb`, `anilist`, `imdb`) to every version 1 field; `tmdb_id` is
  null for an anime TMDB lacks. The CSV added `Frog ID` and `AniList ID` as
  its last columns. A Frog backup of either version is read back by
  `parsers.ts` (`readFrogJson`), with the episodes that were watched, undated:
  the file says which, not when.

## Tests

- `backend/tests/transfer.rs`: matching by id, year, casing, alternative
  name, animation and season; episodes by name; batch limits; skip and
  overwrite; exact episodes and dates; films logged once; both export files
  field for field.
- `backend/tests/api.rs`: anime seasons and season parts, merge never undoing
  what you did, accounts never seeing each other's data.
- `transfer/matching.rs` and `transfer/apply.rs` unit tests: the choosing and
  merging rules on plain values.
- `frontend/tests/parsers.test.mjs`: every reader on a small file in its
  source's shape, with gzip and ZIP handling; `frontend/tests/netflix.test.mjs`:
  the Netflix reader on a real 4,054-line export;
  `frontend/tests/review.test.mjs`: every message on its kind of problem,
  grouping, what bulk keeping may touch, and the threshold. Node has no `DOMParser`, so a
  small stand-in reads MyAnimeList's XML there; the XML itself is only ever
  parsed by a real browser.

## Behaviour in detail

- **Netflix import** (`frontend/src/lib/import/netflix.ts`) groups plays into
  one row per show and season before matching, then `settleNetflix` finishes
  them with TMDB's answer: titles sent one by one fold back into their show,
  named seasons ("STONE OCEAN", "Egghead Arc ①") are placed only when a TMDB
  season has that name, and a show is completed only when every season was
  seen to the end. Shows are split by season marker first, then by where
  titles branch across the whole file, never by "the first colon". The
  server looks every row's episode names up (`find_episodes` in
  `backend/src/transfer/matching.rs`): a row sure of its season reads that
  one, one in doubt reads the whole show, and a name placed there settles the
  exact episode and its date. In doubt it also settles the season and proves
  the match; a numbered season stays where Netflix put it, and names not found
  count from its first episode, undated, with no warning. "Episode N" needs
  no lookup at all. Rows are matched six at a time (`MATCH_AT_ONCE`).
  `npm test` runs it against `tests/fixtures/netflix-viewing-history.csv`,
  real titles with made-up dates; `node --test` reads the TypeScript directly
  through `tests/ts-paths.mjs`.
- **The review** sorts each row's messages into five kinds by their wording
  (`review.ts`), so a new warning needs a line there or it lands under "Check
  the match". A closest match with any other doubt counts as "Check the
  match": only a plain one can be kept in bulk. Bulk actions reach only the
  rows on screen under a filter, never All, and a row left out stays on
  screen, dimmed, until the filter changes. Rows needing a look are still
  imported as shown unless someone leaves them out.
  `node scripts/verify-import.mjs [file.csv]` walks it in real Chromium.
- **Barely watched** (`holdBack` in `review.ts`). Netflix lists every play,
  including a show opened once. After settling, a show with fewer than
  `SHOW_THRESHOLD` (3) episodes across all its rows is left out and listed under
  the Barely watched chip, in the same review, with "Add these back" and an undo.
  Nothing is dropped: the rows stay, and the finished import counts them as Left
  out. The review's own menu moves the threshold, from none to 10; there is no
  setting elsewhere, and each import starts at 3.
  - The count is distinct episodes, so five plays of one episode are one.
  - A show already in the library is never left out: its watches are worth
    having.
  - Moving the threshold only undoes what it did. A row ticked or unticked by
    hand stays as the person left it.
  - Rows with no match are not counted. They were never going to be imported,
    and stay under No match.
  - **What cannot be detected.** The viewing history has titles and dates and no
    watch time, so a film stopped after five minutes looks exactly like one
    watched to the end, and an episode abandoned early counts as watched. Films
    always come in from that file, and the review says so. A title with no
    episode name reads as a film whatever TMDB matched it to, so it stays too.
    The data-request ZIP has durations: a film under 15 minutes in all is left
    out on that evidence (`NetflixMeta.seconds`). That file has never been tried
    on a real export.
  - On the 4,054-line export, 768 rows (611 titles, 3,550 episodes) become 627
    rows (470 titles, 3,390 episodes). 141 shows wait under the chip: 117 with
    one episode and 24 with two. Every film stays.
- **Imported shows get no title-level diary entry.** `apply` only writes one
  for films; a show's watches are its episodes, and a title-level watch
  would count the show as a film in Stats.
