# Changelog

Everything that changes in Frog, written for the person using it.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing yet.

## [0.2.0] — 2026-10-24

The first proper pass over the alpha: Frog has a face, a home screen and a lot
of knobs.

### Added

- **A frog.** Frog finally looks like its name: a round green face with big
  eyes, a small smile and pink cheeks. It is the app icon, the favicon and the
  logo in the corner, and it stays readable shrunk down to 16 pixels.
- **Four mascots to choose from** in Settings — Classic, Sleepy, Popcorn and
  3D glasses. Picking one changes the logo and the browser tab icon right away.
- **The Pond theme**, built from the logo's own colours: lily-pad green on a
  deep pond green-black, with cream text and blush pink. It is now the default,
  and there is a light version of it too.
- **A Home screen**, separate from the library. It opens on a big image of
  whatever you are in the middle of, then shows what to continue, what airs
  this week, what you added recently and your favourites.
- **Continue watching looks right.** Instead of one enormous portrait poster,
  each card is a wide still with the show's logo, the exact next episode and a
  progress bar.
- **A command palette.** Press <kbd>Ctrl</kbd>+<kbd>K</kbd> (or <kbd>/</kbd>) to
  jump to any title, page or action. It searches your library instantly and
  falls back to TMDB. `g` then `h`, `l`, `s` or `t` jumps straight to Home,
  Library, Search or Stats.
- **A theme editor.** Change the background, surfaces, accent and text colour
  with a live preview, then save, duplicate, export or import themes as JSON.
- **A lot more to adjust**: font, text size, corner radius, glass blur,
  animation level, card style, poster size, grid spacing, which details show on
  cards, hero style, the order of the home rows, which page Frog opens on,
  default status, automatic dates, half stars, date format and a custom CSS box.
- **Spoiler protection**, which blurs the still for any episode you have not
  watched yet.
- **Settings now follow you.** Preferences are stored on the server, so your
  phone and your laptop agree instead of each keeping their own.
- **A "What's new" page**, with a dot on Settings after an update.
- **Private by default**: the app asks search engines not to index it, serves a
  `robots.txt` that blocks everything, and sends no referrer.
- **"Remember this device"** on the sign-in screen. Leave it off on a shared
  computer and the session ends with the browser.
- **Sign-in is rate limited.** Eight wrong passwords and the door closes for ten
  minutes.
- **Four ways to draw the library**: posters, wide landscape cards, a list, or
  a compact table.
- **Mobile filters fit in one button** that opens a sheet, instead of three rows
  of controls eating the top of the screen.
- **The episode heatmap is filled in.** It now shows TMDB's score for every
  episode as soon as you add a show, with your own ratings painted over the top
  and ringed, and the squares are big enough to see.
- **Search results say Film or Show**, lead with the thing you probably meant
  rather than obscure shorts, and have an add button on every card instead of
  one that only appears when you hover.
- **Discovery can skip animation**, if that is not what you are here for.
- **Streaming logos are links** to where you can actually watch the thing.
- **Rows have arrows** on a wide screen, and no longer hide their first card
  under the edge of the page.
- **Settings can check for a newer version**, on request only — Frog does not
  phone home on its own.

### Fixed

- **The Stats page no longer breaks.** It was showing a raw database error.
- **Errors are human now.** An internal fault says something went wrong and puts
  the detail in the log, instead of printing database internals at you.
- **The library could fail to load** once it held a title you had never watched.
- **The browser tab title was stuck** on whatever title page you had opened
  last. Every page names itself now.
- **Episodes without a still no longer wreck the row layout**; the slot stays
  put and shows the episode number instead.
- **Episodes that have not aired cannot be ticked or rated.** They are dimmed
  and say when they are due, and marking a whole season watched skips them.
- **Less flicker while scrolling.** The blurred bars are kept on their own layer,
  and if your machine still struggles you can turn the blur off in Settings.
- **Rating is no longer a row of sliders.** One small control per title or
  episode that says "Rate" when it is empty and opens a tap-to-choose popover.
- **The search button in the corner** opened the search page you were already
  one tab away from. It opens the command palette instead.

### Changed

- The library moved to its own page at `/library`, leaving `/` for Home.
- **The README is for people now.** A proper front page with screenshots of the
  real app, taken automatically from a demo library so they never go stale.
  Installation moved to [docs/INSTALL.md](docs/INSTALL.md).
- Preferences changed moments before a reload are no longer lost.

## [0.1.0] — 2026-10-24

The first version that runs.

### Added

- Sign in, with a one-time setup screen on a fresh install.
- Search all of TMDB and add anything to your library.
- Five statuses, ratings, favourites and notes.
- Tick episodes one at a time or a whole season at once, with per-episode
  ratings and notes.
- A library wall with filters, sorts and search.
- Title pages with colours taken from the poster, logo artwork, cast and crew,
  seasons, an episode rating heatmap, trailers, franchise order and where to
  stream in Belgium.
- Person pages that highlight what you have already seen.
- Five rating formats, from 100 points to three smileys.
- Installable on a phone, with an offline shell.
- Runs as one Docker container, or straight from a script with no Docker.

[Unreleased]: https://github.com/MihCons/Frog/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/MihCons/Frog/releases/tag/v0.2.0
[0.1.0]: https://github.com/MihCons/Frog/releases/tag/v0.1.0
