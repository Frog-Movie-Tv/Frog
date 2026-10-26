# Changelog

What changed in each release of Frog.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## Upcoming — Lily Pad —

### Highlights

- **Import from AniList and MyAnimeList.** Enter an AniList username or choose
  a MyAnimeList export. Each season on your list lands on the right season on
  TMDB, parts and long-running seasons included, with the episodes you have
  seen already ticked. Anything Frog is unsure of asks you first.
- **When did you watch this?** Marking a season or a whole show asks: today,
  on release, on a date you pick, or you don't know. Undated episodes count
  towards your progress and stay out of the diary.
- **A tidier diary.** A day of one show is one entry, such as "S04E08–E16 ·
  9 episodes", that opens to show each episode. Change the date of any entry.
- **Two-factor sign-in.** Turn it on in Settings → Security, scan the code
  with any authenticator app, and keep ten recovery codes for a lost phone.
- **Choose your tabs.** On phones, pick four tabs and find the rest under More.
  Reorder or hide pages in Settings → Navigation.

### Improvements

- Import and export live in Settings → Your data, in the Library menu and on
  an empty library.
- Each status has its own colour, everywhere it appears.
- The calendar shows what is on inside each day, with a legend, and the chosen
  day opens beside the month.
- Home's hero reads clearly on light themes and says Start watching for a show
  you have not begun.
- Data saver loads smaller images.
- Calendar and Diary can be your start page.
- Mark a whole show watched from its page.
- Sign-in remembers this device unless you untick Keep me signed in.

### Fixes

- Frog opens and works offline once installed, signed in.
- The command palette's search field sits on one line.
- Season headers fit on narrow phones.
- Every dropdown uses Frog's own menu.

## [0.3.0] — Tadpole — 2026-10-24

Import your library from elsewhere, plan the week ahead, and read back
everything you have watched.

![Home in the Pond theme](docs/screenshots/home-desktop-pond.png)

### Highlights

- **Import from Letterboxd, Trakt, IMDb, TV Time and Frog backups.** A guided
  importer reads the export, finds each title on TMDB and shows you the
  matches before anything is written. Anything uncertain is flagged so you can
  choose the right one, and you decide whether the file or your library wins.
- **Export your library** as a full JSON backup or a CSV, from the Import page
  or the command palette.
- **Calendar.** A month view and an agenda of every episode and release you
  follow, with the days you have already watched marked off.
- **Diary.** Everything you have watched, newest first, grouped by month, with
  rewatches and episode ratings in line.
- **About page** with version, credits and licences.

### Improvements

- Frog ships its own typefaces, so text looks the same on every device.
- Every icon comes from one set, drawn at the same weight throughout.
- Dropdown menus match the rest of the app on every platform.
- Removing a title or unticking an episode offers an Undo.
- The command palette searches your library first, then pages, then actions
  such as Import and Export, and only then the wider TMDB catalogue. Results
  are grouped under headings.
- Home no longer repeats the same show in the hero and the row beneath it.
- A show whose next episode has not aired reads as Caught up, with the date
  the next one arrives.
- Several episodes of one show in the same week appear as a single entry.
- Pages show a skeleton while they load.
- Empty states say what to do next.
- Trailer artwork is drawn locally; nothing is requested from YouTube until
  you press play.
- Search ranks popular titles above obscure ones and labels each result Film
  or Show.
- The episode grid shows TMDB scores underneath your own ratings.
- The mobile tab bar includes Stats.

### Fixes

- Offline mode reports why it is unavailable rather than failing quietly. It
  needs a secure connection, which Tailscale Serve provides.
- Content no longer blanks out while scrolling. Background blur is off by
  default and adjustable in Settings.
- A row holding a single title keeps the normal card size.
- Release notes show complete entries, including keyboard shortcuts.
- The command palette input sits on one line.
- Counts read correctly in the singular.

## [0.2.0] — Ripple — 2026-10-24

A face, a home screen and a great deal to adjust.

### Highlights

- **Frog has a mascot.** A round green face with big eyes and pink cheeks,
  readable down to 16 pixels, used as the app icon and the favicon. Four
  variants to choose from: Classic, Sleepy, Popcorn and 3D glasses.
- **The Pond theme**, built from the mascot's colours, alongside Pond Light,
  Dark and AMOLED.
- **A home screen** separate from the library, opening on whatever you are in
  the middle of, with what to continue, what airs this week and what you added
  recently.
- **A command palette** on <kbd>Ctrl</kbd>+<kbd>K</kbd>.
- **A theme editor** with live preview, and themes you can save, duplicate,
  export and import.

### Improvements

- Continue watching uses wide artwork with the show logo, the next episode and
  a progress bar.
- Settings are stored on the server, so a phone and a laptop agree.
- Font, text size, corner radius, animation level, card style, poster size,
  grid spacing, card details, hero style, home row order, landing page,
  default status, half stars, date format and custom CSS are all adjustable.
- Spoiler protection blurs stills for episodes you have not watched.
- Search engines are asked not to index the instance.
- Remember this device on the sign-in screen, and sign-in is rate limited.
- A release notes page, with a marker in the navigation after an update.

### Fixes

- The stats page loads.
- Internal faults show a readable message and keep the detail in the log.
- Libraries containing a title you had never watched load correctly.
- Each page sets its own browser tab title.
- Episodes without artwork keep the row layout intact.
- Episodes that have not aired cannot be ticked or rated, and marking a season
  watched skips them.

## [0.1.0] — Spawn — 2026-10-24

The first release.

### Highlights

- **Track films and shows** from TMDB, with five statuses, ratings, favourites
  and notes.
- **Episode tracking**, one at a time or a season at once, with per-episode
  ratings and notes.
- **Title pages** coloured from their own artwork, with cast, crew, seasons,
  trailers, franchise order and where to stream.
- **Install it on a phone** and keep what you have opened available offline.

### Improvements

- Five rating formats, from 100 points to three smileys.
- A library wall with filters, sorting and search.
- Person pages that highlight what you have already seen.
- Runs as a single container, or directly with one script.

[0.3.0]: https://github.com/MihCons/Frog/releases/tag/v0.3.0
[0.2.0]: https://github.com/MihCons/Frog/releases/tag/v0.2.0
[0.1.0]: https://github.com/MihCons/Frog/releases/tag/v0.1.0
