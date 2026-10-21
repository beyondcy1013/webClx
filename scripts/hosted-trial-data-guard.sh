#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/hosted-trial-data-guard.sh ACTION --customer-id qa-ID [options]

Actions:
  measure   Report workspace and artifact usage in bytes
  enforce   Freeze an instance when its configured capacity is exceeded
  backup    Encrypt a workspace-only backup to a GPG recipient
  restore   Verify and restore an encrypted backup into an empty directory

Options:
  --root-dir PATH       Instance root (default: /srv/webclx-trials)
  --backup-dir PATH     Encrypted backup root
  --recipient FPR       Exact GPG recipient fingerprint
  --gpg-home PATH       Optional isolated GnuPG home
  --max-mib N           Capacity limit for enforce
  --backup-file PATH    Encrypted backup to restore
  --restore-dir PATH    Explicit empty restore destination
  --apply               Perform operations
  --dry-run             Print an enforcement plan without changing state
USAGE
}

die_usage() {
  echo "$1" >&2
  exit 2
}

die() {
  echo "$1" >&2
  exit 1
}

json_string() {
  local value="$1"
  value=${value//\\/\\\\}
  value=${value//\"/\\\"}
  value=${value//$'\n'/\\n}
  value=${value//$'\r'/\\r}
  value=${value//$'\t'/\\t}
  printf '"%s"' "$value"
}

print_command() {
  local separator="" argument
  for argument in "$@"; do
    printf '%s%q' "$separator" "$argument"
    separator=" "
  done
  printf '\n'
}

validate_absolute_path() {
  local path="$1" label="$2"
  [ -n "$path" ] || die_usage "$label must not be empty"
  [[ "$path" = /* ]] || die_usage "$label must be an absolute path"
  [ "$path" != "/" ] || die_usage "$label must not be the filesystem root"
  case "/$path/" in
    */../*|*/./*) die_usage "$label must not contain dot path components" ;;
  esac
  case "$path" in
    *$'\n'*|*$'\r'*) die_usage "$label contains an invalid character" ;;
  esac
}

validate_child_path() {
  local path="$1" parent="$2" label="$3"
  case "$path" in
    "$parent"/*) ;;
    *) die "$label escapes its configured root" ;;
  esac
}

load_manifest() {
  [ -f "$MANIFEST" ] && [ ! -L "$MANIFEST" ] || die "instance manifest not found or unsafe: $MANIFEST"
  local key value seen_customer=false seen_instance=false seen_workspace=false seen_artifact=false seen_service=false
  while IFS='=' read -r key value; do
    case "$key" in
      customer_id) [ "$value" = "$CUSTOMER_ID" ] || die "manifest customer mismatch"; seen_customer=true ;;
      instance_dir) [ "$value" = "$INSTANCE_DIR" ] || die "manifest path mismatch"; seen_instance=true ;;
      workspace_dir) WORKSPACE_DIR="$value"; seen_workspace=true ;;
      artifact_dir) ARTIFACT_DIR="$value"; seen_artifact=true ;;
      service_name) SERVICE_NAME="$value"; seen_service=true ;;
    esac
  done < "$MANIFEST"
  [ "$seen_customer" = true ] && [ "$seen_instance" = true ] && [ "$seen_workspace" = true ] && \
    [ "$seen_artifact" = true ] && [ "$seen_service" = true ] || die "instance manifest is incomplete"
  validate_absolute_path "$WORKSPACE_DIR" "workspace directory"
  validate_absolute_path "$ARTIFACT_DIR" "artifact directory"
  validate_child_path "$WORKSPACE_DIR" "$INSTANCE_DIR" "workspace directory"
  validate_child_path "$ARTIFACT_DIR" "$INSTANCE_DIR" "artifact directory"
  [ "$SERVICE_NAME" = "webclx-qa-${CUSTOMER_ID}.service" ] || die "manifest service mismatch"
}

directory_bytes() {
  local directory="$1"
  [ -d "$directory" ] && [ ! -L "$directory" ] || die "required directory is missing or unsafe: $directory"
  du -sb -- "$directory" | awk '{print $1}'
}

measure_usage() {
  WORKSPACE_BYTES="$(directory_bytes "$WORKSPACE_DIR")"
  ARTIFACT_BYTES="$(directory_bytes "$ARTIFACT_DIR")"
  TOTAL_BYTES=$((WORKSPACE_BYTES + ARTIFACT_BYTES))
}

write_frozen_manifest() {
  local temporary="$MANIFEST.tmp.$$" key value found_state=false
  umask 077
  : > "$temporary"
  while IFS='=' read -r key value; do
    if [ "$key" = state ]; then
      printf 'state=quota-frozen\n' >> "$temporary"
      found_state=true
    elif [ "$key" = updated_at ]; then
      continue
    else
      printf '%s=%s\n' "$key" "$value" >> "$temporary"
    fi
  done < "$MANIFEST"
  [ "$found_state" = true ] || printf 'state=quota-frozen\n' >> "$temporary"
  printf 'updated_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$temporary"
  mv -- "$temporary" "$MANIFEST"
}

ACTION="${1:-}"
case "$ACTION" in
  measure|enforce|backup|restore) shift ;;
  -h|--help) usage; exit 0 ;;
  *) usage; exit 2 ;;
esac

CUSTOMER_ID=""
ROOT_DIR="/srv/webclx-trials"
BACKUP_DIR=""
RECIPIENT=""
GPG_HOME=""
MAX_MIB=""
BACKUP_FILE=""
RESTORE_DIR=""
APPLY=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --customer-id) CUSTOMER_ID="${2:-}"; shift 2 ;;
    --root-dir) ROOT_DIR="${2:-}"; shift 2 ;;
    --backup-dir) BACKUP_DIR="${2:-}"; shift 2 ;;
    --recipient) RECIPIENT="${2:-}"; shift 2 ;;
    --gpg-home) GPG_HOME="${2:-}"; shift 2 ;;
    --max-mib) MAX_MIB="${2:-}"; shift 2 ;;
    --backup-file) BACKUP_FILE="${2:-}"; shift 2 ;;
    --restore-dir) RESTORE_DIR="${2:-}"; shift 2 ;;
    --apply) APPLY=true; shift ;;
    --dry-run) APPLY=false; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die_usage "unexpected argument: $1" ;;
  esac
done

[ -n "$CUSTOMER_ID" ] || die_usage "--customer-id is required"
[[ "$CUSTOMER_ID" =~ ^qa-[a-z0-9][a-z0-9-]{0,25}[a-z0-9]$ ]] || \
  die_usage "QA customer id must begin with qa- and contain 5-29 lowercase letters, digits, or hyphens"
validate_absolute_path "$ROOT_DIR" "root directory"

INSTANCE_DIR="$ROOT_DIR/$CUSTOMER_ID"
MANIFEST="$INSTANCE_DIR/manifest.env"
WORKSPACE_DIR="$INSTANCE_DIR/workspace"
ARTIFACT_DIR="$INSTANCE_DIR/artifacts"
SERVICE_NAME="webclx-qa-${CUSTOMER_ID}.service"
validate_child_path "$INSTANCE_DIR" "$ROOT_DIR" "instance directory"
[ -d "$INSTANCE_DIR" ] && [ ! -L "$INSTANCE_DIR" ] || die "instance directory is missing or unsafe"
load_manifest

case "$ACTION" in
  measure)
    measure_usage
    printf '{"customer_id":%s,"workspace_bytes":%s,"artifact_bytes":%s,"total_bytes":%s}\n' \
      "$(json_string "$CUSTOMER_ID")" "$WORKSPACE_BYTES" "$ARTIFACT_BYTES" "$TOTAL_BYTES"
    ;;

  enforce)
    [[ "$MAX_MIB" =~ ^[0-9]+$ ]] || die_usage "--max-mib must be a non-negative integer"
    measure_usage
    MAX_BYTES=$((MAX_MIB * 1024 * 1024))
    if [ "$TOTAL_BYTES" -le "$MAX_BYTES" ]; then
      printf '{"customer_id":%s,"total_bytes":%s,"max_bytes":%s,"exceeded":false}\n' \
        "$(json_string "$CUSTOMER_ID")" "$TOTAL_BYTES" "$MAX_BYTES"
      exit 0
    fi
    if [ "$APPLY" = true ]; then
      systemctl stop "$SERVICE_NAME"
      chmod -R a-w -- "$WORKSPACE_DIR" "$ARTIFACT_DIR"
      write_frozen_manifest
      printf '{"customer_id":%s,"total_bytes":%s,"max_bytes":%s,"exceeded":true,"state":"quota-frozen"}\n' \
        "$(json_string "$CUSTOMER_ID")" "$TOTAL_BYTES" "$MAX_BYTES"
    else
      print_command systemctl stop "$SERVICE_NAME"
      print_command chmod -R a-w -- "$WORKSPACE_DIR" "$ARTIFACT_DIR"
    fi
    exit 3
    ;;

  backup)
    [ "$APPLY" = true ] || die_usage "backup requires --apply"
    [ -n "$BACKUP_DIR" ] || die_usage "--backup-dir is required"
    [ -n "$RECIPIENT" ] || die_usage "--recipient is required"
    validate_absolute_path "$BACKUP_DIR" "backup directory"
    case "$BACKUP_DIR" in
      "$INSTANCE_DIR"|"$INSTANCE_DIR"/*) die_usage "backup directory must be outside the live instance" ;;
    esac
    if [ -e "$BACKUP_DIR" ] || [ -L "$BACKUP_DIR" ]; then
      [ -d "$BACKUP_DIR" ] && [ ! -L "$BACKUP_DIR" ] || die "backup directory must be a real directory"
    fi
    [ -d "$WORKSPACE_DIR" ] && [ ! -L "$WORKSPACE_DIR" ] || die "workspace must be a real directory"
    if find "$WORKSPACE_DIR" -type l -print -quit | grep -q .; then
      die "workspace backup refuses symbolic links"
    fi
    [[ "$RECIPIENT" =~ ^[0-9A-Fa-f]{40}$|^[0-9A-Fa-f]{64}$ ]] || \
      die_usage "--recipient must be an exact GPG fingerprint"
    RECIPIENT="${RECIPIENT^^}"
    if [ -n "$GPG_HOME" ]; then validate_absolute_path "$GPG_HOME" "GPG home"; fi
    GPG_ARGS=(--batch --yes --no-tty)
    if [ -n "$GPG_HOME" ]; then GPG_ARGS+=(--homedir "$GPG_HOME"); fi
    RECIPIENT_RECORDS="$(gpg "${GPG_ARGS[@]}" --with-colons --fingerprint --list-keys -- "$RECIPIENT" 2>/dev/null)" || \
      die "GPG recipient is unavailable"
    printf '%s\n' "$RECIPIENT_RECORDS" | awk -F: -v wanted="$RECIPIENT" \
      '$1 == "fpr" && toupper($10) == wanted {found = 1} END {exit !found}' || \
      die "exact GPG recipient fingerprint is unavailable"
    umask 077
    install -d -m 0700 -- "$BACKUP_DIR"
    TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
    BACKUP_FILE="$BACKUP_DIR/${CUSTOMER_ID}-workspace-$TIMESTAMP.tar.gz.gpg"
    ARCHIVE_PART="$BACKUP_FILE.archive.part.$$"
    ENCRYPTED_PART="$BACKUP_FILE.part.$$"
    CHECKSUM_PART="$BACKUP_FILE.sha256.part.$$"
    cleanup_backup() { rm -f -- "$ARCHIVE_PART" "$ENCRYPTED_PART" "$CHECKSUM_PART"; }
    trap cleanup_backup EXIT HUP INT TERM
    tar --create --gzip --file "$ARCHIVE_PART" --directory "$INSTANCE_DIR" workspace
    chmod 0600 "$ARCHIVE_PART"
    gpg "${GPG_ARGS[@]}" --trust-model always --recipient "$RECIPIENT" --encrypt \
      --output "$ENCRYPTED_PART" -- "$ARCHIVE_PART"
    chmod 0600 "$ENCRYPTED_PART"
    mv -- "$ENCRYPTED_PART" "$BACKUP_FILE"
    CHECKSUM="$(sha256sum -- "$BACKUP_FILE" | awk '{print $1}')"
    printf '%s  %s\n' "$CHECKSUM" "$(basename "$BACKUP_FILE")" > "$CHECKSUM_PART"
    chmod 0600 "$CHECKSUM_PART"
    mv -- "$CHECKSUM_PART" "$BACKUP_FILE.sha256"
    BACKUP_BYTES="$(stat -c %s -- "$BACKUP_FILE")"
    rm -f -- "$ARCHIVE_PART"
    trap - EXIT HUP INT TERM
    printf '{"customer_id":%s,"backup_file":%s,"sha256":%s,"bytes":%s}\n' \
      "$(json_string "$CUSTOMER_ID")" "$(json_string "$BACKUP_FILE")" "$(json_string "$CHECKSUM")" "$BACKUP_BYTES"
    ;;

  restore)
    [ "$APPLY" = true ] || die_usage "restore requires --apply"
    [ -n "$BACKUP_FILE" ] || die_usage "--backup-file is required"
    [ -n "$RESTORE_DIR" ] || die_usage "--restore-dir is required"
    validate_absolute_path "$BACKUP_FILE" "backup file"
    validate_absolute_path "$RESTORE_DIR" "restore directory"
    case "$RESTORE_DIR" in
      "$INSTANCE_DIR"|"$INSTANCE_DIR"/*) die_usage "restore directory must be outside the live instance" ;;
    esac
    [ -f "$BACKUP_FILE" ] && [ ! -L "$BACKUP_FILE" ] || die "encrypted backup is missing or unsafe"
    [ -f "$BACKUP_FILE.sha256" ] && [ ! -L "$BACKUP_FILE.sha256" ] || die "backup checksum is missing or unsafe"
    if [ -e "$RESTORE_DIR" ]; then
      [ -d "$RESTORE_DIR" ] && [ ! -L "$RESTORE_DIR" ] || die "restore destination must be a real directory"
      [ -z "$(find "$RESTORE_DIR" -mindepth 1 -print -quit)" ] || die "restore destination must be empty"
    fi
    EXPECTED_HASH="$(awk 'NR == 1 {print $1}' "$BACKUP_FILE.sha256")"
    [[ "$EXPECTED_HASH" =~ ^[0-9a-fA-F]{64}$ ]] || die "backup checksum is malformed"
    ACTUAL_HASH="$(sha256sum -- "$BACKUP_FILE" | awk '{print $1}')"
    [ "${EXPECTED_HASH,,}" = "$ACTUAL_HASH" ] || die "backup checksum mismatch"
    if [ -n "$GPG_HOME" ]; then validate_absolute_path "$GPG_HOME" "GPG home"; fi
    TEMP_ROOT="$(mktemp -d)"
    chmod 0700 "$TEMP_ROOT"
    ARCHIVE="$TEMP_ROOT/workspace.tar.gz"
    cleanup_restore() { rm -rf --one-file-system -- "$TEMP_ROOT"; }
    trap cleanup_restore EXIT HUP INT TERM
    GPG_ARGS=(--batch --yes --no-tty)
    if [ -n "$GPG_HOME" ]; then GPG_ARGS+=(--homedir "$GPG_HOME"); fi
    gpg "${GPG_ARGS[@]}" --decrypt --output "$ARCHIVE" -- "$BACKUP_FILE"
    chmod 0600 "$ARCHIVE"
    tar --list --gzip --file "$ARCHIVE" > "$TEMP_ROOT/names"
    [ -s "$TEMP_ROOT/names" ] || die "backup archive is empty"
    while IFS= read -r entry; do
      case "$entry" in
        workspace|workspace/*) ;;
        *) die "backup archive contains an unsafe path" ;;
      esac
      case "/$entry/" in
        */../*|*/./*) die "backup archive contains dot path components" ;;
      esac
      [[ "$entry" != /* ]] || die "backup archive contains an absolute path"
    done < "$TEMP_ROOT/names"
    tar --list --verbose --gzip --file "$ARCHIVE" > "$TEMP_ROOT/verbose"
    while IFS= read -r listing; do
      case "${listing:0:1}" in
        -|d) ;;
        *) die "backup archive contains a link or special file" ;;
      esac
    done < "$TEMP_ROOT/verbose"
    install -d -m 0700 -- "$RESTORE_DIR"
    tar --extract --gzip --file "$ARCHIVE" --directory "$RESTORE_DIR" \
      --no-same-owner --no-same-permissions --delay-directory-restore
    trap - EXIT HUP INT TERM
    cleanup_restore
    printf '{"customer_id":%s,"restore_dir":%s,"sha256":%s}\n' \
      "$(json_string "$CUSTOMER_ID")" "$(json_string "$RESTORE_DIR")" "$(json_string "$ACTUAL_HASH")"
    ;;
esac
