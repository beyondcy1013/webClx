#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/hosted-trial-maintenance.sh ACTION [options]

Actions:
  capacity   Enforce configured capacity limits for all QA instances
  backup     Create encrypted backups and prune complete old backup pairs
  run        Enforce capacity, then perform the once-daily backup window

Options:
  --config-dir PATH    Private per-customer config directory
  --root-dir PATH      Instance root (default: /srv/webclx-trials)
  --backup-root PATH   Backup root (default: /srv/webclx-trial-backups)
  --state-dir PATH     Daily-run state (default: /var/lib/webclx-trial-maintenance)
  --guard-script PATH  Data guard executable
  --backup-hour HOUR   Earliest UTC hour for daily backup (default: 3)
  --apply              Perform operations
  --dry-run            Run capacity checks without changing instances
USAGE
}

die_usage() { echo "$1" >&2; exit 2; }
die() { echo "$1" >&2; exit 1; }

validate_absolute_path() {
  local path="$1" label="$2"
  [ -n "$path" ] && [[ "$path" = /* ]] && [ "$path" != / ] || die_usage "$label must be a non-root absolute path"
  case "/$path/" in */../*|*/./*) die_usage "$label must not contain dot path components" ;; esac
  case "$path" in *$'\n'*|*$'\r'*) die_usage "$label contains an invalid character" ;; esac
}

load_config() {
  local config="$1" key value mode
  [ -f "$config" ] && [ ! -L "$config" ] || die "customer config must not be a symbolic link: $config"
  mode="$(stat -c %a -- "$config")"
  case "$mode" in 400|600) ;; *) die "customer config must have private 0600 or 0400 permissions: $config" ;; esac

  CUSTOMER_ID=""
  MAX_MIB=""
  BACKUP_RECIPIENT=""
  GPG_HOME=""
  RETENTION_COUNT=""
  local seen="|"
  while IFS='=' read -r key value || [ -n "$key" ]; do
    [ -n "$key" ] || continue
    case "$seen" in *"|$key|"*) die "duplicate customer config field: $key" ;; esac
    seen="$seen$key|"
    case "$key" in
      customer_id) CUSTOMER_ID="$value" ;;
      max_mib) MAX_MIB="$value" ;;
      backup_recipient) BACKUP_RECIPIENT="$value" ;;
      gpg_home) GPG_HOME="$value" ;;
      retention_count) RETENTION_COUNT="$value" ;;
      *) die "unknown customer config field: $key" ;;
    esac
  done < "$config"

  [[ "$CUSTOMER_ID" =~ ^qa-[a-z0-9][a-z0-9-]{0,25}[a-z0-9]$ ]] || die "invalid QA customer id in config"
  [ "$(basename "$config")" = "$CUSTOMER_ID.conf" ] || die "customer config filename mismatch"
  [[ "$MAX_MIB" =~ ^[0-9]+$ ]] || die "invalid max_mib in customer config"
  [[ "$BACKUP_RECIPIENT" =~ ^[0-9A-Fa-f]{40}$|^[0-9A-Fa-f]{64}$ ]] || die "invalid backup recipient fingerprint"
  [[ "$RETENTION_COUNT" =~ ^[1-9][0-9]*$ ]] || die "retention_count must be positive"
  validate_absolute_path "$GPG_HOME" "GPG home"
}

invoke_capacity() {
  local mode=--dry-run status
  [ "$APPLY" = true ] && mode=--apply
  set +e
  "$GUARD_SCRIPT" enforce --customer-id "$CUSTOMER_ID" --root-dir "$ROOT_DIR" \
    --max-mib "$MAX_MIB" "$mode"
  status=$?
  set -e
  [ "$status" -eq 0 ] || [ "$status" -eq 3 ] || return "$status"
}

prune_backups() {
  local customer_dir="$BACKUP_ROOT/$CUSTOMER_ID" remove_count index file
  [ -d "$customer_dir" ] && [ ! -L "$customer_dir" ] || die "customer backup directory is missing or unsafe"
  mapfile -t COMPLETE_BACKUPS < <(
    find "$customer_dir" -maxdepth 1 -type f -name "$CUSTOMER_ID-workspace-????????T??????Z.tar.gz.gpg" -printf '%f\n' | sort
  )
  remove_count=$((${#COMPLETE_BACKUPS[@]} - RETENTION_COUNT))
  [ "$remove_count" -gt 0 ] || return 0
  for ((index = 0; index < remove_count; index++)); do
    file="${COMPLETE_BACKUPS[$index]}"
    [ -f "$customer_dir/$file.sha256" ] && [ ! -L "$customer_dir/$file.sha256" ] || continue
    rm -f -- "$customer_dir/$file" "$customer_dir/$file.sha256"
  done
}

run_capacity_all() {
  local config
  for config in "$CONFIG_DIR"/*.conf; do
    [ -e "$config" ] || continue
    load_config "$config"
    invoke_capacity
  done
}

run_backup_all() {
  [ "$APPLY" = true ] || die_usage "backup requires --apply"
  local config customer_dir
  for config in "$CONFIG_DIR"/*.conf; do
    [ -e "$config" ] || continue
    load_config "$config"
    customer_dir="$BACKUP_ROOT/$CUSTOMER_ID"
    "$GUARD_SCRIPT" backup --customer-id "$CUSTOMER_ID" --root-dir "$ROOT_DIR" \
      --backup-dir "$customer_dir" --recipient "$BACKUP_RECIPIENT" --gpg-home "$GPG_HOME" --apply
    prune_backups
  done
}

ACTION="${1:-}"
case "$ACTION" in capacity|backup|run) shift ;; -h|--help) usage; exit 0 ;; *) usage; exit 2 ;; esac

CONFIG_DIR="/etc/webclx/trials"
ROOT_DIR="/srv/webclx-trials"
BACKUP_ROOT="/srv/webclx-trial-backups"
STATE_DIR="/var/lib/webclx-trial-maintenance"
GUARD_SCRIPT="/usr/local/libexec/webclx/hosted-trial-data-guard.sh"
BACKUP_HOUR=3
APPLY=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --config-dir) CONFIG_DIR="${2:-}"; shift 2 ;;
    --root-dir) ROOT_DIR="${2:-}"; shift 2 ;;
    --backup-root) BACKUP_ROOT="${2:-}"; shift 2 ;;
    --state-dir) STATE_DIR="${2:-}"; shift 2 ;;
    --guard-script) GUARD_SCRIPT="${2:-}"; shift 2 ;;
    --backup-hour) BACKUP_HOUR="${2:-}"; shift 2 ;;
    --apply) APPLY=true; shift ;;
    --dry-run) APPLY=false; shift ;;
    *) die_usage "unexpected argument: $1" ;;
  esac
done

validate_absolute_path "$CONFIG_DIR" "config directory"
validate_absolute_path "$ROOT_DIR" "root directory"
validate_absolute_path "$BACKUP_ROOT" "backup root"
validate_absolute_path "$STATE_DIR" "state directory"
validate_absolute_path "$GUARD_SCRIPT" "guard script"
[[ "$BACKUP_HOUR" =~ ^([0-9]|1[0-9]|2[0-3])$ ]] || die_usage "backup hour must be from 0 to 23"
[ -d "$CONFIG_DIR" ] && [ ! -L "$CONFIG_DIR" ] || die "config directory is missing or unsafe"
[ -x "$GUARD_SCRIPT" ] && [ ! -L "$GUARD_SCRIPT" ] || die "data guard executable is missing or unsafe"
if [ -e "$BACKUP_ROOT" ] || [ -L "$BACKUP_ROOT" ]; then
  [ -d "$BACKUP_ROOT" ] && [ ! -L "$BACKUP_ROOT" ] || die "backup root is unsafe"
fi
if [ -e "$STATE_DIR" ] || [ -L "$STATE_DIR" ]; then
  [ -d "$STATE_DIR" ] && [ ! -L "$STATE_DIR" ] || die "state directory is unsafe"
fi

case "$ACTION" in
  capacity) run_capacity_all ;;
  backup) run_backup_all ;;
  run)
    [ "$APPLY" = true ] || die_usage "run requires --apply"
    run_capacity_all
    CURRENT_DATE="$(date -u +%F)"
    CURRENT_HOUR="$(date -u +%H)"
    MARKER="$STATE_DIR/last-backup-date"
    if [ "$((10#$CURRENT_HOUR))" -ge "$BACKUP_HOUR" ] && { [ ! -f "$MARKER" ] || [ "$(<"$MARKER")" != "$CURRENT_DATE" ]; }; then
      run_backup_all
      install -d -m 0700 -- "$STATE_DIR"
      printf '%s\n' "$CURRENT_DATE" > "$MARKER.tmp.$$"
      chmod 0600 "$MARKER.tmp.$$"
      mv -- "$MARKER.tmp.$$" "$MARKER"
    fi
    ;;
esac
