# Deploying Frog on Oracle Cloud

Developer and operator documentation. Nothing here is user-facing.

> **Status: research only.** The facts below were checked on **2026-10-26** and
> are the basis for the `deploy/` folder. No deploy code has been written yet,
> and nothing is deployed. Three findings change the plan — see
> [What the research changes](#what-the-research-changes).

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

## Still to write

The click-by-click runbook — Oracle sign-up, VM creation, security list rules,
DuckDNS tokens, GHCR token, SSH keys — lands here with the `deploy/` folder,
once Parts 1 to 3 are done and the decisions above are made.
