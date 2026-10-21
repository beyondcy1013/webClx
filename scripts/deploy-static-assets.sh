#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"
SOURCE_STATIC_DIR="$PROJECT_DIR/static"
TARGET_STATIC_DIR="${WEBCLX_STATIC_DEPLOY_DIR:-/home/bin/webclx/static}"

if [ "$#" -eq 0 ]; then
  echo "usage: $0 <static-relative-file> [...]" >&2
  exit 2
fi

install -d -m 0755 "$TARGET_STATIC_DIR"
TARGET_STATIC_DIR="$(cd "$TARGET_STATIC_DIR" && pwd -P)"

for relative_path in "$@"; do
  case "$relative_path" in
    ""|/*|..|../*|*/..|*/../*)
      echo "invalid static asset path: $relative_path" >&2
      exit 2
      ;;
  esac

  source_path="$SOURCE_STATIC_DIR/$relative_path"
  target_path="$TARGET_STATIC_DIR/$relative_path"
  if [ ! -f "$source_path" ]; then
    echo "static asset not found: $source_path" >&2
    exit 1
  fi

  install -d -m 0755 "$(dirname "$target_path")"
  install -m 0644 "$source_path" "$target_path"
  cmp -s "$source_path" "$target_path"
  printf '[deploy-static-assets] installed %s\n' "$relative_path" >&2
done
