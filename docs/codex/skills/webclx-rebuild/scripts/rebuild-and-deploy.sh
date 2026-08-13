#!/usr/bin/env bash
# Rebuild webClx release binary and deploy to the running service.
# Usage: bash rebuild-and-deploy.sh [--skip-sync] [--skip-restart]
set -euo pipefail

REPO_DIR="/home/codes/webClx"
DEPLOY_BIN="/home/bin/webclx/webClx"
DEPLOY_STATIC="/home/bin/webclx/static"
SERVICE_NAME="webclx.service"

SKIP_SYNC=false
SKIP_RESTART=false
for arg in "$@"; do
  case "$arg" in
    --skip-sync)   SKIP_SYNC=true ;;
    --skip-restart) SKIP_RESTART=true ;;
  esac
done

cd "$REPO_DIR"

# 1. Determine Cargo target directory
TARGET_DIR=$(cargo metadata --no-deps --format-version 1 2>/dev/null | jq -r '.target_directory')
if [ -z "$TARGET_DIR" ] || [ "$TARGET_DIR" = "null" ]; then
  echo "ERROR: could not determine Cargo target_directory"
  exit 1
fi
echo "Target directory: $TARGET_DIR"

# 2. Build release
echo "Building release..."
cargo build --release 2>&1

# 3. Install binary
echo "Installing binary to $DEPLOY_BIN ..."
install -m 0755 "$TARGET_DIR/release/webclx" "$DEPLOY_BIN"

# 4. Sync static files
if [ "$SKIP_SYNC" = false ]; then
  echo "Syncing static files to $DEPLOY_STATIC ..."
  rsync -a --delete "$REPO_DIR/static/" "$DEPLOY_STATIC/"
else
  echo "Skipping static file sync (--skip-sync)."
fi

# 5. Restart service
if [ "$SKIP_RESTART" = false ]; then
  echo "Restarting $SERVICE_NAME ..."
  systemctl restart "$SERVICE_NAME"
  sleep 1
  systemctl status "$SERVICE_NAME" --no-pager -l
else
  echo "Skipping service restart (--skip-restart)."
fi

echo "Done."
