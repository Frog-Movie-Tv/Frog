#!/usr/bin/env bash
# An encrypted copy of one instance's database, pushed to a private repository.
#
#   /opt/frog/deploy/backup.sh production            nightly, from the timer
#   /opt/frog/deploy/backup.sh production pre-update before an update
#
# SQLite is copied with .backup, not cp: copying the file while the server is
# writing gives you a torn database that restores into nothing. Encryption is
# age with a public key, so the server can write backups but cannot read them
# back — the private key lives with the owners, not on the machine.
set -euo pipefail

INSTANCE="${1:?usage: backup.sh production|staging [label]}"
LABEL="${2:-nightly}"
ROOT="/opt/frog"
DIR="$ROOT/$INSTANCE"
STORE="$ROOT/backups/$INSTANCE"
KEEP_DAYS=30

log() { echo "[$(date -Is)] $*"; }
# shellcheck source=ops-log.sh
. "$ROOT/deploy/ops-log.sh"

# Where the database lives, which is also where the outcome is recorded for
# Frog to read (see ops-log.sh).
VOLUME="$(docker volume inspect --format '{{.Mountpoint}}' \
	"${COMPOSE_PROJECT_NAME:-frog-$INSTANCE}_data" 2>/dev/null || true)"
WHAT="Nightly backup"
[ "$LABEL" = "pre-update" ] && WHAT="Backup before updating"
fail() {
	log "$1"
	ops_record "$VOLUME" backup false "$WHAT failed: $2"
	exit 1
}
# Anything else that stops the script part way is a failure too.
trap 'ops_record "$VOLUME" backup false "$WHAT stopped at line $LINENO of backup.sh."' ERR

# shellcheck source=/dev/null
[ -f "$ROOT/backup.env" ] && . "$ROOT/backup.env"
[ -n "${AGE_RECIPIENT:-}" ] \
	|| fail "set AGE_RECIPIENT in /opt/frog/backup.env" "no encryption key is set up."

cd "$DIR"
mkdir -p "$STORE"
PUSHED=local

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
NAME="frog-$INSTANCE-$STAMP-$LABEL.db"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A consistent snapshot while the server keeps serving. .backup is SQLite's
# own online copy; it understands WAL, which cp does not — copying the file
# under a running writer gives you a torn database that restores into nothing.
# The image has no sqlite3, so this runs on the host against the volume.
command -v sqlite3 > /dev/null || fail "sqlite3 is not installed" "sqlite3 is not installed."

if [ -z "$VOLUME" ] || [ ! -f "$VOLUME/frog.db" ]; then
	log "could not find the database volume for $INSTANCE"
	exit 1
fi

log "snapshotting $INSTANCE"
sqlite3 "$VOLUME/frog.db" ".backup '$TMP/$NAME'"

# Checked before it is trusted, so a corrupt database does not quietly replace
# thirty days of good ones.
if [ "$(sqlite3 "$TMP/$NAME" 'PRAGMA integrity_check;')" != "ok" ]; then
	fail "the snapshot failed its integrity check; not keeping it" \
		"the copy failed its integrity check, so it was not kept."
fi

gzip -9 "$TMP/$NAME"
age --encrypt --recipient "$AGE_RECIPIENT" \
	--output "$STORE/$NAME.gz.age" "$TMP/$NAME.gz"
log "wrote $STORE/$NAME.gz.age ($(du -h "$STORE/$NAME.gz.age" | cut -f1))"

# Keep a month locally.
find "$STORE" -name '*.age' -mtime +"$KEEP_DAYS" -delete

# And push to the private repository, which is the copy that survives the VM.
if [ -n "${BACKUP_REPO_DIR:-}" ] && [ -d "$BACKUP_REPO_DIR/.git" ]; then
	cd "$BACKUP_REPO_DIR"
	mkdir -p "$INSTANCE"
	cp "$STORE/$NAME.gz.age" "$INSTANCE/"
	# Thirty days there too, so the repository does not grow without end.
	find "$INSTANCE" -name '*.age' -mtime +"$KEEP_DAYS" -delete
	git add -A
	if git diff --cached --quiet; then
		log "nothing new to push"
		PUSHED=pushed
	else
		git -c user.name='frog-backup' -c user.email='backup@frog.invalid' \
			commit -q -m "backup: $INSTANCE $STAMP ($LABEL)"
		if git push -q origin HEAD 2>/dev/null; then
			log "pushed to the backup repository"
			PUSHED=pushed
		else
			log "WARNING: could not push the backup; the local copy is kept"
			PUSHED=failed
		fi
	fi
else
	log "BACKUP_REPO_DIR is not set up; keeping the local copy only"
fi

# One line beside the database, read by the admin page: when, why, and whether
# the copy reached the backup repository.
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $LABEL $([ "$PUSHED" = pushed ] && echo pushed || echo local)" \
	> "$VOLUME/last-backup"

case "$PUSHED" in
	pushed) ops_record "$VOLUME" backup true "$WHAT copied off the server." ;;
	failed) ops_record "$VOLUME" backup false \
		"$WHAT could not be copied off the server; it is kept on the server." ;;
	*) ops_record "$VOLUME" backup true "$WHAT kept on the server only." ;;
esac
