#!/usr/bin/env bash
set -euo pipefail

SOURCE_ROOT="/home/codes"
QUEUE_DIR="/home/codes/webClx/.webclx-compile-queue"
APPLY=false

usage() {
  cat <<'EOF'
Usage: migrate-compile-api-logs.sh [options]

Move historical webClx compile API logs out of client source trees.
The default mode is a read-only dry run.

Options:
  --source-root DIR  Directory tree to scan (default: /home/codes)
  --queue-dir DIR    webClx compile queue directory
  --apply            Perform the migration
  -h, --help         Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --source-root)
      SOURCE_ROOT="${2:-}"
      shift 2
      ;;
    --queue-dir)
      QUEUE_DIR="${2:-}"
      shift 2
      ;;
    --apply)
      APPLY=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[ -n "$SOURCE_ROOT" ] || { echo "--source-root must not be empty" >&2; exit 2; }
[ -n "$QUEUE_DIR" ] || { echo "--queue-dir must not be empty" >&2; exit 2; }
SOURCE_ROOT="$(realpath -m -- "$SOURCE_ROOT")"
QUEUE_DIR="$(realpath -m -- "$QUEUE_DIR")"
[ -d "$SOURCE_ROOT" ] || { echo "source root not found: $SOURCE_ROOT" >&2; exit 1; }
[ "$SOURCE_ROOT" != "/" ] || { echo "refusing to scan filesystem root" >&2; exit 2; }
[ "$QUEUE_DIR" != "$SOURCE_ROOT" ] || { echo "queue directory must differ from source root" >&2; exit 2; }

LEGACY_ROOT="$QUEUE_DIR/legacy"
RUNS_DIR="$QUEUE_DIR/runs"
MANIFEST_DIR="$QUEUE_DIR/migration-manifests"
temp_dir="$(mktemp -d)"
trap 'rm -rf -- "$temp_dir"' EXIT

is_managed_source_path() {
  local path="$1"
  case "$path" in
    "$QUEUE_DIR"|"$QUEUE_DIR"/*) return 1 ;;
    "$SOURCE_ROOT"/docs/logs/webclx-build-*.log) return 0 ;;
    "$SOURCE_ROOT"/docs/logs/webclx-install-report-*.json) return 0 ;;
    "$SOURCE_ROOT"/*/docs/logs/webclx-build-*.log) return 0 ;;
    "$SOURCE_ROOT"/*/docs/logs/webclx-install-report-*.json) return 0 ;;
    *) return 1 ;;
  esac
}

destination_for_source() {
  local source="$1"
  printf '%s/%s\n' "$LEGACY_ROOT" "${source#"$SOURCE_ROOT"/}"
}

declare -a sources=()
declare -a destinations=()
declare -a actions=()
declare -a sizes=()
declare -a hashes=()
declare -A planned_destinations=()
declare -A source_log_dirs=()

files=0
moved=0
deduplicated=0
bytes=0
conflicts=0

while IFS= read -r -d '' source; do
  is_managed_source_path "$source" || continue
  destination="$(destination_for_source "$source")"
  size="$(stat -c '%s' -- "$source")"
  action="moved"
  if [ -e "$destination" ]; then
    if cmp -s -- "$source" "$destination"; then
      action="deduplicated"
      deduplicated=$((deduplicated + 1))
    else
      echo "destination conflict: $source -> $destination" >&2
      conflicts=$((conflicts + 1))
      continue
    fi
  else
    moved=$((moved + 1))
  fi
  hash=""
  if [ "$APPLY" = true ]; then
    hash="$(sha256sum -- "$source" | awk '{print $1}')"
  fi
  sources+=("$source")
  destinations+=("$destination")
  actions+=("$action")
  sizes+=("$size")
  hashes+=("$hash")
  planned_destinations["$source"]="$destination"
  source_log_dirs["$(dirname -- "$source")"]=1
  files=$((files + 1))
  bytes=$((bytes + size))
done < <(
  find "$SOURCE_ROOT" \
    -path "$QUEUE_DIR" -prune -o \
    -type f \( \
      -path '*/docs/logs/webclx-build-*.log' -o \
      -path '*/docs/logs/webclx-install-report-*.json' \
    \) -print0 | sort -z
)

if [ "$conflicts" -ne 0 ]; then
  echo "migration aborted before changes: conflicts=$conflicts" >&2
  exit 1
fi

references_updated=0
declare -a reference_files=()
declare -a reference_destinations=()
if [ -d "$RUNS_DIR" ]; then
  while IFS= read -r -d '' reference_file; do
    old_path="$(<"$reference_file")"
    is_managed_source_path "$old_path" || continue
    destination="$(destination_for_source "$old_path")"
    if [ -n "${planned_destinations[$old_path]:-}" ] || [ -f "$destination" ]; then
      reference_files+=("$reference_file")
      reference_destinations+=("$destination")
      references_updated=$((references_updated + 1))
    fi
  done < <(
    find "$RUNS_DIR" -type f \( \
      -name 'log-*.path' -o \
      -name 'install-report-*.path' \
    \) -print0 | sort -z
  )
fi

if [ "$APPLY" = false ]; then
  printf 'mode=dry-run files=%d moved=%d deduplicated=%d references_updated=%d bytes=%d\n' \
    "$files" "$moved" "$deduplicated" "$references_updated" "$bytes"
  exit 0
fi

dirs_removed=0
manifest_path=""
if [ "$files" -gt 0 ] || [ "$references_updated" -gt 0 ]; then
  mkdir -p -- "$LEGACY_ROOT" "$MANIFEST_DIR"
  chmod 0700 -- "$LEGACY_ROOT" "$MANIFEST_DIR"
  manifest_name="compile-api-logs-$(date -u +%Y%m%dT%H%M%SZ)-$$.tsv"
  manifest_path="$MANIFEST_DIR/$manifest_name"
  manifest_tmp="$temp_dir/$manifest_name"
  printf 'source\tdestination\taction\tsize\tsha256\n' >"$manifest_tmp"

  for index in "${!sources[@]}"; do
    source="${sources[$index]}"
    destination="${destinations[$index]}"
    action="${actions[$index]}"
    mkdir -p -- "$(dirname -- "$destination")"
    if [ "$action" = "deduplicated" ]; then
      rm -- "$source"
    else
      mv -- "$source" "$destination"
    fi
    printf '%s\t%s\t%s\t%s\t%s\n' \
      "$source" "$destination" "$action" "${sizes[$index]}" "${hashes[$index]}" \
      >>"$manifest_tmp"
  done

  for index in "${!reference_files[@]}"; do
    reference_file="${reference_files[$index]}"
    destination="${reference_destinations[$index]}"
    reference_tmp="$(mktemp "${reference_file}.tmp.XXXXXX")"
    printf '%s\n' "$destination" >"$reference_tmp"
    chmod --reference="$reference_file" "$reference_tmp"
    mv -f -- "$reference_tmp" "$reference_file"
  done

  for log_dir in "${!source_log_dirs[@]}"; do
    if rmdir -- "$log_dir" 2>/dev/null; then
      dirs_removed=$((dirs_removed + 1))
    fi
  done

  chmod 0600 -- "$manifest_tmp"
  mv -- "$manifest_tmp" "$manifest_path"
fi

printf 'mode=apply files=%d moved=%d deduplicated=%d references_updated=%d dirs_removed=%d bytes=%d manifest=%s\n' \
  "$files" "$moved" "$deduplicated" "$references_updated" "$dirs_removed" "$bytes" "${manifest_path:--}"
