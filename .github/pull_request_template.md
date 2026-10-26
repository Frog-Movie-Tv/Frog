## What this changes

<!-- One or two sentences. What can a person do now that they could not before,
or what stopped being broken? -->

## Why

<!-- The reason, if it is not obvious from the above. Link an issue if there is
one. -->

## How to check it

<!-- The commands you ran, and anything a reviewer should click through. -->

- [ ] `cd backend && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test`
- [ ] `cd frontend && npm run check && npm run build`
- [ ] `cd frontend && npx playwright test` (needs a current `cargo build --release`)
- [ ] `cd frontend && npm run audit`

## Checklist

- [ ] Commits follow Conventional Commits, so release-please can read them.
- [ ] Any new migration has a timestamped filename and has never been edited
      after being applied.
- [ ] Any query touching personal data names its owner; `cargo test` includes
      the guard for this.
- [ ] Any new API route sits behind `Auth` (or `AdminAuth`) and is listed in
      the auth-wall test in `frontend/e2e/smoke.spec.ts`.
- [ ] No secrets, `.env` files or generated output are in the diff.
- [ ] User-facing copy is in Frog's voice: short, neutral, no implementation
      detail, no roadmap.
