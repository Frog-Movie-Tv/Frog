# Contributing to Frog

Developer documentation. Nothing here belongs in the README, which is for
people who want to *use* Frog.

Frog has two owners, [Mihai Constantin](https://github.com/MihCons) and
[Zjef Schaeken](https://github.com/zschaeken), who work on it in parallel.
Everything below exists so that two people (and two Claude Code sessions) can
change the same codebase without standing on each other.

## Get it running

One command, from a clean clone:

```powershell
.\scripts\dev.ps1
```

```bash
./scripts/dev.sh
```

It creates `.env` from `.env.example` if you have none, builds the backend and
the frontend, seeds two demo accounts, and serves on <http://127.0.0.1:8080>.

| | |
| --- | --- |
| `mihai` / `demo-password-mihai` | owner |
| `zjef` / `demo-password-zjef` | admin |

Two accounts rather than one, so isolation between them is visible: sign in as
each in two browser profiles and you should see two different libraries.
Everything lives in `data/dev.db`, which is gitignored and disposable — pass
`-Fresh` / `--fresh` to start it over.

Put your free [TMDB key](https://www.themoviedb.org/settings/api) in `.env` to
get metadata and artwork. Without one Frog still runs, the seeded libraries are
just empty.

**Docker is how Frog actually runs.** `scripts/dev.ps1`, `scripts/dev.sh`,
`scripts/run-local.ps1` and the `verify-*.mjs` scripts are development and
test tooling only. What ships is the image CI builds.

## Branches and pull requests

`main` is protected. Nothing is pushed to it directly.

1. Branch: `feat/short-name` for a feature, `fix/short-name` for a fix. Also
   `docs/`, `chore/`, `ci/`, `refactor/`, `test/` where they fit.
2. Commit with [Conventional Commits](https://www.conventionalcommits.org)
   (see below) — release-please reads them.
3. Open a pull request against `main`. The template lists what to check.
4. CI has to be green and **the other owner has to approve**. CODEOWNERS
   requests their review automatically.
5. Merge. An administrator can override the review requirement in a genuine
   emergency; it is there for outages, not for impatience.

## Commit messages

```
<type>(<optional scope>): <what changed, lower case, no full stop>

<why, and anything a reader needs that the diff does not show>
```

Types that reach the changelog: `feat`, `fix`, `perf`, `refactor`, `docs`.
Types that do not: `test`, `build`, `ci`, `chore`, `style`.

While Frog is on `0.x`, **a feature bumps the patch**
(`bump-patch-for-minor-pre-major`), so `feat` on 0.4.0 produces 0.4.1. A
breaking change (`!` or a `BREAKING CHANGE:` footer) bumps the minor. Do not
add one lightly.

## Releases

You never edit a version or `CHANGELOG.md` by hand, which is the point: two
people editing a changelog is a merge conflict every time.

- **`CHANGELOG.md` is generated.** release-please keeps a release pull request
  open that bumps `version.txt`, `backend/Cargo.toml` and
  `frontend/package.json`, and writes the changelog from the commit messages.
  Merging it tags the release.
- **`RELEASE-NOTES.md` is written by hand, in Frog's voice.** It is what the
  *What's new* page inside the app shows, and a generated list of commit
  subjects is not something you show a person. Short, neutral, complete
  sentences, no implementation detail. The `/release` skill has the rules.
- **The release pull request needs its notes.** CI fails while
  `RELEASE-NOTES.md` has no `## [x.y.z]` section for the version in
  `version.txt`. Before merging release-please's pull request, push a commit to
  it that turns the Upcoming section into that version's section.
- **Staging follows `main`**, **production follows the latest release tag.**
  So merging a pull request puts it on staging, and merging the release pull
  request puts it in front of people.

### Cutting a big release

While Frog is on `0.x`, every change bumps the patch — `0.4.5`, `0.4.6`, and
so on — because `bump-patch-for-minor-pre-major` is on. That is right for most
weeks and wrong when something large lands, and release-please cannot tell the
difference on its own. You tell it.

Put a `Release-As:` footer on any commit that reaches `main`:

```
feat(accounts): invitations, profiles and per-account settings

Release-As: 0.5.0
```

The next release pull request will be that version, whatever the commits
since would otherwise have produced. An empty commit works just as well if
the work has already landed:

```bash
git commit --allow-empty -m "chore: release 0.5.0" -m "Release-As: 0.5.0"
```

**What a big release needs beyond the version**, none of which is automatic:

1. **Write the notes yourself.** The bot drafts a section from commit
   subjects so a release is never blocked, but a drafted section reads like a
   list of commits, because it is one. Replace it in `RELEASE-NOTES.md` with
   what a person can now do. The draft is marked with an HTML comment saying
   so.

   That section becomes three things: the *What's new* page inside the app,
   the body of the GitHub release, and what anyone following a link reads.
   `CHANGELOG.md` stays as the commit list, for people working on Frog.
2. **Take the next codename** from the list in `ROADMAP.md`, and put it in the
   heading, on the About page and in the About test in `e2e/smoke.spec.ts`.
3. **Read the README as a stranger.** A big release usually makes something in
   it untrue. The accounts release made four things untrue.
4. **Regenerate the screenshots** if any page in them changed:
   `cd frontend && npm run screenshots` (needs the TMDB key, about 8 minutes).
5. **Check the image was published.** Merging the release pull request tags
   the version and builds it; the tag existing is not proof the image does:

   ```bash
   docker manifest inspect ghcr.io/frog-movie-tv/frog:<version> > /dev/null      && echo published || echo "not published"
   ```

6. **Check it arrived.** Production pulls within fifteen minutes. The About
   page should show the new version.

## Migrations

- One numbered or timestamped `.sql` file in `backend/migrations/`, embedded at
  compile time.
- **New migrations get a timestamped name**: `202610261730_add_thing.sql`.
  Digits only before the first `_`: sqlx reads that part as a number, so
  `20261026T1730_…` fails to compile. The old `0001`–`0008` files stay as they
  are. Timestamps mean two branches
  never pick the same number and silently reorder when both merge.
- **Never edit a migration that has been applied anywhere.** sqlx stores a
  checksum and refuses to start if it changed, which takes a running instance
  down on its next deploy. Add another migration instead. The pre-commit hook
  refuses a change to an existing migration file.
- CI applies every migration to an empty database *and* to one built by the
  previous release tag, which is what every existing install does.

## Accounts and isolation

Frog holds several accounts. Everything personal — library, ratings, episode
ticks, diary, shelves, settings, notifications, sessions, two-factor — carries
a `user_id`, and only `title`, `episode`, `title_person`, the TMDB and image
caches and the push keys are shared.

**Every query that reads or writes personal data must name whose it is.**
`backend/tests/scoping.rs` reads the source and fails if one does not; if a
statement really is shared, add it to `ALLOWED` there with the reason, so the
exception is a decision somebody made rather than an oversight.

New API routes go behind the `Auth` extractor (or `AdminAuth`), and get added
to the auth-wall test in `frontend/e2e/smoke.spec.ts`.

## Checks

The hooks run these for you. Install them once per clone:

```bash
git config core.hooksPath .githooks
```

`pre-commit` runs the fast ones (rustfmt, prettier, and eslint on the staged
files) and refuses a `.env`, a database file, or an edit to an applied
migration. `pre-push` runs clippy, the backend tests and svelte-check. `FROG_SKIP_HOOKS=1` skips them once, for a
work-in-progress commit on your own branch.

By hand:

```bash
cd backend && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
cd frontend && npm run format:check && npm run check && npm run build
cd frontend && npx playwright test        # needs a current cargo build --release
cd frontend && npm run audit              # every page, both viewports, console errors
```

**The end-to-end suite prefers `backend/target/release/frog`.** If you changed
the backend and did not rebuild release, Playwright silently tests yesterday's
binary. Rebuild, or delete the release binary so it falls back to debug.

## Secrets

- `TMDB_API_KEY` lives in `.env`, which is gitignored. Never print it, never
  paste it into an issue, never commit it.
- On a server, secrets live in `/opt/frog/.env`, `chmod 600`.
- Everything configurable is an environment variable, documented in
  `.env.example`. Nothing is configured by editing code.

## Layout

```
backend/            Rust: axum, sqlx (SQLite), tokio
  src/routes/       one module per area; thin — parse, call the store, serialise
  src/store.rs      reads and writes of a user's own data
  src/auth.rs       sessions, roles, the Auth and AdminAuth extractors
  src/tmdb/         TMDB client, cache, and the JSON to view mapping
  src/jobs/         background worker (sessions, cache pruning, refresh, alerts)
  migrations/       timestamped .sql files, embedded at compile time
  scripts/          check-migrations.sh
  tests/            api.rs (HTTP surface, isolation), scoping.rs (the guard)
frontend/           SvelteKit + Svelte 5 runes, adapter-static SPA, Vite
  src/lib/          components, stores, import parsers
  scripts/          icons, changelog, screenshots, audit, verify-*.mjs
  e2e/              Playwright, desktop and Pixel 7
scripts/            dev.ps1, dev.sh, seed-demo.mjs, run-local.ps1
deploy/             compose files, Caddyfile and the server scripts
docs/               INSTALL, CONTRIBUTING, DEPLOY-ORACLE
```

## Issues and planning

Bug reports and feature requests have templates. The board is **Backlog → In
progress → Review → Done**; move your card when you pick something up, so the
other owner does not start the same thing.
