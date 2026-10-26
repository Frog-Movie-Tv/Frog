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

total_kb() { awk '/^MemTotal:/ {print $2}' /proc/meminfo; }

start() {
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

case "${1:-status}" in
	start) start ;;
	stop) stop ;;
	status) status ;;
	*) echo "usage: keepalive.sh start|stop|status"; exit 1 ;;
esac
