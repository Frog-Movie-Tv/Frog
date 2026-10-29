#!/usr/bin/env bash
# Send the encrypted backups to a private repository, so they survive losing
# this machine.
#
#   sudo /opt/frog/deploy/backup-repo.sh git@github.com:OWNER/REPOSITORY.git
#
# Before running it, add /opt/frog/backup-key.pub (install.sh makes it) to that
# repository as a deploy key with write access. This clones it with that key,
# proves the key can push, and records both in /opt/frog/backup.env. Safe to
# run again, for instance to point at another repository.
set -euo pipefail

URL="${1:?usage: backup-repo.sh git@github.com:OWNER/REPOSITORY.git}"
ROOT="/opt/frog"
KEY="$ROOT/backup-key"
DIR="$ROOT/backup-repo"
ENV_FILE="$ROOT/backup.env"

[ "$(id -u)" = "0" ] || { echo "run this with sudo"; exit 1; }
[ -f "$KEY" ] || { echo "no $KEY: run install.sh first"; exit 1; }
[ -f "$ENV_FILE" ] || { echo "no $ENV_FILE: run install.sh first"; exit 1; }

log() { echo "[$(date -Is)] $*"; }

# The deploy key and nothing else; GitHub's host key is trusted on first use.
export GIT_SSH_COMMAND="ssh -i $KEY -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=accept-new"

if [ -d "$DIR/.git" ]; then
	log "already cloned; pointing it at $URL"
	git -C "$DIR" remote set-url origin "$URL"
	git -C "$DIR" pull -q --rebase origin HEAD 2> /dev/null || true
else
	log "cloning $URL"
	if ! git clone -q "$URL" "$DIR" 2> /tmp/backup-repo.err; then
		cat /tmp/backup-repo.err
		log "could not clone: is $KEY.pub a deploy key on that repository?"
		exit 1
	fi
fi

# A brand-new repository has no commit, so there is nothing to push onto yet.
if ! git -C "$DIR" rev-parse -q --verify HEAD > /dev/null; then
	git -C "$DIR" symbolic-ref HEAD refs/heads/main
	cat > "$DIR/README.md" <<'EOF'
# Frog backups

Nightly, age-encrypted copies of Frog's database, pushed by the server. They
can only be read with the private age key, which is not on the server.
Restore one with `deploy/restore.sh` (see docs/DEPLOY-ORACLE.md in Frog).
EOF
	git -C "$DIR" add README.md
	git -C "$DIR" -c user.name='frog-backup' -c user.email='backup@frog.invalid' \
		commit -q -m "backups: start"
fi

log "checking the key can push"
if ! git -C "$DIR" push -q -u origin HEAD; then
	log "the push was refused: is $KEY.pub a deploy key on that repository, with write access?"
	exit 1
fi

# Record both in backup.env, replacing what was there.
set_var() {
	if grep -q "^$1=" "$ENV_FILE"; then
		sed -i "s|^$1=.*|$1=$2|" "$ENV_FILE"
	else
		echo "$1=$2" >> "$ENV_FILE"
	fi
}
set_var BACKUP_REPO_DIR "$DIR"
set_var BACKUP_SSH_KEY "$KEY"
chmod 600 "$ENV_FILE"

log "connected: every backup from now on is also pushed to $URL"
log "make one now to see it arrive:  sudo $ROOT/deploy/backup.sh production"
