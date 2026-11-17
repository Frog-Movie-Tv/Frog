---
name: event
description: Make a new Frog event — a countdown to a big release with its catch-up list, two badges of its own and a premiere animation of its own — as a pack in the repo, on its own branch and pull request. Use when the owner says event, countdown, "add an event for …", or names a release that should get one.
---

# Making an event

Every event is a pack Claude makes: `backend/events/{slug}/` with
`event.json`, `ready.svg` and `opening.svg`, and a choreography in
`frontend/src/lib/premieres/{slug}.ts`. Administration can only switch one
on or off, pin its moment and preview it. CLAUDE.md's "Events" entry says
how the parts fit; `avengers-doomsday` is the worked example.

A pack never contains a provider's art, text copied from anywhere, or a
domain. Names, notes and badge words are Frog's own, in product voice.

## 1. The title

- Ask which release, if the owner has not said.
- Read it from TMDB (the key is in `.env`; never print it): kind, the release
  date for `FROG_REGION` (`/movie/{id}?append_to_response=release_dates`,
  the same rules as `airing::film_opens`), or a show's season and its air
  date. Note its collection (`belongs_to_collection`) and, for a show, what
  `franchise.rs` joins to it.
- The slug is the title in lower-case words joined by `-`, never a year
  unless two releases share a name.

## 2. The list — wait for a yes

Propose, in one message:

- whether it needs a catch-up list at all (a first film or a standalone has
  none: `"catch_up": false`, and then no Ready badge),
- the groups, in watch order, each title with `essential` or optional and a
  note of a few words where it says why (`"Why they split"`),
- for a show, the seasons that count (`"seasons": [1, 2]`),
- `opens` (`cinemas`, `streaming` or `season`), `shows_from_days` (45 unless
  the owner says otherwise), `ends_after_days` (14), and an accent colour
  that suits the film and reads on dark.

Keep essentials to what the story needs: 8 to 14 for a big franchise. Do not
write anything until the owner says yes.

## 3. The pack

- `backend/events/{slug}/event.json`, in the shape of Doomsday's (see
  `events/pack.rs` for every field). `day` is the TMDB date for the region;
  `follows_release` stays true so a moved date follows on its own.
- Add `pack!("{slug}")` to `RAW` in `backend/src/events/pack.rs`. The test
  `every_pack_reads_and_keeps_its_own_rules` checks the rest.
- Badges: "Ready for {Name}" (every essential seen before it opened) and
  "Opening Night: {Name}" (seen in its first three days). Add both scenes to
  `frontend/scripts/badges/scenes.mjs` keyed `event.{slug}.ready` and
  `event.{slug}.opening`, drawn from the same pieces as the others (the frog,
  `glow`, `sky`, `clock`, `ticket`, `seat`, `popcorn`, `sparkles`), with a
  motif of the film's own in its accent. `npm run badges` writes the two
  SVGs into the pack (jade ring for Ready, gold for Opening Night). Look at
  each at 64, 128 and 512 px before going on.

## 4. The premiere

- Write `frontend/src/lib/premieres/{slug}.ts` and add it to `OWN` in
  `premieres/index.ts`. It only says what happens when, with the stage's
  pieces (`stage.ts`): shards, rings, sparks, embers, streaks, cracks,
  sigils, the camera's push and shake, letterbox bars, the logo's sweep,
  the counting line and the badge landing. Give it a motif of its own (sand
  for Dune, not Doomsday's glass). About 6 to 8 seconds, then `handoff`.
- Never load anything: the pictures are the page's own, through `/img`.
- `node scripts/verify-premiere.mjs` (pointed at the new slug) must pass:
  95th-percentile frames at or under 16.7 ms on desktop, the badge whole on a
  phone, skip, a hidden tab, reduced motion. Look at every beat's
  screenshot, then Preview premiere in Administration at both widths.

## 5. Ship it

- A line under Upcoming in `RELEASE-NOTES.md`, in product voice
  ("A countdown to Dune: Part Three, with what to watch first.").
- Its own branch, `feat/event-{slug}`, one `feat(events): …` commit, and a
  pull request. Commit and push only when the owner asks.
- Say plainly how many days are left to merge and release before
  `shows_from` (the day minus `shows_from_days`), since the event appears on
  Home only from a release that carries it.
