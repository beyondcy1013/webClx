#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/prepare-source-release-dir.sh <source.tar.gz> [destination]

Verify a webClx source release and extract it into a clean, non-worktree
directory suitable for webClx compile/deploy API project_dir.
USAGE
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  usage >&2
  exit 2
fi

ARCHIVE="$(realpath -e "$1")"
CHECKSUM_FILE="$ARCHIVE.sha256"
DESTINATION="${2:-}"
RELEASE_CACHE_DIR="${WEBCLX_RELEASE_CACHE_DIR:-/home/cache/webclx-release}"

if [ ! -f "$CHECKSUM_FILE" ]; then
  echo "source release checksum is missing: $CHECKSUM_FILE" >&2
  exit 1
fi

EXPECTED_SHA="$(awk 'NF >= 1 { print $1; exit }' "$CHECKSUM_FILE")"
if ! [[ "$EXPECTED_SHA" =~ ^[0-9A-Fa-f]{64}$ ]]; then
  echo "source release checksum is malformed" >&2
  exit 1
fi
ACTUAL_SHA="$(sha256sum "$ARCHIVE" | awk '{ print $1 }')"
if [ "${ACTUAL_SHA,,}" != "${EXPECTED_SHA,,}" ]; then
  echo "source release checksum mismatch" >&2
  exit 1
fi

ENTRY_LIST="$(mktemp)"
TYPE_LIST="$(mktemp)"
STATIC_MANIFEST_ACTUAL="$(mktemp)"
CREATED_CONTAINER=""
cleanup() {
  rm -f "$ENTRY_LIST" "$TYPE_LIST" "$STATIC_MANIFEST_ACTUAL"
  if [ -n "$CREATED_CONTAINER" ] && [ -d "$CREATED_CONTAINER" ]; then
    rm -rf -- "$CREATED_CONTAINER"
  fi
}
trap cleanup EXIT

tar -tzf "$ARCHIVE" > "$ENTRY_LIST"
if [ ! -s "$ENTRY_LIST" ]; then
  echo "source release archive is empty" >&2
  exit 1
fi

TOP_LEVEL="$(awk -F/ 'NF { print $1; exit }' "$ENTRY_LIST")"
if [ -z "$TOP_LEVEL" ] || ! [[ "$TOP_LEVEL" =~ ^webClx-[0-9A-Za-z._+-]+$ ]]; then
  echo "source release must use one versioned webClx top-level directory" >&2
  exit 1
fi

if ! awk -v root="$TOP_LEVEL" '
  function invalid_component(path, count, parts, part_index) {
    count = split(path, parts, "/")
    for (part_index = 1; part_index <= count; part_index++) {
      if (parts[part_index] == "..") return 1
    }
    return 0
  }
  /^\// { exit 1 }
  invalid_component($0) { exit 1 }
  $0 != root && index($0, root "/") != 1 { exit 1 }
  END { if (NR == 0) exit 1 }
' "$ENTRY_LIST"; then
  echo "source release contains an unsafe or foreign archive path" >&2
  exit 1
fi

tar -tvzf "$ARCHIVE" | awk '{ print substr($0, 1, 1) }' > "$TYPE_LIST"
if awk '$0 != "-" && $0 != "d" { found = 1 } END { exit found ? 0 : 1 }' "$TYPE_LIST"; then
  echo "source release contains links or unsupported archive entry types" >&2
  exit 1
fi

if [ -n "$DESTINATION" ]; then
  if [ -e "$DESTINATION" ] || [ -L "$DESTINATION" ]; then
    echo "destination already exists: $DESTINATION" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$DESTINATION")"
  CREATED_CONTAINER="$(realpath -m "$DESTINATION")"
  mkdir "$CREATED_CONTAINER"
else
  mkdir -p "$RELEASE_CACHE_DIR"
  CREATED_CONTAINER="$(mktemp -d "$RELEASE_CACHE_DIR/source-release.XXXXXX")"
fi

tar -xzf "$ARCHIVE" --no-same-owner --no-same-permissions -C "$CREATED_CONTAINER"
PROJECT_DIR="$CREATED_CONTAINER/$TOP_LEVEL"

for required in Cargo.toml Cargo.lock SOURCE_RELEASE STATIC_ASSETS_MANIFEST.sha256 \
  static/index.html static/i18n.js scripts/rebuild-and-deploy.sh; do
  if [ ! -f "$PROJECT_DIR/$required" ]; then
    echo "source release is missing required file: $required" >&2
    exit 1
  fi
done

mapfile -t SOURCE_RELEASE_LINES < "$PROJECT_DIR/SOURCE_RELEASE"
if [ "${#SOURCE_RELEASE_LINES[@]}" -ne 3 ] \
  || ! [[ "${SOURCE_RELEASE_LINES[0]}" =~ ^version=([^[:space:]]+)$ ]] \
  || ! [[ "${SOURCE_RELEASE_LINES[1]}" =~ ^commit=([0-9A-Fa-f]{12,40})$ ]] \
  || ! [[ "${SOURCE_RELEASE_LINES[2]}" =~ ^created_utc=([0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z)$ ]]; then
  echo "source release provenance is malformed" >&2
  exit 1
fi
SOURCE_VERSION="${SOURCE_RELEASE_LINES[0]#version=}"
SOURCE_COMMIT="${SOURCE_RELEASE_LINES[1]#commit=}"
SOURCE_CREATED_UTC="${SOURCE_RELEASE_LINES[2]#created_utc=}"
if [ "$(date -u -d "$SOURCE_CREATED_UTC" '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || true)" != "$SOURCE_CREATED_UTC" ]; then
  echo "source release provenance timestamp is malformed" >&2
  exit 1
fi
CARGO_VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$PROJECT_DIR/Cargo.toml" | head -n 1)"
if [ -z "$SOURCE_VERSION" ] || [ "$SOURCE_VERSION" != "$CARGO_VERSION" ]; then
  echo "source release version does not match Cargo.toml" >&2
  exit 1
fi
if [ "$TOP_LEVEL" != "webClx-$SOURCE_VERSION" ]; then
  echo "source release directory name does not match its version" >&2
  exit 1
fi
if ! awk '
  NF != 2 { exit 1 }
  $1 !~ /^[0-9A-Fa-f]{64}$/ { exit 1 }
  $2 !~ /^static\// { exit 1 }
  $2 ~ /(^|\/)\.\.($|\/)/ { exit 1 }
  END { if (NR == 0) exit 1 }
' "$PROJECT_DIR/STATIC_ASSETS_MANIFEST.sha256"; then
  echo "static asset manifest is malformed" >&2
  exit 1
fi
if ! (cd "$PROJECT_DIR" && sha256sum --check --strict --status STATIC_ASSETS_MANIFEST.sha256); then
  echo "static asset manifest verification failed" >&2
  exit 1
fi
(
  cd "$PROJECT_DIR"
  find static -type f -print0 | sort -z | xargs -0 sha256sum
) > "$STATIC_MANIFEST_ACTUAL"
if ! cmp -s "$PROJECT_DIR/STATIC_ASSETS_MANIFEST.sha256" "$STATIC_MANIFEST_ACTUAL"; then
  echo "static asset manifest does not exactly cover the static directory" >&2
  exit 1
fi

CREATED_CONTAINER=""
printf '%s\n' "$PROJECT_DIR"
