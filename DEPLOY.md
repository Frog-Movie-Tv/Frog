# Running Frog

Target setup: Frog runs in Docker on your laptop, and your phone reaches it
over Tailscale. Nothing is exposed to the internet, nothing costs money, and no
credit card is involved anywhere.

---

## 1. Get a TMDB key (free)

1. Make an account at <https://www.themoviedb.org/signup>.
2. Go to <https://www.themoviedb.org/settings/api> and request an API key.
   Choose **Developer**. For "type of use" pick personal/non-commercial; the
   form accepts a placeholder URL for a personal project.
3. Copy either the **API Key (v3 auth)** or the **API Read Access Token (v4)**.
   Frog accepts both and works out which one you gave it.

No card, no paid tier. TMDB asks only that you show attribution, which the app
does in the footer of every page.

---

## 2. Install Docker Desktop

<https://www.docker.com/products/docker-desktop/> — free for personal use.
On Windows let it use the WSL 2 backend when it offers.

Check it works:

```bash
docker run --rm hello-world
```

---

## 3. Configure and start Frog

From the repository root:

```bash
cp .env.example .env
```

Open `.env` and set at least:

```
TMDB_API_KEY=your_key_here
```

If you would rather not type credentials into a browser form, also set
`FROG_ADMIN_USERNAME` and `FROG_ADMIN_PASSWORD` (8 characters or more) and the
account is created at first boot. Otherwise leave them blank and the app shows
a setup screen the first time you open it.

Build and run:

```bash
docker compose up -d --build
```

The first build takes a few minutes because it compiles Rust from scratch.
Later builds reuse the dependency layer and take seconds.

Check it:

```bash
curl http://127.0.0.1:8080/api/health
```

You should see `{"ok":true,...,"tmdb_configured":true,...}`. Then open
<http://127.0.0.1:8080> and create your account.

`.env` is in `.gitignore` and must stay that way. `.env.example` is the file
that gets committed.

---

## 4. Reach it from your phone with Tailscale

Tailscale's Personal plan is free and needs no card. It puts your laptop and
your phone on the same private WireGuard network, so the phone can reach Frog
with no port forwarding and nothing open to the internet.

1. Make an account at <https://login.tailscale.com/start>.
2. Install Tailscale on the laptop and sign in.
3. Install the Tailscale app on Android and sign in with the same account.
4. On the laptop, find its Tailscale address:

```bash
tailscale ip -4
```

Note that `docker-compose.yml` binds the port to `127.0.0.1` only. To let
Tailscale reach it, use Tailscale Serve, which is the better option anyway
because it gives you real HTTPS:

```bash
tailscale serve --bg 8080
```

That publishes Frog at `https://<your-laptop>.<your-tailnet>.ts.net` inside
your tailnet, with a valid certificate. Then set `FROG_SECURE_COOKIES=1` in
`.env` and restart (`docker compose up -d`) so the session cookie gets the
`Secure` flag.

> Do **not** run `tailscale funnel`. That publishes to the open internet.

If you would rather skip Serve, change the port line in `docker-compose.yml` to
`'8080:8080'` and use `http://<tailscale-ip>:8080`. You then lose HTTPS, so
leave `FROG_SECURE_COOKIES=0`.

### Install it on the phone

Open the `https://…ts.net` address in Chrome on Android, then menu →
**Add to Home screen**. It installs as a standalone app with its own icon,
offline shell and cached posters.

---

## 5. Keep it running

Docker Desktop can start on login (Settings → General → *Start Docker Desktop
when you sign in*), and the container has `restart: unless-stopped`, so Frog
comes back by itself.

The laptop still has to be awake. On Windows, set *Power & battery → Screen and
sleep → When plugged in, put my device to sleep* to **Never** if you want the
phone to reach it at any hour.

---

## Backups

Everything lives in one Docker volume: your library, the TMDB cache and every
cached poster.

```bash
docker run --rm -v frog_frog-data:/data -v "$PWD:/backup" \
  debian:bookworm-slim tar czf /backup/frog-backup.tar.gz -C /data .
```

Restore:

```bash
docker run --rm -v frog_frog-data:/data -v "$PWD:/backup" \
  debian:bookworm-slim tar xzf /backup/frog-backup.tar.gz -C /data
```

Only `frog.db` matters; the caches rebuild themselves. Phase 2 adds a JSON
export from inside the app, which is the backup you can actually read.

---

## Running without Docker

Useful while developing.

```bash
# terminal 1 — API on :8080
cd backend
cargo run

# terminal 2 — web dev server on :5173, proxying /api and /img to :8080
cd frontend
npm install
npm run dev
```

Then open <http://localhost:5173>. For a production-shaped run:

```bash
cd frontend && npm run build
cd ../backend && FROG_STATIC_DIR=../frontend/build cargo run --release
```

---

## Tests

```bash
cd backend && cargo test          # 22 unit + integration tests, no network
cd frontend && npm run check      # TypeScript + Svelte
cd frontend && npm run test:e2e   # Playwright, desktop + Pixel 7 viewports
```

The end-to-end suite builds nothing by itself: run `npm run build` in
`frontend/` and `cargo build` in `backend/` first. It drives the real binary
against a throwaway database with no TMDB key, so it never touches the network.

---

## About cloud hosting

You asked me to check before recommending a host. I did, in October 2026, and
the honest answer is that self-hosting is the only option that meets your rule
cleanly:

- **Fly.io** — the free tier is gone for new signups. A short trial, then a
  card is required before any usage.
- **Koyeb** — one free 512 MB service, but since February 2026 it requires a
  credit card with a hold placed on it.
- **Render** — the closest to free, 750 instance-hours a month, but services
  sleep after 15 minutes idle and take about a minute to wake. Sources
  disagree about whether a card is needed for Docker deployments.
- **Railway** — no longer a usable free tier; reports conflict, and it moved
  away from prepaid credit in early 2026.
- **Google Cloud Run / Oracle Cloud** — genuinely generous free usage, but both
  require a card on file.

A sleeping container is also the wrong shape for this app: the background
worker needs to run on a schedule to pick up new episodes, and it cannot do
that while scaled to zero. Your laptop plus Tailscale costs nothing, keeps your
watch history on hardware you own, and the worker actually runs.

Sources: [Fly.io vs Railway vs Render 2026](https://speedtesthq.com/compare/fly-io-vs-railway-vs-render),
[Render vs Railway vs Fly.io pricing compared](https://hostim.dev/blog/render-vs-railway-vs-fly-pricing/),
[Platforms with a real free tier in 2026](https://render.com/articles/platforms-with-a-real-free-tier-for-developers-in-2026),
[Free cloud deployment platforms compared](https://snapdeploy.dev/blog/free-cloud-deployment-platforms-2026-comparison).

---

## Troubleshooting

**"No TMDB key" badge in the header.** `TMDB_API_KEY` is empty or did not reach
the container. Check `.env`, then `docker compose up -d` to recreate it.

**The setup screen does not appear, it asks me to sign in.** An account already
exists. Delete the volume to start over: `docker compose down -v`. That erases
your library too.

**Signed out immediately after signing in.** `FROG_SECURE_COOKIES=1` while
serving over plain HTTP: the browser drops the cookie. Set it to `0`, or put
Tailscale Serve in front.

**Posters do not load.** They are proxied through `/img/…`, which needs a
session. If the app shows content but no images, check the browser console for
401s and sign in again.

**Phone cannot reach it.** Confirm both devices are on the tailnet
(`tailscale status`), and that Docker is actually running the container
(`docker compose ps`).
