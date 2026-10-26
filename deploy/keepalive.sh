#!/usr/bin/env bash
# Keeps the instance from being reclaimed as idle.
#
# Oracle deems an Always Free instance idle only when, over seven days, 95th
# percentile CPU is under 20% AND network is under 20% AND memory is under 20%
# (memory applies to A1 shapes, which is what this is). All three have to be
# true at once, so holding one above the line is enough.
#
# Memory is the cheap one: a tmpfs file of about a quarter of the machine's RAM
# costs no CPU, no bandwidth and no money. Chasing the CPU threshold instead
# would mean keeping a core busy for eight hours a week to no purpose.
#
#   keepalive.sh start
#   keepalive.sh stop
#   keepalive.sh status
set -euo pipefail

MOUNT=/run/frog-keepalive
# A comfortable margin over the 20% line without crowding the two instances.
SHARE=28

# The memory test applies to A1 shapes only. On anything else — the E2 micro,
# say — holding memory buys nothing and simply takes RAM the machine needs, so
# there the keepalive is a light touch of CPU and network instead.
shape() {
	curl -s --max-time 2 http://169.254.169.254/opc/v1/instance/shape 2>/dev/null || echo unknown
}

is_a1() {
	case "$(shape)" in
		VM.Standard.A1*) return 0 ;;
		*) return 1 ;;
	esac
}

total_kb() { awk '/^MemTotal:/ {print $2}' /proc/meminfo; }

start() {
	if ! is_a1; then
		echo "$(shape): memory is not part of the idle test here; nothing held."
		echo "The timer does a light CPU and network touch instead."
		return 0
	fi

	local total_mb hold_mb
	total_mb=$(( $(total_kb) / 1024 ))
	hold_mb=$(( total_mb * SHARE / 100 ))

	mkdir -p "$MOUNT"
	mountpoint -q "$MOUNT" || mount -t tmpfs -o size=$(( hold_mb + 64 ))m tmpfs "$MOUNT"

	# tmpfs pages count as used memory and are not reclaimable while the file
	# exists, which is exactly the property wanted here.
	if [ ! -f "$MOUNT/ballast" ]; then
		dd if=/dev/zero of="$MOUNT/ballast" bs=1M count="$hold_mb" status=none
	fi
	echo "holding ${hold_mb}MB of ${total_mb}MB (${SHARE}%)"
}

stop() {
	rm -f "$MOUNT/ballast"
	mountpoint -q "$MOUNT" && umount "$MOUNT"
	echo "released"
}

status() {
	if [ -f "$MOUNT/ballast" ]; then
		echo "holding $(du -h "$MOUNT/ballast" | cut -f1)"
	else
		echo "not holding anything"
	fi
	free -h
}

# A short, cheap sign of life for shapes where memory does not count. Kept
# deliberately small: this is to look alive, not to burn a core for nothing.
tick() {
	if is_a1; then
		echo "A1: the memory hold covers this; nothing to do."
		return 0
	fi

	# Its own health endpoint, then one request off the machine, so the
	# network metric sees something in both directions.
	curl -s --max-time 5 http://127.0.0.1:8080/healthz > /dev/null 2>&1 || true
	curl -s --max-time 5 -o /dev/null https://api.github.com/zen 2>/dev/null || true

	local until
	until=$(( $(date +%s) + 45 ))
	while [ "$(date +%s)" -lt "$until" ]; do
		# One core, briefly. The other stays free for Frog.
		: $((RANDOM * RANDOM))
	done
	echo "ticked"
}

case "${1:-status}" in
	start) start ;;
	stop) stop ;;
	tick) tick ;;
	status) status ;;
	*) echo "usage: keepalive.sh start|stop|tick|status"; exit 1 ;;
esac
