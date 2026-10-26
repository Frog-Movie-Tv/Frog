#!/usr/bin/env bash
# Migrations have to apply two ways: to an empty database, and to one created
# by the previous release. The second is what every existing install does, and
# it is the one that breaks — sqlx refuses to start if an applied migration's
# checksum changed, so editing a shipped migration is caught here rather than
# on somebody's server.
set -euo pipefail

cd "$(dirname "$0")/.."
BIN="target/debug/frog"
[ -x "$BIN" ] || BIN="target/release/frog"
[ -x "$BIN" ] || { echo "build the binary first: cargo build"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Start the server against a database, wait for /healthz, stop it. Starting is
# what runs the migrations, so a failure to come up is a migration failure.
boot() {
	local db="$1" label="$2" port="$3"
	DATABASE_URL="sqlite:${db}?mode=rwc" \
	FROG_BIND="127.0.0.1:${port}" \
	FROG_STATIC_DIR="$WORK/static" \
	TMDB_API_KEY="" \
	RUST_LOG="frog=warn" \
		"$BIN" > "$WORK/$label.log" 2>&1 &
	local pid=$!

	for _ in $(seq 1 40); do
		if curl -fsS "http://127.0.0.1:${port}/healthz" > /dev/null 2>&1; then
			kill "$pid" 2>/dev/null || true
			wait "$pid" 2>/dev/null || true
			echo "  $label: migrations applied, /healthz answered"
			return 0
		fi
		kill -0 "$pid" 2>/dev/null || break
		sleep 0.5
	done

	kill "$pid" 2>/dev/null || true
	echo "  $label: FAILED"
	cat "$WORK/$label.log"
	return 1
}

echo "fresh database"
boot "$WORK/fresh.db" fresh 8101

# The newest release tag that is an ancestor of this commit.
PREV="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)"
if [ -z "$PREV" ]; then
	echo "no previous release tag found, so there is no upgrade to check"
	exit 0
fi

echo "database built by $PREV, then upgraded"
OLD="$WORK/old-migrations"
mkdir -p "$OLD"
# Only that release's migrations, so the database really is last version's.
git archive "$PREV" -- migrations 2>/dev/null | tar -x -C "$OLD" || {
	echo "could not read migrations from $PREV; skipping the upgrade check"
	exit 0
}

mv migrations migrations.current
cp -r "$OLD/migrations" migrations
boot "$WORK/upgrade.db" "as-of-$PREV" 8102
rm -rf migrations
mv migrations.current migrations

boot "$WORK/upgrade.db" upgraded 8103

echo "migrations apply cleanly both ways"
