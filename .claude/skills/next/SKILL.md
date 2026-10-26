---
name: next
description: Pick up the next item on Frog's roadmap and build it end to end — backend, frontend, tests, audit — then update ROADMAP.md. Use when the owner says next, continue, keep going, carry on, pick up where we left off, or asks what is left to build.
---

# Building the next roadmap item

## 1. Get oriented

- Read `CLAUDE.md` (rules, gotchas, conventions) and `ROADMAP.md`.
- Unless the owner named a specific item, take the **top item under Next**.
- Run `git status` and `git log --oneline -5` to see whether anything is half
  done from a previous session. If there are uncommitted changes, find out
  what they are before building on top of them.
- Tell the owner in one line which item you are taking.

## 2. Build it

- Backend first when the feature needs data: migration (next number in
  `backend/migrations/`), store or route code, and a test in
  `backend/tests/api.rs` that would have failed without the change.
- Then the frontend, following the conventions in CLAUDE.md: `Icon`, `Select`,
  `Empty`, undo toasts, a `<title>` per route, product-voice copy.
- Keep it zero cost. If the item turns out to need something paid, stop and
  say so instead of building around it.

## 3. Verify

```bash
cd backend && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
cd frontend && npm run check && npm run build
cd frontend && npx playwright test
cd frontend && npm run audit
```

Add or extend a Playwright test for anything user-visible. The audit must stay
clean. Stop the owner's server before `cargo build --release`.

If the change is visible, run the app and look at it in the browser pane at
both desktop and phone width before calling it done.

## 4. Record it

- In `ROADMAP.md`: remove the item from **Next**, add a line under an
  **Unreleased** heading in **Shipped** (create it if missing), and add any
  follow-ups you discovered to **Next** or **Known issues**.
- Add a line under the `## Upcoming — … —` heading at the top of
  `CHANGELOG.md`, in product voice.

## 5. Report

Say what was built, how it was verified, and what is left. Offer to commit, or
to carry on with the next item, or to ship a release with `/release`. Do not
commit or push unless asked.

## If the session is getting long

Finish the piece in hand, make sure ROADMAP.md and CHANGELOG.md reflect it,
and suggest a fresh session for the next item. A new session that reads
CLAUDE.md and ROADMAP.md picks up exactly where this one stopped.
