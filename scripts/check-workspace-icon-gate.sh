#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"
PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-/home/codes/third_party/browser-tools/playwright/beyondcy-ms-playwright}"
export PLAYWRIGHT_BROWSERS_PATH

cd "$PROJECT_DIR"

# 静态实体目录位于源码仓库；运行目录 static 是指向该实体的绝对符号链接。
# 同时检查链接方向和 canonical 目标，拒绝第二份副本。
STATIC_TARGET="$(readlink -f static)"
RUNTIME_TARGET="$(readlink -f /home/bin/webclx/static 2>/dev/null || true)"
if [ -L static ] || [ ! -L /home/bin/webclx/static ] || [ "$(readlink /home/bin/webclx/static)" != "$PROJECT_DIR/static" ] || [ "$STATIC_TARGET" != "$PROJECT_DIR/static" ] || [ "$STATIC_TARGET" != "$RUNTIME_TARGET" ]; then
  echo "workspace icon gate: runtime static must link to the physical $PROJECT_DIR/static (got $STATIC_TARGET / $RUNTIME_TARGET)" >&2
  exit 1
fi

node tests/workspace-project-icons.test.mjs
log() { printf '[workspace icon deployment gate] %s\n' "$*" >&2; }
log "using Playwright browsers at $PLAYWRIGHT_BROWSERS_PATH"
python3 tests/workspace_project_icons.browser.py
cargo test filesystem::tests::workspace_icon --bin webclx

echo "workspace icon deployment gate passed"
