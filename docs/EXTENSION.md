# The browser extension

Frog's extension logs what you watch on any site, on its own. It has no list
of supported sites: it reads what every page says about itself, asks your
Frog what that is, and logs it once you are 90% in.

It works in Firefox and in Chrome, and in the browsers built on them (Zen,
Brave, Edge), from one source in `extension/`.

## Install it

In Frog, open **Settings → Connections**. Under **Browser extension** are
two buttons, which always give the newest version.

**Firefox**

1. Choose **Firefox** and open the file it downloads.
2. Allow the install, and allow Frog on every site when asked. It needs them
   all to find players; see [what leaves the browser](#what-leaves-the-browser).

It updates itself from then on. Mozilla signs it as unlisted: it installs
from the file and is never published on addons.mozilla.org.

**Chrome**

1. Choose **Chrome** and unzip what it downloads somewhere it can stay.
2. Open `chrome://extensions` (`brave://extensions` in Brave), switch on
   **Developer mode**, choose **Load unpacked** and pick the unzipped folder.
3. Pin Frog to the toolbar.

Chrome only updates extensions from its Web Store by itself, which costs a
fee. For a new version, download again, unzip over the same folder, and
press the reload arrow on Frog's card. The token, settings and site rules
stay.

The same files are on every
[release page](https://github.com/Frog-Movie-Tv/Frog/releases), as
`frog-extension-firefox.xpi` and `frog-extension-chrome.zip`.

## Connect it

1. In Frog, open **Settings → Connections → Browser extension** and choose
   **New token**. Name it after the browser.
2. Copy the token. Frog shows it once and keeps only its hash.
3. In the extension's settings (Frog in the toolbar → **Settings**), enter
   your Frog's address and paste the token.

A token can read your library and log and undo watches, nothing else: it
cannot change settings, your account, two-factor, invites or anything an
administrator does. Every other part of Frog answers it with 403. Revoke it in
the same place in Frog and it stops at once.

## What it does

When a video plays, the extension looks at the biggest one that is actually
playing and at least three minutes long, in the page or in any of its frames,
inside open shadow roots too. Shorter videos are trailers, clips and ads, and
a pre-roll ad that hands its player to the episode starts a new sitting, so
the ad never counts.

At 90% (Settings → Logging changes it), or when the video ends:

- **Sure** (an id on the page, or a site rule): it logs, and a small card
  offers **Undo** for 10 seconds.
- **Likely** or **a guess** (a name only): the card asks first.
- **Nothing to go on**: the toolbar button shows `?` and its popup asks
  **What are you watching?**, with a search filled from the page title.

**Always ask first** in Settings makes every match ask.

A log is the same as ticking the episode on its page in Frog: the show joins
your library as Watching, finishes when its last episode does, and the watch
reaches the diary. The same episode from the same browser within six hours is
one watch. If Frog cannot be reached, the watch waits in the extension and is
sent when Frog is back; nothing is dropped. The popup shows what is waiting.

The toolbar badge says what it thinks is playing: `S2E7` or `Film`, green
when sure, amber when likely, grey for a guess.

## What it can and cannot detect

It reads, from the page and from every frame's own address:

| Signal                                      | Examples                                             |
| ------------------------------------------- | ---------------------------------------------------- |
| TMDB and IMDb ids in an address             | `?tmdb=95396&s=2&e=7`, `/tv/95396/2/7`, `tt11280740` |
| schema.org JSON-LD                          | `Movie`, `TVEpisode` with its season and series      |
| OpenGraph and `video:*` tags                | `og:title`, `og:type = video.episode`                |
| The media session                           | what the system's media controls show                |
| Text in the title, headings and the address | `S02E07`, `2x07`, `Season 2 Episode 7`, `Episode 7`  |

Ids beat text. An id read only from the shape of an address (`/tv/95396/…`)
might be the site's own number, so it is only sure once the name Frog finds
agrees with the page. An episode number with no season is counted from the
start of the show, the way anime sites number them, and Frog places it in the
right season.

It cannot see:

- **DRM players that draw nothing into the page** in some browsers, or a
  player inside a closed shadow root.
- **Live streams**, which have no end to reach.
- **Ads stitched into the episode's own stream**, which look like the episode.
- **Pages with no id and no name of any kind**: those always ask.

## Site rules

When you confirm a match from the card or the popup, or pick another one, the
extension remembers for that site where the title's id was, where the season
and episode were, and which name the site uses for that title. The next time
on that site the match is sure and logs on its own.

Rules are keyed by the site's domain, the one you were on. No domain is
written into the extension. In Settings → **Site rules** you can read, edit
(as JSON), delete (with Undo), export and import them. When a site moves to a
new address, **Copy to…** gives the new domain the same rule.

**Detect on this site** in the popup switches a site off; Settings →
**Ignored sites** lists them.

## What leaves the browser

Only what resolve and log need, and only to your own Frog: TMDB and IMDb ids,
the kind, season and episode numbers, a name, a year and the page's title.
Never an address, never your history, never anything about other tabs. There
are no analytics. The content script does nothing until a video plays, and the
token goes only to the address you connected.

## For developers

```
cd extension
npm ci
npm run dev            # Chromium, reloads as you edit; dev:firefox for Firefox
npm run build          # .output/chrome-mv3; build:firefox for .output/firefox-mv3
npm run check          # types
npm test               # the detection rules, no browser
npx playwright test    # the built extension in Chromium against e2e/fixtures
```

| File                               | What it does                                                                     |
| ---------------------------------- | -------------------------------------------------------------------------------- |
| `src/entrypoints/content.ts`       | Every frame: waits for a video to play, samples it, reports what the frame shows |
| `src/entrypoints/background.ts`    | Each tab's picture, resolving, when to log or ask, the queue, toasts, badge      |
| `src/lib/signals.ts`               | Pages to clues to the signals resolve takes. Pure, tested in `tests/`            |
| `src/lib/rules.ts`                 | Site rules: learning, applying, import and export                                |
| `src/lib/playback.ts`              | Which video counts, sittings, when it is watched                                 |
| `src/lib/queue.ts`                 | What an answer from Frog means for a waiting watch                               |
| `src/entrypoints/popup`, `options` | The popup and Settings, in Frog's own colours                                    |

The end-to-end tests serve fixture pages on 8072 (as `localhost` and as
`127.0.0.1`, two origins, for the cross-origin player) and a stand-in Frog on
8071 (`e2e/server.mjs`). One fixture per pattern: ids in the address, ids only
in a cross-origin iframe, JSON-LD only, a title only, a video in a shadow root,
a pre-roll ad, and a page with nothing to go on. `npm run videos` remakes the
two tiny test videos with ffmpeg.

Playwright loads the extension into its own Chromium build. Where that build
will not start (on Windows, a missing runtime library says "side-by-side
configuration is incorrect"), `E2E_CHANNEL=msedge npx playwright test` runs
the same suite in Edge.

The server side is `backend/src/tokens.rs` (tokens and `TokenAuth`, the only
extractor that reads a bearer token), `backend/src/extension.rs` (resolve,
log, undo, the popup's list) and `backend/src/routes/ext.rs`.

## Releases

Each release builds both, signs the Firefox build as unlisted with
`web-ext sign`, and attaches `frog-extension-firefox.xpi`,
`frog-extension-chrome.zip` and `updates.json` to the GitHub release
(`.github/workflows/release-please.yml`). The names never change, so
`releases/latest/download/…` is always the newest: Settings links there, and
the Firefox build's `update_url` reads `updates.json` from there
(`scripts/updates-json.mjs` writes it from the signed build's manifest;
`FROG_EXTENSION_UPDATES` points a fork elsewhere). Signing needs the
repository secrets `AMO_JWT_ISSUER` and `AMO_JWT_SECRET`, an API key from
[addons.mozilla.org's developer hub](https://addons.mozilla.org/developers/addon/api/key/);
without them the release still gets the Chromium zip and says the Firefox
build was not signed (and Firefox installs see no update). The extension's version follows Frog's, which
release-please writes into `extension/package.json`.
