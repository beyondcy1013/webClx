#!/usr/bin/env bash
set -euo pipefail

REPO_API="${WEBCLX_RELEASE_API:-https://api.github.com/repos/beyondcy1013/webClx/releases}"
VERSION=""
PREFIX="${WEBCLX_INSTALL_PREFIX:-$HOME/.local}"
START=true

usage() {
  cat <<'USAGE'
Usage: curl -fsSL https://raw.githubusercontent.com/beyondcy1013/webClx/main/install.sh | bash

Install webClx from its latest verified GitHub release.

Options:
  --version <version>   Install a release tag such as 1.10.22 (default: latest)
  --prefix <path>       Installation prefix (default: ~/.local)
  --no-start            Install without starting webClx
  --url <api-url>       GitHub releases API base URL
  -h, --help            Show this help

Environment:
  WEBCLX_RELEASE_API      Same as --url
  WEBCLX_INSTALL_PREFIX   Same as --prefix
USAGE
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "webClx installer requires $1" >&2
    exit 1
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || { echo "missing value for $1" >&2; exit 2; }
      VERSION="${2#v}"
      shift 2
      ;;
    --prefix)
      [ "$#" -ge 2 ] || { echo "missing value for $1" >&2; exit 2; }
      PREFIX="$2"
      shift 2
      ;;
    --no-start)
      START=false
      shift
      ;;
    --url)
      [ "$#" -ge 2 ] || { echo "missing value for $1" >&2; exit 2; }
      REPO_API="$2"
      shift 2
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

for command in curl jq tar sha256sum uname; do
  need "$command"
done

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) PLATFORM="linux-x86_64"; BINARY="webclx" ;;
  Linux-aarch64) PLATFORM="linux-arm64"; BINARY="webclx" ;;
  Darwin-x86_64|Darwin-arm64)
    echo "webClx does not publish macOS release packages; build from the source release instead" >&2
    exit 2
    ;;
  *)
    echo "unsupported platform: $(uname -s)-$(uname -m)" >&2
    exit 2
    ;;
esac

if [ -n "$VERSION" ]; then
  RELEASE_JSON="$(curl -fsSL "$REPO_API/tags/v$VERSION")"
else
  RELEASE_JSON="$(curl -fsSL "$REPO_API/latest")"
fi

VERSION="$(printf '%s' "$RELEASE_JSON" | jq -r '.tag_name // empty' | sed 's/^v//')"
if [ -z "$VERSION" ] || [ "$VERSION" = "null" ]; then
  echo "cannot determine the selected webClx release version" >&2
  exit 1
fi

PACKAGE="webClx-$VERSION-$PLATFORM.tar.gz"
ASSET_URL="$(printf '%s' "$RELEASE_JSON" | jq -r --arg name "$PACKAGE" '.assets[]? | select(.name == $name) | .browser_download_url' | head -n 1)"
CHECKSUM_URL="$(printf '%s' "$RELEASE_JSON" | jq -r --arg name "$PACKAGE.sha256" '.assets[]? | select(.name == $name) | .browser_download_url' | head -n 1)"
if [ -z "$ASSET_URL" ] || [ -z "$CHECKSUM_URL" ] || [ "$ASSET_URL" = "null" ] || [ "$CHECKSUM_URL" = "null" ]; then
  echo "release v$VERSION does not contain required asset: $PACKAGE(.sha256)" >&2
  exit 1
fi

STAGE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/webclx-install.XXXXXX")"
ARCHIVE="$STAGE_ROOT/$PACKAGE"
CHECKSUM_FILE="$ARCHIVE.sha256"
INSTALL_ROOT="$(mkdir -p "$PREFIX" && cd "$PREFIX" && pwd -P)"
VERSION_DIR="$INSTALL_ROOT/webclx-$VERSION"
NEXT_DIR="$INSTALL_ROOT/.webclx-$VERSION.next.$$"

curl -fsSL --retry 3 -o "$ARCHIVE" "$ASSET_URL"
curl -fsSL --retry 3 -o "$CHECKSUM_FILE" "$CHECKSUM_URL"

EXPECTED_SHA="$(awk 'NF >= 1 { print $1; exit }' "$CHECKSUM_FILE")"
if ! [[ "$EXPECTED_SHA" =~ ^[0-9A-Fa-f]{64}$ ]]; then
  echo "malformed SHA-256 checksum file for $PACKAGE" >&2
  exit 1
fi
ACTUAL_SHA="$(sha256sum "$ARCHIVE" | awk '{ print $1 }')"
if [ "${ACTUAL_SHA,,}" != "${EXPECTED_SHA,,}" ]; then
  echo "SHA-256 mismatch for $PACKAGE" >&2
  exit 1
fi

mkdir "$NEXT_DIR"
tar -xzf "$ARCHIVE" -C "$NEXT_DIR"
PACKAGE_DIR="$NEXT_DIR/webClx-$VERSION-$PLATFORM"
if [ ! -x "$PACKAGE_DIR/$BINARY" ] || [ ! -f "$PACKAGE_DIR/static/index.html" ]; then
  echo "release package layout is invalid" >&2
  exit 1
fi

if [ -e "$VERSION_DIR" ]; then
  BACKUP="$INSTALL_ROOT/.webclx-$VERSION.previous.$$"
  mv "$VERSION_DIR" "$BACKUP"
fi
mv "$PACKAGE_DIR" "$VERSION_DIR"

printf '%s\n' "$VERSION_DIR"

if [ "$START" = true ]; then
  echo "Starting webClx. Open http://127.0.0.1:11111 when it is ready."
  echo "The first-run password is stored in .webclx-initial-password under the process working directory."
  (cd "$VERSION_DIR" && exec "./$BINARY" serve)
else
  echo "Installed webClx $VERSION at $VERSION_DIR"
  echo "Run: cd \"$VERSION_DIR\" && ./webclx serve"
fi
