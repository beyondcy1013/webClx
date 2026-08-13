#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"
VERSION="$(git -C "$PROJECT_DIR" show HEAD:Cargo.toml | sed -n 's/^version = "\([^"]*\)"/\1/p' | head -n 1)"
COMMIT="$(git -C "$PROJECT_DIR" rev-parse --short=12 HEAD)"
OUTPUT="${1:-$PROJECT_DIR/dist/webClx-$VERSION-source.tar.gz}"
STAGE_ROOT="$(mktemp -d)"
ARCHIVE_PART="$OUTPUT.$$.part"
CHECKSUM_PART="$OUTPUT.sha256.$$.part"
trap 'rm -rf "$STAGE_ROOT"; rm -f "$ARCHIVE_PART" "$CHECKSUM_PART"' EXIT
STAGE="$STAGE_ROOT/webClx-$VERSION"

if [ -z "$VERSION" ]; then
  echo "cannot determine package version" >&2
  exit 1
fi
if [ ! -f "$PROJECT_DIR/static/index.html" ] || [ ! -f "$PROJECT_DIR/static/i18n.js" ]; then
  echo "static source is incomplete; expected static/index.html and static/i18n.js" >&2
  exit 1
fi

mkdir -p "$STAGE" "$(dirname "$OUTPUT")"
git -C "$PROJECT_DIR" archive HEAD | tar -x -C "$STAGE"
rm -f "$STAGE/static"
cp -aL "$PROJECT_DIR/static" "$STAGE/static"
rm -rf \
  "$STAGE/.claude" \
  "$STAGE/.codex/plans" \
  "$STAGE/.codex/skills/webclx-nas-deploy" \
  "$STAGE/.codex/skills/webclx-remote-deploy" \
  "$STAGE/.codex/skills/webclx-windows-deploy" \
  "$STAGE/.codex/skills/webclx-workspace-icon-setting" \
  "$STAGE/.qoder" \
  "$STAGE/.zcode" \
  "$STAGE/docs/cross-model-verification"
rm -f "$STAGE/AGENTS.MD" "$STAGE/scripts/deploy-remote-servers.sh"

ARCHIVE_VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$STAGE/Cargo.toml" | head -n 1)"
if [ "$ARCHIVE_VERSION" != "$VERSION" ]; then
  echo "archive version mismatch: expected $VERSION, got $ARCHIVE_VERSION" >&2
  exit 1
fi

find "$STAGE" -name '__pycache__' -type d -prune -exec rm -rf {} +
find "$STAGE" -type f \( -name '*.pyc' -o -name '*.log' -o -name '.webclx-*' \) -delete
( cd "$STAGE" && find static -type f -print0 | sort -z | xargs -0 sha256sum ) \
  > "$STAGE/STATIC_ASSETS_MANIFEST.sha256"
printf 'version=%s\ncommit=%s\ncreated_utc=%s\n' \
  "$VERSION" "$COMMIT" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$STAGE/SOURCE_RELEASE"

tar -C "$STAGE_ROOT" -czf "$ARCHIVE_PART" "$(basename "$STAGE")"
mv "$ARCHIVE_PART" "$OUTPUT"
( cd "$(dirname "$OUTPUT")" && sha256sum "$(basename "$OUTPUT")" ) > "$CHECKSUM_PART"
mv "$CHECKSUM_PART" "$OUTPUT.sha256"
printf '%s\n' "$OUTPUT"
cat "$OUTPUT.sha256"
