#!/usr/bin/env bash
# Sourced by update.sh and backup.sh: record how a job went where Frog can
# read it, so administrators see it on the admin page and get a notification
# when something fails, instead of it living only in a log on the server.
#
# One JSON object per line in ops.jsonl, beside the database in the data
# volume:
#
#   {"at":"2026-10-29T18:00:00.123Z","kind":"backup","ok":true,"reason":"..."}
#
# kind is backup, update, rollback or restore. The reason is one short line in
# plain words; it is what the notification says.

# ops_record <volume dir> <kind> <true|false> <reason>
ops_record() {
	local volume="$1" kind="$2" ok="$3" reason="$4"
	# Without the volume there is nowhere Frog could read it from.
	[ -n "$volume" ] && [ -d "$volume" ] || return 0
	local file="$volume/ops.jsonl"
	# JSON string escaping: backslash and quote, and no control characters.
	reason="$(printf '%s' "$reason" | tr -d '\000-\037' | sed 's/\\/\\\\/g; s/"/\\"/g')"
	printf '{"at":"%s","kind":"%s","ok":%s,"reason":"%s"}\n' \
		"$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)" "$kind" "$ok" "$reason" >> "$file" || return 0
	# A couple of hundred lines is months of history and stays tiny.
	if [ "$(wc -l < "$file")" -gt 200 ]; then
		tail -n 200 "$file" > "$file.tmp" && mv "$file.tmp" "$file"
	fi
	# Written as root on the host, read by the unprivileged user in the image.
	chmod 644 "$file" 2> /dev/null || true
}
