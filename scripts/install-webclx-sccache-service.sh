#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd -P -- "$SCRIPT_DIR/.." && pwd -P)"
SERVICE_TEMPLATE="$PROJECT_DIR/config/systemd/webclx-sccache.service.in"
DROP_IN_SOURCE="$PROJECT_DIR/config/systemd/webclx.service.d/sccache.conf"
SERVICE_TARGET="/etc/systemd/system/webclx-sccache.service"
DROP_IN_TARGET="/etc/systemd/system/webclx.service.d/sccache.conf"
APPLY=false

usage() {
  printf '%s\n' \
    "Usage: bash scripts/install-webclx-sccache-service.sh [--apply] [--dry-run]" \
    "" \
    "Without --apply, print the resolved installation paths and service file."
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --apply) APPLY=true; shift ;;
    --dry-run) APPLY=false; shift ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

SCCACHE_BIN="$(command -v sccache || true)"
if [ -z "$SCCACHE_BIN" ] || [ ! -x "$SCCACHE_BIN" ]; then
  printf 'sccache executable not found on PATH\n' >&2
  exit 1
fi
SCCACHE_BIN="$(realpath "$SCCACHE_BIN")"

rendered_service="$(sed "s|@SCCACHE_BIN@|$SCCACHE_BIN|g" "$SERVICE_TEMPLATE")"

if [ "$APPLY" = false ]; then
  printf 'service_target=%s\n' "$SERVICE_TARGET"
  printf 'drop_in_target=%s\n' "$DROP_IN_TARGET"
  printf 'sccache_bin=%s\n' "$SCCACHE_BIN"
  printf '%s\n' "$rendered_service"
  exit 0
fi

install -d -m 0755 "$(dirname "$DROP_IN_TARGET")"
printf '%s\n' "$rendered_service" >"$SERVICE_TARGET"
chmod 0644 "$SERVICE_TARGET"
install -m 0644 "$DROP_IN_SOURCE" "$DROP_IN_TARGET"
systemctl daemon-reload
systemctl enable --now webclx-sccache.service
systemctl is-active --quiet webclx-sccache.service
