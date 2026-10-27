---
name: release
description: Ship a new version of Frog — bump the version, write the changelog entry in product voice with a pond codename, run every check, regenerate screenshots, then commit, tag and push. Use when the owner says release, ship, cut a version, tag it, or "push this as 0.x".
---

# Releasing Frog

Work through these in order. Stop and report if any check fails; do not tag a
release with a red test.

> **Most releases need none of this.** Merging to `main` keeps a release pull
> request open, and merging that one tags the release, builds the image and
> deploys it. This skill is for the parts a person has to do: deciding that a
> release is a big one, and writing what people read.

## 1. Decide the version

- `0.x` bumps the patch for everything, so a large release has to say so.
  Put `Release-As: 0.5.0` in a commit footer on `main` and the next release
  pull request becomes that version. `docs/CONTRIBUTING.md` has the detail.
- Take the next codename from `ROADMAP.md`. It goes in the `RELEASE-NOTES.md`
  heading, beside the version on the About page
  (`frontend/src/routes/about/+page.svelte`) **and** in the About test in
  `frontend/e2e/smoke.spec.ts`. All three have gone stale before.
- Never edit `version.txt`, `Cargo.toml`, `package.json` or `CHANGELOG.md`
  by hand. They are generated.

## 2. Write the changelog entry

Work in progress collects under the heading at the top of `RELEASE-NOTES.md`:

```
## Upcoming — Lily Pad —
```

The in-app release notes skip that heading, because
`frontend/scripts/changelog.mjs` only reads headings with a bracketed version.
To release, turn it into a versioned heading in exactly this shape, keeping the
codename, and fold its entries into the sections below:

```
## [0.4.0] — Lily Pad — 2026-11-02

One line saying what this release gives the user.

![Short description](docs/screenshots/home-desktop-pond.png)

### Highlights
- **Bold lead-in.** One or two short, complete sentences.

### Improvements
- Short, complete sentence.

### Fixes
- Short, complete sentence about what now works.
```

Voice rules — these are strict:
- Describe what the user gets: "New: import from Letterboxd." Not how it works.
- Never write "alpha", "beta", "finally", "phase", "now properly", or describe
  how something used to be broken ("Errors are human now" is wrong;
  "Error messages say what went wrong" is right).
- No developer terms: no SQL, endpoints, components, refactors.
- Short, complete sentences. British spelling, matching the rest of the file.
- Above the new release, start a fresh `## Upcoming — <next codename> —`
  heading with the line `In progress.` under it. The codename order is in
  ROADMAP.md.
- At the bottom, add a link reference for the new version only, for example
  `[0.4.0]: https://github.com/Frog-Movie-Tv/Frog/releases/tag/v0.4.0`. The Upcoming
  heading has no brackets, so it gets no link reference.

## 3. Verify everything

In Git Bash, `export PATH="$HOME/.cargo/bin:$PATH"` first. Stop the owner's
running server before the release build (Windows locks `frog.exe`).

```bash
cd backend && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
cd backend && cargo build --release
cd frontend && npm run check && npm run build
cd frontend && npx playwright test
cd frontend && npm run audit
```

The audit must report no server errors, no browser problems, and a registered
service worker on both viewports. If any new route was added, make sure it is
in the auth-wall test in `e2e/smoke.spec.ts` and in the audit's route list.

## 4. Screenshots

```bash
cd frontend && npm run screenshots
```

Takes about eight minutes; run it in the background. Look at
`docs/screenshots/home-desktop-pond.png` and any page this release changed
before committing — an empty or broken screenshot is worse than an old one.
If a new page deserves a place in the README gallery, add it.

## 5. Update the roadmap

Move what shipped into **Shipped** in `ROADMAP.md`, bump the current/next
release line, and leave anything unfinished at the top of **Next**.

## 6. Commit, tag, push — only if the owner asked

Check nothing secret or generated is staged:

```bash
git status --porcelain | grep -iE "\.env$|-data|node_modules|target/" || echo clean
```

Then commit with a message that leads with `Release 0.x.0 <Codename>: …`,
explains the notable changes for a developer reader, and ends with the
attribution line from the session's system reminder. Tag and push:

```bash
git tag -a v0.4.0 -m "Frog 0.4.0 Lily Pad"
git push origin main --follow-tags
```

## 7. Tell the owner

- What shipped, in a short list.
- What was not finished (and that it is now in ROADMAP.md).
- That the GitHub Release needs creating by hand from the tag at
  `https://github.com/Frog-Movie-Tv/Frog/releases/new?tag=v0.4.0`, with the
  RELEASE-NOTES.md section as the notes — unless `gh` is installed and signed in.
- Restart their server: `.\scripts\run-local.ps1`.
