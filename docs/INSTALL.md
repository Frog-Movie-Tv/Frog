# Installing Frog

[← back to the README](../README.md)

**You may not need this page.** If somebody has already set Frog up and sent
you an invitation link, open it, pick a name and a password, and you are done.
There is nothing to install: add it to your home screen and it behaves like any
other app.

This page is for running your own Frog. There are two ways:

| | |
| --- | --- |
| **On a server** | A public site anyone you invite can open, with HTTPS and updates that install themselves. [docs/DEPLOY-ORACLE.md](DEPLOY-ORACLE.md) does this on a free Oracle instance, click by click. |
| **On your own machine** | Docker on a laptop or a home server, reached over your own network or privately over Tailscale. That is what the rest of this page covers. |

Either way it costs nothing to run. A free TMDB key is the only credential.

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

## 2. Choose how to run it

Two options. **A needs nothing but Rust and Node, which you already have if you
cloned this and ran the tests.** B is the containerised version, and is what
you want once it is running all the time.

### Option A — run it directly (no Docker)

Skip to step 3, then use `scripts/run-local.ps1`. Nothing else to install.

### Option B — Docker Desktop

<https://www.docker.com/products/docker-desktop/> — free for personal use. On
Windows, let it use the WSL 2 backend when it offers. Or from a terminal:

```powershell
winget install Docker.DockerDesktop
```

Docker Desktop needs a sign-out or reboot after installing, and it must be
**running** (whale icon in the tray) before any `docker` command works. Check:

```bash
docker run --rm hello-world
```

If PowerShell says `The term 'docker' is not recognized`, it is not installed,
or it is installed but the terminal predates the install — open a new one.

---

## 3. Configure and start Frog

From the repository root:

```powershell
Copy-Item .env.example .env
```

Open `.env` and set at least:

```
TMDB_API_KEY=your_key_here
```

`MDBLIST_API_KEY` is optional: a free key from
[mdblist.com/preferences](https://mdblist.com/preferences) adds IMDb, Rotten
Tomatoes, Metacritic and other scores to title pages. Without one those scores
simply do not appear.

`FANART_API_KEY` is optional too: a free personal key from
[fanart.tv](https://fanart.tv/get-an-api-key/) fills in title logos TMDB has
none of, which matters most for anime kept from AniList. TMDB's own logo
always wins when it has one.

Leave `TMDB_API_KEY` empty and the app still starts, but search finds nothing
and no artwork loads — the header shows a **No TMDB key** badge until you set
it.

If you would rather not type credentials into a browser form, also set
`FROG_ADMIN_USERNAME` and `FROG_ADMIN_PASSWORD` (8 characters or more) and the
account is created at first boot. Otherwise leave them blank and the app shows
a setup screen the first time you open it.

### Start it — option A, no Docker

```powershell
.\scripts\run-local.ps1
```

It builds whatever is missing, then serves on <http://127.0.0.1:8080>. The
database goes in `./data/frog.db`. `-Port 9000` moves it, `-Rebuild` forces a
fresh build. Ctrl+C stops it.

This reads your `.env` for the TMDB key and everything else, but overrides
`DATABASE_URL` and `FROG_STATIC_DIR`, because the values in `.env.example` are
the paths *inside the container* and do not exist on the host.

### Start it — option B, Docker

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

## 4. Reach it from your phone, privately

Skip this if Frog is on a server with a hostname — it is already reachable, and
[the deployment guide](DEPLOY-ORACLE.md) covers that. This is for a Frog on
your own machine that you want on your phone without opening a port.

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

Both ways of running Frog bind to `127.0.0.1` only. To let Tailscale reach it,
use Tailscale Serve, which is the better option anyway because it gives you
real HTTPS:

```bash
tailscale serve --bg 8080
```

That publishes Frog at `https://<your-laptop>.<your-tailnet>.ts.net` inside
your tailnet, with a valid certificate. Then set `FROG_SECURE_COOKIES=1` in
`.env` and restart so the session cookie gets the `Secure` flag.

HTTPS is not optional if you want the app on your phone: a browser will only
install a web app, run its offline mode, and show new-episode notifications
over a secure connection. Over plain `http://` Frog still works, but About will
tell you offline mode is unavailable, and Settings → Notifications says it
needs HTTPS.

New-episode notifications go out from 18:00 on the day an episode airs. Set
`FROG_NOTIFY_HOUR` in `.env` (0–23, the server's local time) to change that.

Posters and stills are kept in the database so pages load without asking
TMDB again. The cache stays under `FROG_IMAGE_CACHE_MB` (512 by default):
pictures nobody has looked at for longest are dropped first, and fetched
again when a page needs them.

### Make the address say "frog"

The address comes from the machine's name in Tailscale, so rename the machine
and the address follows:

1. Open the [machines list](https://login.tailscale.com/admin/machines).
2. Find your laptop, open the **⋯** menu and choose **Edit machine name**.
3. Set it to `frog` and disable "auto-generate from OS hostname".

The address becomes `https://frog.<your-tailnet>.ts.net`, which is what the
phone will show once it is installed. Run `tailscale serve --bg 8080` again
afterwards so the certificate matches the new name.

> Do **not** run `tailscale funnel`. That publishes to the open internet.

If you would rather skip Serve, change the port line in `docker-compose.yml` to
`'8080:8080'` and use `http://<tailscale-ip>:8080`. You then lose HTTPS, so
leave `FROG_SECURE_COOKIES=0`.

### Install it on the phone

Open `https://frog.<your-tailnet>.ts.net` in Chrome on Android, then menu →
**Add to Home screen**. It installs with its own icon, opens without browser
chrome, and keeps anything you have already looked at available offline.

To check it worked, open **About** inside the app: offline mode should read
*Active*. If it says unavailable, you are on `http://` rather than `https://`
and Tailscale Serve is the fix.

---

## 5. Keep it running

Docker Desktop can start on login (Settings → General → *Start Docker Desktop
when you sign in*), and the container has `restart: unless-stopped`, so Frog
comes back by itself.

The laptop still has to be awake. On Windows, set *Power & battery → Screen and
sleep → When plugged in, put my device to sleep* to **Never** if you want the
phone to reach it at any hour.

This is the part a small server solves and a laptop cannot: it is awake when
you are not, so new episodes are picked up and notifications arrive on time.

---

## Updating

Frog tells you when there is a newer release: **Settings → Check for updates**
asks GitHub, which is the only request the app ever makes to anywhere but TMDB,
and only when you press the button. A dot appears next to Settings after you
update, and the **What's new** page explains what changed in plain language.

### One command

Running without Docker:

```powershell
git pull; .\scripts
un-local.ps1 -Rebuild
```

With Docker:

```bash
git pull && docker compose up -d --build
```

Your library is untouched either way: it lives in `data/frog.db` or in the
`frog-data` volume, neither of which the update goes near.

### Letting it update itself

Point `docker-compose.yml` at the published image rather than building from
this folder, and a release is a pull away:

```yaml
services:
  frog:
    image: ghcr.io/frog-movie-tv/frog:latest
    # ...everything else stays the same
```

```bash
docker compose pull && docker compose up -d
```

For a machine you are not sitting at, the deployment in
[docs/DEPLOY-ORACLE.md](DEPLOY-ORACLE.md) does this on a timer and does it
properly: it backs up first, waits for `/healthz`, and rolls back to the
previous image if the new one does not answer. `deploy/update.sh` is the whole
of it and works on any Docker host, not only Oracle.

**Not Watchtower.** It is unmaintained, and handing a container the Docker
socket hands it the machine. A timer running one script is less to trust and
easier to read.

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

## Developing

`scripts/run-local.ps1` serves the built app, which is what you want for
day-to-day use. While changing the frontend you want hot reload instead:

```bash
# terminal 1 — API on :8080
cd backend
cargo run

# terminal 2 — web dev server on :5173, proxying /api and /img to :8080
cd frontend
npm install
npm run dev
```

Then open <http://localhost:5173>.

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

Checked in October 2026, and revisited when Frog was actually deployed. The
conclusion changed: **Oracle Cloud Always Free is the one that works**, and it
is where Frog runs. A card is asked for to verify identity and is not charged,
which is the compromise worth making; nothing else came close.

[docs/DEPLOY-ORACLE.md](DEPLOY-ORACLE.md) has the whole thing, including the
traps — the instance shape that is actually available, the firewall rule that
leaves the site dark even when the cloud settings look right, and the keepalive
that stops an idle instance being reclaimed.

The others, for the record:

- **Fly.io** — the free tier is gone for new signups. A short trial, then a
  card is required before any usage.
- **Koyeb** — one free 512 MB service, but since February 2026 it requires a
  credit card with a hold placed on it.
- **Render** — the closest to free, 750 instance-hours a month, but services
  sleep after 15 minutes idle and take about a minute to wake. Sources
  disagree about whether a card is needed for Docker deployments.
- **Railway** — no longer a usable free tier; reports conflict, and it moved
  away from prepaid credit in early 2026.
- **Google Cloud Run** — generous free usage, but scales to zero, which is the
  wrong shape here: the background worker has to run on a schedule to notice
  new episodes, and it cannot while asleep.

A sleeping container is the thing to avoid, whoever hosts it. Oracle's free
instance is always on, so the worker actually runs.

Sources: [Fly.io vs Railway vs Render 2026](https://speedtesthq.com/compare/fly-io-vs-railway-vs-render),
[Render vs Railway vs Fly.io pricing compared](https://hostim.dev/blog/render-vs-railway-vs-fly-pricing/),
[Platforms with a real free tier in 2026](https://render.com/articles/platforms-with-a-real-free-tier-for-developers-in-2026),
[Free cloud deployment platforms compared](https://snapdeploy.dev/blog/free-cloud-deployment-platforms-2026-comparison).

---

## Troubleshooting

**`docker: The term 'docker' is not recognized`.** Docker Desktop is not
installed, or the terminal was open before you installed it. Either install it
(step 2) or use `.\scripts
un-local.ps1`, which needs no Docker at all.

**"No TMDB key" badge in the header.** `TMDB_API_KEY` is empty or did not reach
the process. Check `.env`; under Docker, `docker compose up -d` recreates the
container with the new value, and `run-local.ps1` just needs a restart.

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
