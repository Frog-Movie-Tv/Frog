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
ROOT="/opt/frog"
DIR="$ROOT/$INSTANCE"
[ -d "$DIR" ] || { echo "no such instance: $DIR"; exit 1; }
cd "$DIR"

log() { echo "[$(date -Is)] $*"; }

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
	exit 1
}

log "starting the new image"
docker compose up -d --remove-orphans

# Migrations run as the server starts, so healthy means migrated and serving.
log "waiting for /healthz"
healthy=0
for _ in $(seq 1 60); do
	if docker compose exec -T frog curl -fsS http://127.0.0.1:8080/healthz > /dev/null 2>&1; then
		healthy=1
		break
	fi
	sleep 2
done

if [ "$healthy" = "1" ]; then
	version="$(docker compose exec -T frog curl -fsS http://127.0.0.1:8080/healthz 2>/dev/null || true)"
	log "$INSTANCE is healthy: $version"
	docker image prune -f > /dev/null 2>&1 || true
	exit 0
fi

log "the new image never became healthy; rolling back"
docker compose logs --tail 50 frog || true

if [ -n "$BEFORE" ]; then
	# Pin the compose file to the digest that was working a moment ago.
	IMAGE_PIN="$BEFORE" docker compose -f compose.yml \
		-f <(printf 'services:\n  frog:\n    image: %s\n' "$BEFORE") up -d
	log "rolled back to $BEFORE"
else
	log "no previous image recorded; restore from a backup with restore.sh"
fi

# The database may already carry the new migrations, so say so loudly: a
# rollback of the binary is not a rollback of the schema.
log "NOTE: migrations from the failed image are still applied. If the old"
log "      image refuses to start, restore the pre-update backup:"
log "      $ROOT/deploy/restore.sh $INSTANCE latest"
exit 1
