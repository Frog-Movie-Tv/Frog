#!/usr/bin/env bash
# Pull the image this instance follows, back up first, and roll back if the new
# one does not come up healthy.
#
#   /opt/frog/deploy/update.sh production
#   /opt/frog/deploy/update.sh staging
#
# Nothing inbound is needed: the server pulls. Run by a systemd timer.
set -euo pipefail

INSTANCE="${1:?usage: update.sh production|staging}"
ROOT="${FROG_ROOT:-/opt/frog}"
DIR="$ROOT/$INSTANCE"
[ -d "$DIR" ] || { echo "no such instance: $DIR"; exit 1; }
cd "$DIR"

log() { echo "[$(date -Is)] $*"; }
# shellcheck source=ops-log.sh
. "$ROOT/deploy/ops-log.sh"

# Where the database lives, which is also where outcomes are recorded for
# Frog to read (see ops-log.sh).
VOLUME="$(docker volume inspect --format '{{.Mountpoint}}' \
	"${COMPOSE_PROJECT_NAME:-frog-$INSTANCE}_data" 2>/dev/null || true)"
record() { ops_record "$VOLUME" "$@"; }

# The image's own digest, short enough to read in a notification.
short() { printf '%s' "${1##*:}" | cut -c1-12; }

# The digest actually running, so rollback goes back to a specific image
# rather than to whatever the old tag points at by then.
current_digest() {
	docker inspect --format '{{index .RepoDigests 0}}' \
		"$(docker compose config --images | head -1)" 2>/dev/null || true
}

BEFORE="$(current_digest)"
log "$INSTANCE: running $BEFORE"

log "pulling"
if ! docker compose pull --quiet; then
	log "pull failed; staying on the current image"
	exit 1
fi

AFTER="$(current_digest)"
if [ -n "$BEFORE" ] && [ "$BEFORE" = "$AFTER" ]; then
	log "already up to date"
	exit 0
fi

# Always back up before letting a new binary touch the database: migrations
# run at startup and are not reversible.
log "backing up before the change"
"$ROOT/deploy/backup.sh" "$INSTANCE" pre-update || {
	log "backup failed; refusing to update"
	record update false "The backup before updating failed, so nothing was changed."
	exit 1
}

# The encrypted backup above cannot be read on this machine (the key is
# deliberately elsewhere), so keep a plain copy too, for the one case where
# rollback has to put the database back by itself. It sits on the same disk as
# the live database and is removed as soon as the update is healthy.
SNAPSHOT="$ROOT/backups/$INSTANCE/pre-update.db"
if [ -z "$VOLUME" ] || [ ! -f "$VOLUME/frog.db" ]; then
	log "could not find the database volume; refusing to update"
	exit 1
fi
rm -f "$SNAPSHOT"
(umask 077 && sqlite3 "$VOLUME/frog.db" ".backup '$SNAPSHOT'") || {
	log "could not snapshot the database; refusing to update"
	record update false "Could not copy the database before updating, so nothing was changed."
	exit 1
}

# The schema generation Frog keeps in SQLite's user_version. It only goes up
# when a release changes the schema so the one before it cannot run on it.
generation() { sqlite3 "$1" 'PRAGMA user_version;' 2>/dev/null || echo 0; }
GENERATION_BEFORE="$(generation "$SNAPSHOT")"

# Migrations run as the server starts, so healthy means migrated and serving.
wait_healthy() {
	for _ in $(seq 1 60); do
		if docker compose exec -T frog curl -fsS http://127.0.0.1:8080/healthz > /dev/null 2>&1; then
			return 0
		fi
		sleep 2
	done
	return 1
}

# Pin the compose file to the digest that was working a moment ago.
up_previous() {
	IMAGE_PIN="$BEFORE" docker compose -f compose.yml \
		-f <(printf 'services:\n  frog:\n    image: %s\n' "$BEFORE") up -d
}

log "starting the new image"
docker compose up -d --remove-orphans

log "waiting for /healthz"
if wait_healthy; then
	version="$(docker compose exec -T frog curl -fsS http://127.0.0.1:8080/healthz 2>/dev/null || true)"
	log "$INSTANCE is healthy: $version"
	record update true "Now running $(printf '%s' "$version" | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')."
	rm -f "$SNAPSHOT"
	docker image prune -f > /dev/null 2>&1 || true
	exit 0
fi

log "the new image never became healthy; rolling back"
record update false "The new image ($(short "$AFTER")) never became healthy."
docker compose logs --tail 50 frog || true

if [ -z "$BEFORE" ]; then
	log "no previous image recorded; restore the database by hand:"
	log "      $SNAPSHOT (plain), or $ROOT/deploy/restore.sh $INSTANCE latest"
	record rollback false "No previous image to go back to; it needs a person."
	exit 1
fi

# The previous binary starts on a schema the new one migrated, as long as
# migrations stay additive. Keeping that database keeps anything written since.
# A release that raised the schema generation is not additive: the previous
# image would pass /healthz (it only asks the database for SELECT 1) and fail
# on every title, so go straight to the snapshot.
docker compose stop frog > /dev/null 2>&1 || true
GENERATION_AFTER="$(generation "$VOLUME/frog.db")"
if [ "$GENERATION_AFTER" -gt "$GENERATION_BEFORE" ]; then
	log "the new image changed the schema past what the previous one reads ($GENERATION_BEFORE → $GENERATION_AFTER)"
	record rollback false "The update changed the database past what the previous image reads."
else
	up_previous
	if wait_healthy; then
		log "rolled back to $BEFORE; the newer migrations stay applied"
		record rollback true "Back on the previous image."
		rm -f "$SNAPSHOT"
		exit 1
	fi
	log "the previous image is not healthy either"
	record rollback false "The previous image did not start on the updated database."
	docker compose logs --tail 50 frog || true
fi

# Put the database back as it was before the update.
log "restoring the pre-update database"
docker compose down
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
mv "$VOLUME/frog.db" "$VOLUME/frog.db.failed-update-$STAMP"
rm -f "$VOLUME/frog.db-wal" "$VOLUME/frog.db-shm"
cp "$SNAPSHOT" "$VOLUME/frog.db"
chown 10001:10001 "$VOLUME/frog.db"
up_previous
if wait_healthy; then
	log "rolled back to $BEFORE with the pre-update database"
	log "the database it replaced is kept as frog.db.failed-update-$STAMP"
	record restore true "Put back the database from before the update; the previous image is running."
	rm -f "$SNAPSHOT"
	exit 1
fi

log "still not healthy after restoring; check: docker compose logs frog"
log "the pre-update database is kept at $SNAPSHOT"
record restore false "Still down after putting back the database; it needs a person."
exit 1
