#!/usr/bin/env bash
# DEVELOPMENT ONLY. Docker is how Frog actually runs; this is for working on it.
#
#   ./scripts/dev.sh            build, seed two demo accounts, run on :8080
#   ./scripts/dev.sh --fresh    throw the dev database away first
#   ./scripts/dev.sh --no-seed  skip seeding
#
# Everything lives in data/dev.db, which is gitignored and disposable.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo"

fresh=0
seed=1
for arg in "$@"; do
	case "$arg" in
		--fresh) fresh=1 ;;
		--no-seed) seed=0 ;;
		*) echo "unknown option: $arg"; exit 1 ;;
	esac
done

if [ ! -f .env ]; then
	cp .env.example .env
	echo "Created .env from .env.example. Put your free TMDB key in it."
fi

echo "==> backend"
( cd backend && cargo build )

echo "==> frontend"
( cd frontend && { [ -d node_modules ] || npm ci; } && npm run build )

if [ "$seed" = "1" ]; then
	if [ "$fresh" = "1" ]; then
		echo "==> seeding two demo accounts"
		node scripts/seed-demo.mjs --force
	elif [ ! -f data/dev.db ]; then
		echo "==> seeding two demo accounts"
		node scripts/seed-demo.mjs
	else
		echo "==> data/dev.db exists; leaving it alone (--fresh to replace)"
	fi
fi

echo "==> http://127.0.0.1:8080  (Ctrl+C to stop)"
DATABASE_URL="sqlite:./data/dev.db?mode=rwc" \
FROG_BIND="127.0.0.1:8080" \
FROG_STATIC_DIR="./frontend/build" \
RUST_LOG="${RUST_LOG:-frog=debug,tower_http=warn}" \
	exec backend/target/debug/frog
