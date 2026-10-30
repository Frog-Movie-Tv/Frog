# Deploying Frog on Oracle Cloud

Developer and operator documentation. Nothing here is user-facing.

> **Status: deployed.** Frog runs on a free Oracle instance, behind Caddy,
> updating itself from the newest release image. The facts below were checked
> on **2026-10-26** and are the basis for everything in `deploy/`. Re-check
> anything older than a few months: free tiers move.

## What this actually runs on

Amsterdam had no A1 capacity when the instance was created, so production runs
on a **VM.Standard.E2.1.Micro**: 2 vCPU, 954 MB, **amd64**. Always free and
always available, where A1 rarely is.

Frog is comfortable there — the container sits under 30 MB and the machine
idles at about a quarter of its memory — but three things follow:

- **Production only.** Staging needs another instance; `install.sh` leaves it
  out unless asked.
- **The keepalive holds no memory.** Oracle's memory test is A1-only, so on
  this shape it does a short CPU and network touch instead.
- **Images are built natively per architecture.** amd64 is what this machine
  needs; arm64 is still published for a future A1, on an ARM runner rather
  than through emulation.

The research below still describes the A1, because that is what the free tier
documents and what to take if capacity ever appears.

## Researched facts

Free tiers and tooling change, so every claim here carries its source and the
date it was checked. Re-check anything older than a few months.

### Oracle Always Free compute (checked 2026-10-26)

The Ampere A1 allowance **has been cut in half**, as suspected. Oracle's own
reference page now says Always Free covers:

> the first 1,500 OCPU hours and 9,000 GB hours per month for free for VM
> instances using the VM.Standard.A1.Flex shape

and describes that as **2 OCPUs and 12 GB of memory** total, allocatable
flexibly across one or two instances. The previous allowance was 3,000
OCPU-hours and 18,000 GB-hours (4 OCPU / 24 GB).

Secondary reports say the documentation changed around 15 June 2026 with no
announcement, and that enforcement began around 18 August 2026, with excess
Ampere instances at risk of termination. Oracle's own materials were
inconsistent for a while, and some indexed pages still show the old figures, so
trust the live reference page over anything else.

**2 OCPU / 12 GB is still enough** for production and staging Frog side by
side: Frog is one small Rust binary and a SQLite file, and the heaviest thing
either instance does is talk to TMDB.

Sources: [Oracle, Always Free Resources](https://docs.oracle.com/en-us/iaas/Content/FreeTier/resourceref.htm)
(official) · [Linuxiac](https://linuxiac.com/oracle-quietly-cuts-free-tier-ampere-a1-resources-in-half/) ·
[terminalbytes](https://terminalbytes.com/oracle-cloud-free-tier-changes-2026/) ·
[cnelecar](https://www.cnelecar.com/blog/oracle-always-free-arm-limits-cut-2026/)

### Idle-instance reclamation (checked 2026-10-26)

Oracle's reference page states the test exactly:

> Idle Always Free compute instances may be reclaimed by Oracle. Oracle will
> deem virtual machine and bare metal compute instances as idle if, during a
> 7-day period, the following are true:
> - CPU utilization for the 95th percentile is less than 20%
> - Network utilization is less than 20%
> - Memory utilization is less than 20% *(applies to A1 shapes only)*

**All three must hold at once** for an instance to count as idle, and the
memory criterion applies to A1 shapes — which is what we are running. That
makes the keepalive much cheaper than it first looks: holding **one** metric
above 20% is enough.

- Memory is the cheapest lever. 20% of 12 GB is about 2.4 GB. Reserving a few
  gigabytes costs no CPU and no bandwidth.
- Chasing the CPU threshold instead would mean 95th-percentile CPU above 20%,
  so roughly 8+ hours of real load a week, burning CPU for nothing.

So the keepalive should hold memory, not spin the CPU.

The official page does **not** say what happens to a reclaimed instance.
Secondary sources say Oracle emails first and stops rather than deletes the
instance; that is **unverified**, so treat reclamation as destructive and rely
on the nightly backups.

Sources: [Oracle, Always Free Resources](https://docs.oracle.com/en-us/iaas/Content/FreeTier/resourceref.htm)
(official) · [aiskill.market](https://aiskill.market/blog/keeping-your-free-oracle-vps-from-being-reclaimed)
(secondary, for the email/stop behaviour)

### A1 capacity in Amsterdam (checked 2026-10-26)

**Not confirmed either way.** The search turned up nothing specific to
`eu-amsterdam-1`; the well-known "Out of host capacity" guides are older and
talk about Frankfurt and Singapore. Assume it can happen and plan for the
retry rather than a one-shot console launch.

What the guides agree on, and what we will do:

- Try each availability domain in the region, and look for the **Always Free
  Eligible** label in the placement section before launching.
- Save the instance settings as an Oracle Resource Manager stack, then retry
  the Terraform apply on a loop until capacity appears. This keeps the exact
  settings instead of re-entering them.
- Or loop the OCI CLI launch command across availability domains.

Most guides also recommend upgrading to Pay As You Go for a larger capacity
pool. **We are not doing that** — Always Free only, per the brief. It is worth
knowing that this is the usual advice, so neither of us is tempted by it when
the console says no.

Sources: [Oracle, Launching a Linux Instance](https://docs.oracle.com/en-us/iaas/Content/GSG/Tasks/launchinginstance.htm)
(official) · [hitrov, OCI CLI retry](https://hitrov.medium.com/resolving-oracle-cloud-out-of-capacity-issue-and-getting-free-vps-with-4-arm-cores-24gb-of-a3d7e6a027a8) ·
[oci-arm-host-capacity](https://github.com/oeufmeister/oci-arm-host-capacity) ·
[oci-instance-creator](https://github.com/mowirth/oci-instance-creator)

### The Ubuntu iptables trap (checked 2026-10-26)

Opening 80 and 443 in the Oracle security list is **not enough**. Oracle's
Ubuntu images ship their own host firewall that rejects the traffic, so the
site stays unreachable and the cause looks like DNS or Caddy.

- The image carries a catch-all reject in `/etc/iptables/rules.v4`, reported as
  `-A INPUT -j REJECT --reject-with icmp-host-prohibited`.
- **Appending** ACCEPT rules does nothing, because the reject matches first.
  Insert them above it by line number: check with
  `sudo iptables -L INPUT -n --line-numbers`, then
  `sudo iptables -I INPUT <n> -p tcp --dport 443 -j ACCEPT`.
- Persist with `sudo netfilter-persistent save`, then read the file back — the
  save is reported to write duplicate rules.
- Typical symptom: one port appears to work while the other fails with "No
  route to host".

`install.sh` will do this and verify it, rather than leaving it to a checklist
step someone forgets.

Sources: [Discourse Meta, self-hosting on Oracle Cloud](https://meta.discourse.org/t/self-hosting-for-free-with-oracle-cloud/139184/15) ·
[ithelp.ithome.com.tw](https://ithelp.ithome.com.tw/articles/10286871) ·
[Let's Encrypt community](https://community.letsencrypt.org/t/page-not-accessible-after-installing-letsencrypt-with-certbot/135801)

### DuckDNS, Let's Encrypt and Caddy (checked 2026-10-26)

- **Wildcard certificates need the DNS-01 challenge**, which in Caddy means a
  custom build with a DNS provider module (`caddy-dns/duckdns`). I could not
  confirm that module's current maintenance status.
- **We do not need wildcards.** Two plain hostnames — one for production, one
  for staging — are satisfied by HTTP-01 on stock Caddy, with no plugin, no
  custom image and no DuckDNS token in the server's environment. That is the
  simpler and more durable choice, so that is the plan.
- Let's Encrypt allows **50 certificates per registered domain per week**.
  One source argues `duckdns.org` is on the Public Suffix List, which would
  give each subdomain its own bucket rather than sharing one; **verify at
  publicsuffix.org** before relying on it. Either way, two hostnames with
  normal renewals are nowhere near the limit, and renewals are exempt from the
  failure limits.
- Use Caddy's **staging** ACME endpoint while testing the deploy, or a few
  broken attempts can block issuance for up to a week.

Sources: [Caddy, Automatic HTTPS](https://caddyserver.com/docs/automatic-https)
(official) · [Caddy community, DuckDNS wildcard](https://caddy.community/t/setting-up-caddy-with-lets-encrypt-ssl-with-duckdns-to-serve-multiple-services-running-as-docker-containers/26361) ·
[codewithkarani, DDNS and the Public Suffix List](https://www.codewithkarani.com/blog/ddns-subdomain-letsencrypt-rate-limit-public-suffix-list)

### GHCR and GitHub Actions on the free plan (checked 2026-10-26)

- **GHCR is free today, even for private images.** GitHub's billing page says
  "Container image storage and bandwidth for the Container registry is
  currently free", with at least one month's notice before that changes. The
  500 MB / 1 GB Free-plan Packages quota applies to other package types, not
  container images.
- **Actions minutes: 2,000 per month** for private repositories on the Free
  plan. Public repositories are free on standard GitHub-hosted runners.
- **Free arm64 runners are public-repository only.** `ubuntu-24.04-arm` and
  friends are generally available at 4 vCPU / 16 GB, free and unlimited on
  public repositories — and **workflows in private repositories using these
  labels will fail**.

Sources: [GitHub, billing for Packages](https://docs.github.com/en/billing/managing-billing-for-your-products/managing-billing-for-github-packages/about-billing-for-github-packages)
(official) · [GitHub, billing for Actions](https://docs.github.com/en/billing/managing-billing-for-your-products/managing-billing-for-github-actions/about-billing-for-github-actions)
(official) · [GitHub, supported runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
(official) · [GitHub Changelog, arm64 runners GA](https://github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/)

### Release automation (checked 2026-10-26)

**release-please is current and the plan stands.** The repository carries no
archive or deprecation notice, and the npm package is at **17.11.2**. It
generates the changelog, the version bumps and the GitHub release from
Conventional Commits, as a release PR.

Sources: [googleapis/release-please](https://github.com/googleapis/release-please) ·
[npm registry, release-please](https://registry.npmjs.org/release-please/latest)

## What the research changes

Three things do not fit the plan as written. They need a decision before the
deploy code is worth writing.

### 1. The repository is private, so "multi-arch on free runners" does not work

`Frog-Movie-Tv/Frog` is private. That collides with the zero-cost rule in two
places:

- Native arm64 runners are **public-repository only**, so an `arm64` image has
  to be built under QEMU emulation instead — several times slower.
- Every one of those minutes comes out of the **2,000 per month**. CI on every
  push and pull request, plus end-to-end tests, plus an emulated multi-arch
  image build on every merge to main, will not comfortably fit. Running out
  mid-month is fixable only by paying, which the brief rules out.

Options, best first:

1. **Make the repository public.** Frog is already MIT licensed and presented
   as open source, so this costs nothing in secrecy. It buys unlimited standard
   runner minutes, native arm64 runners and genuinely free CI. `.env` is
   already gitignored; the history needs a check for anything secret first.
2. **Stay private and trim CI.** Build arm64 under QEMU on tags only, never on
   pull requests, and keep per-PR CI to the cheap checks. Workable, but the
   minute budget stays a live concern.
3. **Stay private and build arm64 on the Oracle VM.** It is an ARM machine
   already. Free, but the production image would no longer be the one CI built
   and tested, which Part 3 explicitly requires.

### 2. Nothing here can configure GitHub itself

`gh` is not installed and there is no token in the environment, so branch
protection, required reviews, labels, the Projects board, repository settings
and GitHub Releases cannot be set from here. Either install the GitHub CLI and
sign in, or do those parts in the web UI — `docs/CONTRIBUTING.md` will spell
out each click.

### 3. Oracle is tighter than the plan assumed

2 OCPU / 12 GB rather than 4 / 24. Production and staging still fit, but the
margin for anything else on the box is small, and the keepalive should hold
memory rather than burn CPU (see above).

## The runbook

Everything in `deploy/` is written; what follows is the part only an owner can
do, because it needs logins. Work top to bottom.

### 1. The Oracle account

1. Sign up at <https://signup.cloud.oracle.com>. A card is asked for to verify
   identity; **Always Free** does not charge it. Pick **Netherlands Northwest
   (Amsterdam)** as the home region — it cannot be changed later.
2. Wait for the "your account is ready" email.
3. **Never click "Upgrade to Pay As You Go."** Not for capacity, not for the
   idle rule, not for the banner that suggests it.

### 2. The machine

Compute → Instances → **Create instance**.

| Field | Value |
| --- | --- |
| Image | Canonical Ubuntu 24.04 |
| Shape | **Ampere, VM.Standard.A1.Flex** |
| OCPUs | 2 |
| Memory | 12 GB |
| Boot volume | 50 GB is plenty |
| SSH keys | paste **both** owners' public keys, one per line |

Check the availability domain says **Always Free Eligible** before creating.

**If A1 capacity is not there**, the fallback is `VM.Standard.E2.1.Micro`:
2 vCPU and 1 GB, always free, and always available. Frog runs fine on it —
the container sits around 25 MB — but two things change:

- **Run production only.** Staging will not fit beside it comfortably. Point
  staging at a second micro, or go without until an A1 frees up.
- **The keepalive works differently.** Oracle's memory test applies to A1
  shapes only, so holding memory there buys nothing and takes RAM the machine
  needs. `keepalive.sh` detects the shape and does a short CPU and network
  touch on a timer instead.

**If it says "Out of host capacity":** that is normal and not your fault. Try
each availability domain in turn. If all three refuse, save the configuration
as a stack (Create → *Save as stack*) and retry the apply on a loop, or script
the launch with the OCI CLI. Do not upgrade the account to get capacity.

### 3. Let the internet in — both halves

**Security list.** Networking → Virtual Cloud Networks → your VCN → the public
subnet → its security list → **Add Ingress Rules**:

| Source | Protocol | Port |
| --- | --- | --- |
| 0.0.0.0/0 | TCP | 80 |
| 0.0.0.0/0 | TCP | 443 |

**The host firewall.** This is the half everyone misses: Oracle's Ubuntu image
rejects the traffic itself, so the site stays unreachable with a correct
security list. `install.sh` fixes it and verifies it — you do not need to do
anything by hand, but if you are debugging later:

```bash
sudo iptables -L INPUT -n --line-numbers   # look for a REJECT above your rules
```

### 4. DuckDNS

1. Sign in at <https://www.duckdns.org> with GitHub.
2. Create two subdomains, one for each site, e.g. `frogtv` and `frogtv-staging`.
3. Point both at the machine's public IP.
4. The token on that page is not needed: two plain hostnames use the HTTP
   challenge, so there is no DNS plugin and no token on the server.

Check from your own machine before going further:

```bash
dig +short frogtv.duckdns.org
```

### 5. Install

```bash
ssh ubuntu@<the public IP>
git clone https://github.com/Frog-Movie-Tv/Frog.git
sudo ./Frog/deploy/install.sh
```

It installs Docker, sqlite3, age, fail2ban and unattended upgrades, fixes the
firewall, lays out `/opt/frog`, writes the systemd units, and starts the
keepalive. It prints what to fill in next.

### 6. Fill in the three env files

```bash
sudo nano /opt/frog/production/.env    # TMDB_API_KEY, TZ
sudo nano /opt/frog/staging/.env       # the same, IMAGE_TAG=main
sudo nano /opt/frog/caddy/.env         # the two DuckDNS names, your email
sudo nano /opt/frog/backup.env         # AGE_RECIPIENT
```

Generate the backup key pair **on your own computer, not the server**:

```bash
age-keygen -o frog-backup.key
```

Keep `frog-backup.key` somewhere safe. Paste only the `age1…` public line into
`backup.env`. The server can then write backups it cannot read back.

**AniList (optional).** Register a client at
<https://anilist.co/settings/developer> with the redirect URL
`https://<production domain>/api/anilist/callback`. It has to match exactly, and
a client has only one, so staging needs a client of its own (or goes without
AniList). Then add all three to that instance's `.env` and restart it:

```bash
ANILIST_CLIENT_ID=12345
ANILIST_CLIENT_SECRET=…
ANILIST_REDIRECT_URI=https://<production domain>/api/anilist/callback
```

With any of the three missing, AniList simply does not appear in Settings.

### 7. The backup repository

Backups are encrypted on the server, kept there for 30 days, and pushed to a
private repository, which is the copy that survives losing the machine. The
server reaches it with a deploy key of its own, `/opt/frog/backup-key`, that
`install.sh` creates and prints.

1. Create a **private**, empty repository in the organisation:
   `gh repo create Frog-Movie-Tv/Frog-Backups --private`.
   Frog-Movie-Tv has deploy keys switched off by default, and only an
   organisation owner can allow them: the organisation's **Settings**, the
   **Deploy keys** policy (searching the settings for "deploy keys" finds it).
   Until then the repository's Deploy keys page says "Disabled by
   Frog-Movie-Tv".
2. In that repository: **Settings → Deploy keys → Add deploy key**. Paste the
   output of `sudo cat /opt/frog/backup-key.pub`, and tick **Allow write
   access**.
3. On the server, connect it. This clones it with the key, proves the key can
   push, and records it in `backup.env`:

   ```bash
   sudo /opt/frog/deploy/backup-repo.sh git@github.com:Frog-Movie-Tv/Frog-Backups.git
   ```

4. Make a backup and see it arrive in the repository:

   ```bash
   sudo /opt/frog/deploy/backup.sh production manual
   ```

   It should log "pushed to the backup repository", the file should be under
   `production/` in the repository, and Administration → Server should say
   "Backup copied off the server."

A backup leaves out TMDB's cached answers and the cached posters: they are
most of the database (133 MB of 151 for a library of 611 titles) and Frog
fetches them again by itself. That library backs up in about 3 MB.

**The repository still grows**, because git keeps every backup it was ever
sent, even after the 30-day clean-up removes them from the working tree.
Somewhere around a gigabyte, start its history over with only the last 30
days:

```bash
cd /opt/frog/backup-repo
sudo git checkout -q --orphan fresh && sudo git commit -qm "backups: the last 30 days"
sudo git branch -D main && sudo git branch -m main
sudo GIT_SSH_COMMAND="ssh -i /opt/frog/backup-key -o IdentitiesOnly=yes" git push -f origin main
```

### 8. Start it

```bash
# A private image needs a token with read:packages. A public one does not.
# sudo docker login ghcr.io -u <your github user>

cd /opt/frog/caddy && sudo docker compose up -d
sudo /opt/frog/deploy/update.sh production
sudo /opt/frog/deploy/update.sh staging
sudo systemctl start frog-update@production.timer frog-update@staging.timer
sudo systemctl start frog-backup@production.timer frog-backup@staging.timer
```

Use the Let's Encrypt **staging** CA for the first attempt (there is a
commented line in `caddy/.env`), confirm HTTPS answers, then switch back and
`docker compose up -d --force-recreate`. A handful of failed attempts against
the real CA can block issuance for a week.

### 9. Check it

```bash
curl -fsS https://frogtv.duckdns.org/healthz
curl -fsS https://frogtv-staging.duckdns.org/healthz
sudo /opt/frog/deploy/keepalive.sh status
```

Then open production in a browser and create the owner account. **Do this
immediately**: the setup screen is open to whoever reaches it first, and it
closes for good once an account exists.

### 10. Test the restore, before you need it

A backup nobody has restored is a rumour. Restore the copy in the backup
repository, the one that survives losing the machine, into a throwaway
instance beside production, so production is never touched:

```bash
# A throwaway instance: its own container and volume, not reachable from outside.
sudo mkdir -p /opt/frog/restoretest
sudo cp /opt/frog/production/compose.yml /opt/frog/production/.env /opt/frog/restoretest/
sudo sed -i 's/^COMPOSE_PROJECT_NAME=.*/COMPOSE_PROJECT_NAME=frog-restoretest/' /opt/frog/restoretest/.env
cd /opt/frog/restoretest && sudo docker compose up -d

# The newest backup, fetched from the repository rather than this disk. A
# fresh clone gives every file the same time, so pick by name: names start
# with the date.
sudo rm -rf /tmp/frog-backups-check
sudo GIT_SSH_COMMAND="ssh -i /opt/frog/backup-key -o IdentitiesOnly=yes" \
  git clone -q git@github.com:Frog-Movie-Tv/Frog-Backups.git /tmp/frog-backups-check
ARCHIVE="$(ls -1 /tmp/frog-backups-check/production/*.age | sort | tail -1)"
echo "$ARCHIVE"

# Asks you to type "restoretest", then to paste the age private key and press Ctrl-D.
sudo /opt/frog/deploy/restore.sh restoretest "$ARCHIVE"

# The same accounts, library and watches as production:
for v in frog-production_data frog-restoretest_data; do
  echo "$v: $(sudo sqlite3 "/var/lib/docker/volumes/$v/_data/frog.db" \
    'SELECT COUNT(*) FROM app_user; SELECT COUNT(*) FROM library_entry; SELECT COUNT(*) FROM watch_event;' | tr '\n' ' ')"
done

# And gone again.
cd /opt/frog/restoretest && sudo docker compose down -v
cd / && sudo rm -rf /opt/frog/restoretest /tmp/frog-backups-check
```

Production may count a few more if somebody used Frog since the backup.

**After losing the machine**, the same steps rebuild it: a new machine
through steps 2–6, a new `backup-key` added to the repository as a deploy key
(read access is enough), then clone the repository and run
`restore.sh production <the newest file>` before starting the timers.
`restore.sh` keeps whatever database it replaces beside the new one as
`frog.db.replaced-*`.

## How updates work

Nothing is open to the internet except 80 and 443, and SSH by key. The machine
**pulls**:

- A systemd timer runs `update.sh` every 15 minutes per instance.
- It compares the running image digest with the tag it follows. Same digest,
  nothing to do.
- Otherwise: back up, start the new image, wait for `/healthz`, and on failure
  roll back to the previous digest and say so.
- Migrations run as the server starts, so healthy means migrated and serving.
  A rollback of the binary is **not** a rollback of the schema: the previous
  image starts on the newer schema (it ignores migrations it does not know,
  which is why migrations stay additive). If it still fails its health check,
  `update.sh` puts back a plain snapshot it took just before the update,
  keeps the database it replaced as `frog.db.failed-update-*`, and starts the
  previous image again. The snapshot sits beside the encrypted backups only
  while an update is in flight.
- A release that changes the schema past what the previous image reads
  raises the schema generation (SQLite's `user_version`; the catalog release
  is generation 1). When a failed update raised it, `update.sh` skips the
  previous image on the new schema, which would pass `/healthz` and break on
  every page, and goes straight to the snapshot.
  `deploy/tests/update-rollback.sh` checks each path in CI.
- The server runs its own copy of these scripts in `/opt/frog/deploy`, made
  by `install.sh`. A change to `update.sh` reaches it only when copied:
  `sudo cp deploy/*.sh /opt/frog/deploy/` from a checkout of the release.
- Every outcome (backup, update, rollback, restore) is also written to
  `ops.jsonl` in the data volume. Administration shows the latest of each
  under Server, and administrators get a notification the first time one
  fails. A site that is down cannot send one, so a failure that leaves Frog
  unreachable is still only in the timer's journal:
  `journalctl -u frog-update@production`.

Staging follows `:main`, so merging a pull request puts it on staging within a
quarter of an hour. Production follows the newest release tag, so merging the
release-please pull request is what puts a change in front of people.

## If something is wrong

| Symptom | Where to look |
| --- | --- |
| Site unreachable, DNS fine | the host firewall (section 3) |
| Certificate never issued | `docker compose logs caddy`; check the rate limit |
| Updates stop happening | `systemctl status frog-update@production.timer` |
| Instance was reclaimed | `keepalive.sh status`; restore from a backup |
| Backups stopped | `journalctl -u frog-backup@production` |
