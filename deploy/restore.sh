#!/usr/bin/env bash
# Put a backup back.
#
#   /opt/frog/deploy/restore.sh production latest
#   /opt/frog/deploy/restore.sh production /opt/frog/backups/production/frog-...age
#   /opt/frog/deploy/restore.sh production --list
#
# The age private key is needed and is deliberately not on this machine: paste
# it in when asked, or put it in a file and pass AGE_KEY_FILE=/path.
set -euo pipefail

INSTANCE="${1:?usage: restore.sh production|staging latest|<file>|--list}"
WHICH="${2:?usage: restore.sh production|staging latest|<file>|--list}"
ROOT="/opt/frog"
DIR="$ROOT/$INSTANCE"
STORE="$ROOT/backups/$INSTANCE"

log() { echo "[$(date -Is)] $*"; }
# shellcheck source=ops-log.sh
. "$ROOT/deploy/ops-log.sh"

if [ "$WHICH" = "--list" ]; then
	ls -1t "$STORE"/*.age 2>/dev/null || echo "no backups in $STORE"
	exit 0
fi

if [ "$WHICH" = "latest" ]; then
	ARCHIVE="$(ls -1t "$STORE"/*.age 2>/dev/null | head -1 || true)"
	[ -n "$ARCHIVE" ] || { log "no backups in $STORE"; exit 1; }
else
	ARCHIVE="$WHICH"
fi
[ -f "$ARCHIVE" ] || { log "no such backup: $ARCHIVE"; exit 1; }

log "restoring $INSTANCE from $(basename "$ARCHIVE")"
read -r -p "This replaces the current $INSTANCE database. Type the instance name to confirm: " ok
[ "$ok" = "$INSTANCE" ] || { log "cancelled"; exit 1; }

TMP="$(mktemp -d)"
chmod 700 "$TMP"
trap 'rm -rf "$TMP"' EXIT

if [ -n "${AGE_KEY_FILE:-}" ]; then
	age --decrypt --identity "$AGE_KEY_FILE" --output "$TMP/db.gz" "$ARCHIVE"
else
	echo "Paste the age private key, then Ctrl-D:"
	cat > "$TMP/key.txt"
	chmod 600 "$TMP/key.txt"
	age --decrypt --identity "$TMP/key.txt" --output "$TMP/db.gz" "$ARCHIVE"
fi
gunzip "$TMP/db.gz"

[ "$(sqlite3 "$TMP/db" 'PRAGMA integrity_check;')" = "ok" ] \
	|| { log "that backup does not pass an integrity check; stopping"; exit 1; }

cd "$DIR"
VOLUME="$(docker volume inspect --format '{{.Mountpoint}}' \
	"${COMPOSE_PROJECT_NAME:-frog-$INSTANCE}_data" 2>/dev/null || true)"
[ -n "$VOLUME" ] || { log "could not find the database volume"; exit 1; }

log "stopping $INSTANCE"
docker compose down

# Keep what was there, in case the backup turns out to be the wrong one.
if [ -f "$VOLUME/frog.db" ]; then
	mv "$VOLUME/frog.db" "$VOLUME/frog.db.replaced-$(date -u +%Y%m%dT%H%M%SZ)"
fi
rm -f "$VOLUME/frog.db-wal" "$VOLUME/frog.db-shm"
cp "$TMP/db" "$VOLUME/frog.db"
chown 10001:10001 "$VOLUME/frog.db"

log "starting $INSTANCE"
docker compose up -d

for _ in $(seq 1 60); do
	if docker compose exec -T frog curl -fsS http://127.0.0.1:8080/healthz > /dev/null 2>&1; then
		log "restored and healthy"
		log "the database it replaced is kept beside it as frog.db.replaced-*"
		ops_record "$VOLUME" restore true "Restored by hand from $(basename "$ARCHIVE")."
		exit 0
	fi
	sleep 2
done

log "it did not come back healthy; check: docker compose logs frog"
ops_record "$VOLUME" restore false "A restore by hand from $(basename "$ARCHIVE") did not come back healthy."
exit 1
