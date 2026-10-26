#!/usr/bin/env bash
# Set up a fresh Oracle Cloud Ubuntu machine to run Frog.
#
#   sudo ./install.sh
#
# Safe to run again: everything it does is idempotent. It does not start
# anything that needs secrets — fill in the .env files it creates, then
# follow the rest of docs/DEPLOY-ORACLE.md.
set -euo pipefail

[ "$(id -u)" = "0" ] || { echo "run this with sudo"; exit 1; }

ROOT=/opt/frog
HERE="$(cd "$(dirname "$0")" && pwd)"
step() { echo; echo "==> $*"; }

step "packages"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq \
	ca-certificates curl git gnupg sqlite3 age fail2ban \
	unattended-upgrades netfilter-persistent iptables-persistent

step "docker"
if ! command -v docker > /dev/null; then
	install -m 0755 -d /etc/apt/keyrings
	curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
		| gpg --dearmor -o /etc/apt/keyrings/docker.gpg
	chmod a+r /etc/apt/keyrings/docker.gpg
	echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
		> /etc/apt/sources.list.d/docker.list
	apt-get update -qq
	apt-get install -y -qq docker-ce docker-ce-cli containerd.io \
		docker-buildx-plugin docker-compose-plugin
fi
systemctl enable --now docker

# Oracle's Ubuntu images ship an iptables policy that rejects everything not
# explicitly allowed, and the rule sits above anything appended later. Opening
# 80 and 443 in the security list alone leaves the site unreachable, which is
# the single most common way this goes wrong.
step "host firewall"
for port in 80 443; do
	if iptables -C INPUT -p tcp --dport "$port" -j ACCEPT 2> /dev/null; then
		echo "    $port already allowed"
		continue
	fi
	# Insert above the catch-all REJECT; appending would never match.
	reject_line="$(iptables -L INPUT --line-numbers -n \
		| awk '/REJECT|DROP/ {print $1; exit}')"
	if [ -n "$reject_line" ]; then
		iptables -I INPUT "$reject_line" -p tcp --dport "$port" -j ACCEPT
	else
		iptables -A INPUT -p tcp --dport "$port" -j ACCEPT
	fi
	echo "    $port allowed"
done
netfilter-persistent save > /dev/null
echo "    saved; check /etc/iptables/rules.v4 for duplicates"

step "directories"
mkdir -p "$ROOT"/{deploy,production,staging,backups/production,backups/staging,caddy}
cp -r "$HERE"/*.sh "$ROOT/deploy/"
cp -r "$HERE"/systemd "$ROOT/deploy/"
cp "$HERE/compose.yml" "$ROOT/production/compose.yml"
cp "$HERE/compose.yml" "$ROOT/staging/compose.yml"
cp "$HERE/caddy/compose.yml" "$ROOT/caddy/compose.yml"
cp "$HERE/caddy/Caddyfile" "$ROOT/caddy/Caddyfile"
chmod +x "$ROOT"/deploy/*.sh

for instance in production staging; do
	env_file="$ROOT/$instance/.env"
	if [ ! -f "$env_file" ]; then
		cp "$HERE/instance.env.example" "$env_file"
		if [ "$instance" = "staging" ]; then
			sed -i 's/^IMAGE_TAG=.*/IMAGE_TAG=main/; s/^COMPOSE_PROJECT_NAME=.*/COMPOSE_PROJECT_NAME=frog-staging/' "$env_file"
		fi
		echo "    created $env_file — fill it in"
	fi
	chmod 600 "$env_file"
done

if [ ! -f "$ROOT/caddy/.env" ]; then
	cp "$HERE/caddy.env.example" "$ROOT/caddy/.env"
	echo "    created $ROOT/caddy/.env — fill it in"
fi
chmod 600 "$ROOT/caddy/.env"

if [ ! -f "$ROOT/backup.env" ]; then
	cp "$HERE/backup.env.example" "$ROOT/backup.env"
	echo "    created $ROOT/backup.env — fill it in"
fi
chmod 600 "$ROOT/backup.env"

step "network"
docker network inspect frog-edge > /dev/null 2>&1 || docker network create frog-edge

step "services"
cp "$ROOT"/deploy/systemd/*.service "$ROOT"/deploy/systemd/*.timer /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now frog-keepalive.service
for instance in production staging; do
	systemctl enable "frog-update@$instance.timer" "frog-backup@$instance.timer"
done
echo "    timers enabled but not started; start them once the .env files are filled in"

step "hardening"
# Keys only. The Oracle image already does this, but say so explicitly.
sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication no/' /etc/ssh/sshd_config
sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin prohibit-password/' /etc/ssh/sshd_config
systemctl reload ssh 2> /dev/null || systemctl reload sshd 2> /dev/null || true

cat > /etc/fail2ban/jail.d/frog.conf <<'EOF'
[sshd]
enabled = true
maxretry = 5
bantime = 1h
findtime = 10m
EOF
systemctl enable --now fail2ban
systemctl restart fail2ban

cat > /etc/apt/apt.conf.d/20auto-upgrades <<'EOF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
EOF

cat <<EOF

Done. Next, by hand:

  1. Fill in $ROOT/production/.env and $ROOT/staging/.env
     (TMDB_API_KEY, FROG_NOTIFY_HOUR, TZ)
  2. Fill in $ROOT/caddy/.env (the two DuckDNS names and an email)
  3. Fill in $ROOT/backup.env (AGE_RECIPIENT, BACKUP_REPO_DIR)
  4. docker login ghcr.io       (only if the repository is private)
  5. cd $ROOT/caddy && docker compose up -d
  6. $ROOT/deploy/update.sh production
     $ROOT/deploy/update.sh staging
  7. systemctl start frog-update@production.timer frog-update@staging.timer
     systemctl start frog-backup@production.timer frog-backup@staging.timer

docs/DEPLOY-ORACLE.md has the click-by-click for the parts only you can do.
EOF
