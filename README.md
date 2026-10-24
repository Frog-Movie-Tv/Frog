<div align="center">

# Frog

**A self-hosted personal cinema.** Your watch history, in something that feels
like a streaming app rather than a database.

Rust · axum · SQLite · SvelteKit · installable PWA · one Docker image · zero cost

</div>

---

Frog tracks the films and shows you watch. It pulls everything it knows from
TMDB — posters, backdrops, logo art, cast, seasons, episodes, trailers,
collections, and where to stream in Belgium — caches all of it locally, and
builds a library you own.

It is built for exactly one person: you. No accounts to manage, no sharing, no
public pages, nothing leaves your machine except cached requests to TMDB.

## Status — phase 1 (alpha)

What works today:

- **First-run setup and sign-in.** Single account, Argon2 hashing, session
  cookies. Every route is behind it.
- **Search all of TMDB**, with quick-add straight from the results grid.
- **Track anything** — watching, completed, watchlist, on hold, dropped — with
  a rating, a favourite flag and notes.
- **Episode checkmarks** per episode, or a whole season at once, with
  per-episode ratings and notes. Status follows what you watch: start a show
  and it moves to watching, finish the last episode and it completes.
- **Library poster wall** with status chips, genre and type filters, text
  search, six sorts, and a continue-watching rail on top.
- **Title pages** with page colours extracted from the poster, logo-art hero,
  cast and crew linked to person pages, seasons with checkmarks, an episode
  rating heatmap, trailers, franchise watch order, and Belgian watch providers.
- **Person pages** with a filmography that highlights what you have seen.
- **Score formats** — 100 point, 10 point decimal, 10 point, 5 stars, 3
  smileys. Ratings are stored 0–100, so switching never loses precision.
- **Themes** — dark, AMOLED, light — plus poster density and follow-the-poster
  accent.
- **Installable PWA** with an offline shell and cached posters.

Phases 2–4 (home screen, calendar, push notifications, imports, theme editor,
smart shelves, command palette, recommendations, scrobbling, Wrapped) are
planned in [ARCHITECTURE.md](ARCHITECTURE.md).

## Quick start

```powershell
Copy-Item .env.example .env   # then put your free TMDB key in it
.\scriptsun-local.ps1       # no Docker needed
```

Or, with Docker Desktop installed and running:

```bash
docker compose up -d --build
```

Either way, open <http://127.0.0.1:8080> and create your account.
Full instructions, including reaching it from your phone over Tailscale, are in
[DEPLOY.md](DEPLOY.md).

## Zero cost, kept honest

Every dependency and service here is free, with no card required: Rust, SQLite,
SvelteKit, Docker Desktop (personal use), GitHub Actions on a public repo,
Tailscale Personal, and TMDB's free API key.

Two things were left out because they would have cost money, as you asked me to
flag:

- **Plex webhook scrobbling needs Plex Pass.** Phase 4 will support Jellyfin's
  free webhook plugin and a generic endpoint instead; Tautulli covers Plex for
  free through the same endpoint.
- **No cloud host is recommended.** Checked October 2026: Fly.io, Koyeb,
  Railway and Render no longer offer an always-on container tier that is both
  free and card-free. Reasoning and sources in [DEPLOY.md](DEPLOY.md).

There is no AI or LLM anywhere in this app. Recommendations are built from
TMDB's own recommendation and similar endpoints, weighted by your ratings.

## Layout

```
backend/     Rust: axum API, SQLite, TMDB client, image proxy, worker
frontend/    SvelteKit SPA, PWA, design tokens
Dockerfile   one image: frontend built, then baked in next to the binary
```

## Development

```bash
cd backend  && cargo run                   # API on :8080
cd frontend && npm install && npm run dev   # web on :5173, proxied to the API
```

Tests:

```bash
cd backend  && cargo test        # 22 tests, no network
cd frontend && npm run check     # types
cd frontend && npm run test:e2e  # Playwright, desktop + mobile
```

## Attribution

This product uses the TMDB API but is not endorsed or certified by TMDB. Data
and images are from [The Movie Database](https://www.themoviedb.org).

## Licence

MIT.
