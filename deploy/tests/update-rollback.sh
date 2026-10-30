#!/usr/bin/env bash
# Runs deploy/update.sh against stand-ins for docker, sqlite3 and chown, and
# checks what each way an update can go leaves in the database:
#
#   bash deploy/tests/update-rollback.sh
#
# Nothing here touches Docker or a real instance. The stand-in "new image"
# writes a row and sets the schema generation (SQLite's user_version) the way
# a starting Frog does; the stand-in health check answers as each scenario
# says.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="${1:-$HERE/../..}"
# The first Python that runs (Windows has a python3 that only opens the Store).
for candidate in python3 python; do
	if "$candidate" -c '' 2> /dev/null; then
		PY="$(command -v "$candidate") -I"
		break
	fi
done
: "${PY:?no Python found}"
# Git Bash on Windows: Python needs C:/… paths, PATH needs /c/….
native() { if command -v cygpath > /dev/null; then cygpath -m "$1"; else printf '%s' "$1"; fi; }
posix() { if command -v cygpath > /dev/null; then cygpath -u "$1"; else printf '%s' "$1"; fi; }
pass=0
fail=0
check() {
	if [ "$2" = "$3" ]; then
		echo "  [PASS] $1"
		pass=$((pass + 1))
	else
		echo "  [FAIL] $1 — got '$2', want '$3'"
		fail=$((fail + 1))
	fi
}

# scenario <name> <new image healthy> <generation it leaves> <previous image healthy>
scenario() {
	local name="$1" new_healthy="$2" new_gen="$3" prev_healthy="$4"
	echo "== $name"
	local root
	root="$(native "$(mktemp -d)")"
	mkdir -p "$root"/{deploy,production,backups/production,volume,bin,state}
	cp "$REPO/deploy/update.sh" "$REPO/deploy/ops-log.sh" "$root/deploy/"
	printf '#!/usr/bin/env bash\nexit 0\n' > "$root/deploy/backup.sh"
	$PY - "$root/volume/frog.db" << 'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("CREATE TABLE t (v TEXT)")
c.execute("INSERT INTO t VALUES ('before')")
c.commit()
EOF
	echo "sha256:old" > "$root/state/digest"
	cat > "$root/bin/sqlite3" << EOF
#!/usr/bin/env bash
exec $PY "$(native "$HERE")/sqlite_shell.py" "\$@"
EOF
	cat > "$root/bin/docker" << EOF
#!/usr/bin/env bash
S="$root/state"; DB="$root/volume/frog.db"
case "\$1 \$2" in
	"volume inspect") echo "$root/volume"; exit 0 ;;
	"inspect --format") echo "ghcr.io/frog@\$(cat \$S/digest)"; exit 0 ;;
	"image prune") exit 0 ;;
esac
shift
case "\$*" in
	"config --images") echo "ghcr.io/frog:latest" ;;
	"pull --quiet") echo "sha256:new" > \$S/digest ;;
	"up -d --remove-orphans")
		echo new > \$S/running
		$PY -c "import sqlite3; c = sqlite3.connect('\$DB'); c.execute(\"INSERT INTO t VALUES ('migrated')\"); c.execute('PRAGMA user_version = $new_gen'); c.commit()" ;;
	-f*) echo previous > \$S/running ;;
	exec*)
		case "\$(cat \$S/running 2> /dev/null)" in
			new) [ "$new_healthy" = yes ] && { echo '{"ok":true,"version":"9.9.9"}'; exit 0; } ;;
			previous) [ "$prev_healthy" = yes ] && { echo '{"ok":true,"version":"9.9.8"}'; exit 0; } ;;
		esac
		exit 1 ;;
	stop*|down*) echo stopped > \$S/running ;;
esac
exit 0
EOF
	for quiet in chown sleep; do printf '#!/usr/bin/env bash\nexit 0\n' > "$root/bin/$quiet"; done
	chmod +x "$root"/bin/* "$root"/deploy/*.sh
	PATH="$(posix "$root")/bin:$PATH" FROG_ROOT="$root" \
		bash "$root/deploy/update.sh" production > "$root/log" 2>&1
	local code=$?
	local rows running snapshot failed
	rows="$($PY -c "import sqlite3; print(','.join(r[0] for r in sqlite3.connect('$root/volume/frog.db').execute('SELECT v FROM t')))")"
	running="$(cat "$root/state/running" 2> /dev/null)"
	snapshot=$([ -f "$root/backups/production/pre-update.db" ] && echo kept || echo removed)
	failed=$(find "$root/volume" -name 'frog.db.failed-update-*' | wc -l | tr -d ' ')
	RESULT="$code|$rows|$running|$snapshot|$failed"
	LOG="$(cat "$root/log")"
	rm -rf "$root"
}

# Fields: exit code | rows in the live database | image running | snapshot | failed copies kept
scenario "a healthy update" yes 1 yes
check "keeps the migrated database and drops the snapshot" "$RESULT" "0|before,migrated|new|removed|0"

scenario "an additive update that fails" no 0 yes
check "goes back to the previous image and keeps the data" "$RESULT" "1|before,migrated|previous|removed|0"

scenario "an update that fails after raising the schema generation" no 1 yes
check "puts the snapshot back under the previous image" "$RESULT" "1|before|previous|removed|1"
case "$LOG" in
*"past what the previous one reads"*) check "and says why" yes yes ;;
*) check "and says why" no yes ;;
esac

scenario "an additive update where the previous image fails too" no 0 no
check "restores the snapshot and keeps it while still down" "$RESULT" "1|before|previous|kept|1"

echo "passed $pass, failed $fail"
[ "$fail" -eq 0 ]
