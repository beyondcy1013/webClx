#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-}"
[[ "$TARGET" =~ ^[a-z_][a-z0-9_-]*@[A-Za-z0-9][A-Za-z0-9.-]*$ ]] || {
  echo "usage: $0 user@host" >&2
  exit 2
}

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
FILES=(
  scripts/hosted-trial-data-guard.sh
  scripts/hosted-trial-maintenance.sh
  ops/hosted-trial/webclx-trial-maintenance.service
  ops/hosted-trial/webclx-trial-maintenance.timer
)

for file in "${FILES[@]}"; do
  [ -f "$PROJECT_DIR/$file" ] && [ ! -L "$PROJECT_DIR/$file" ] || {
    echo "required deployment file is missing or unsafe: $file" >&2
    exit 1
  }
done

REMOTE_DIR="/tmp/webclx-trial-maintenance-deploy.$$"
cleanup() { ssh "$TARGET" "rm -rf -- '$REMOTE_DIR'" >/dev/null 2>&1 || true; }
trap cleanup EXIT HUP INT TERM

ssh "$TARGET" "install -d -m 0700 '$REMOTE_DIR'"
rsync -a --chmod=F600 "$PROJECT_DIR/scripts/hosted-trial-data-guard.sh" "$TARGET:$REMOTE_DIR/"
rsync -a --chmod=F600 "$PROJECT_DIR/scripts/hosted-trial-maintenance.sh" "$TARGET:$REMOTE_DIR/"
rsync -a --chmod=F600 "$PROJECT_DIR/ops/hosted-trial/webclx-trial-maintenance.service" "$TARGET:$REMOTE_DIR/"
rsync -a --chmod=F600 "$PROJECT_DIR/ops/hosted-trial/webclx-trial-maintenance.timer" "$TARGET:$REMOTE_DIR/"

ssh "$TARGET" "REMOTE_DIR='$REMOTE_DIR' bash -s" <<'REMOTE'
set -euo pipefail
install -d -m 0755 /usr/local/libexec/webclx
install -d -m 0700 /etc/webclx/trials /var/lib/webclx-trial-maintenance
install -m 0755 "$REMOTE_DIR/hosted-trial-data-guard.sh" /usr/local/libexec/webclx/hosted-trial-data-guard.sh
install -m 0755 "$REMOTE_DIR/hosted-trial-maintenance.sh" /usr/local/libexec/webclx/hosted-trial-maintenance.sh
install -m 0644 "$REMOTE_DIR/webclx-trial-maintenance.service" /etc/systemd/system/webclx-trial-maintenance.service
install -m 0644 "$REMOTE_DIR/webclx-trial-maintenance.timer" /etc/systemd/system/webclx-trial-maintenance.timer
systemd-analyze verify \
  /etc/systemd/system/webclx-trial-maintenance.service \
  /etc/systemd/system/webclx-trial-maintenance.timer
systemctl daemon-reload
systemctl disable --now webclx-trial-maintenance.timer >/dev/null 2>&1 || true
test -z "$(find /etc/webclx/trials -mindepth 1 -maxdepth 1 -print -quit)"
test "$(systemctl is-enabled webclx-trial-maintenance.timer 2>/dev/null || true)" != enabled
systemctl is-active webclx.service
sha256sum \
  /usr/local/libexec/webclx/hosted-trial-data-guard.sh \
  /usr/local/libexec/webclx/hosted-trial-maintenance.sh \
  /etc/systemd/system/webclx-trial-maintenance.service \
  /etc/systemd/system/webclx-trial-maintenance.timer
REMOTE

trap - EXIT HUP INT TERM
cleanup
