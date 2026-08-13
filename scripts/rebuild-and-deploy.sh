#!/usr/bin/env bash
# webClx 项目级 rebuild + deploy 脚本。
#
# 解决的问题：webClx 把 `static/` 通过 `include_dir!` 嵌入到二进制中，
# 但运行时优先从磁盘读取（main.rs::resolve_static_dir），
# 而且运行时使用的 static_dir 是 进程 CWD 下的 `static/`，不一定等于源码仓库的 `static/`。
# 编译只更新二进制和源码仓库的 `static/`，必须把最新 static 同步到运行服务的 static_dir，
# 浏览器刷新后才会看到改动。
#
# 用法：
#   bash scripts/rebuild-and-deploy.sh             # 构建 release 并同步静态资源
#   bash scripts/rebuild-and-deploy.sh --skip-build # 仅同步静态资源
#   bash scripts/rebuild-and-deploy.sh --help
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"
SOURCE_STATIC_DIR="$PROJECT_DIR/static"
CLI_LINK="${WEBCLX_CLI_LINK:-/usr/local/bin/webclx}"

LISTEN_PORT="${WEBCLX_PORT:-11111}"
SKIP_BUILD=false

usage() {
  cat <<USAGE
用法: bash scripts/rebuild-and-deploy.sh [--skip-build] [--port <port>]

  --skip-build  跳过 cargo build --release，仅同步静态资源到运行服务的 static_dir
  --port <port> 指定 webClx 监听端口（默认 11111）
  --help        显示本帮助
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-build)
      SKIP_BUILD=true
      shift
      ;;
    --port)
      LISTEN_PORT="${2:-}"
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

log() { printf '[rebuild-and-deploy] %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------
# 1. cargo build --release
# ---------------------------------------------------------------------------
if [ "$SKIP_BUILD" = false ]; then
  log "running: cargo build --release (project_dir=$PROJECT_DIR)"
  ( cd "$PROJECT_DIR" && cargo build --release )
else
  log "skip-build set; assuming target/release/webclx is current"
fi

TARGET_DIR="$(cd "$PROJECT_DIR" && cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')"
BUILT_BINARY="$TARGET_DIR/release/webclx"
# The binary must exist whether we built it just now or are reusing a
# pre-built one (--skip-build); the install step below depends on it.
if [ ! -x "$BUILT_BINARY" ]; then
  log "built binary not found at $BUILT_BINARY"
  exit 1
fi

# ---------------------------------------------------------------------------
# 2. 定位运行中 webClx 进程的 app_dir
# ---------------------------------------------------------------------------
find_running_webclx_pid() {
  # 通过 ss 找到监听 LISTEN_PORT 的可执行文件 webClx 的 pid。
  # webClx 通常以 webClx 大写二进制名运行（项目里 release 产物叫 webclx，
  # 但部署目录里通常是 webClx），所以这里宽匹配 'webClx'。
  ss -ltnp 2>/dev/null \
    | awk -v p=":$LISTEN_PORT" '$4 ~ p {print}' \
    | grep -oE 'pid=[0-9]+' \
    | head -n 1 \
    | cut -d= -f2
}

RUNNING_PID="$(find_running_webclx_pid || true)"
if [ -z "$RUNNING_PID" ]; then
  log "no running webClx process listening on port $LISTEN_PORT; skip static sync."
  log "if you expected a deploy, start the service first or set WEBCLX_PORT."
  exit 0
fi

APP_DIR="$(readlink "/proc/$RUNNING_PID/cwd" 2>/dev/null || true)"
EXE_PATH="$(readlink "/proc/$RUNNING_PID/exe" 2>/dev/null || true)"
if [ -z "$APP_DIR" ] || [ ! -d "$APP_DIR" ]; then
  log "cannot resolve /proc/$RUNNING_PID/cwd; aborting static sync."
  exit 1
fi
log "running webClx pid=$RUNNING_PID cwd=$APP_DIR"

if [ -z "$EXE_PATH" ]; then
  log "cannot resolve running executable path; aborting binary deploy."
  exit 1
fi
log "installing binary"
log "  source: $BUILT_BINARY"
log "  target: $EXE_PATH"
# 更新二进制前保留一个备份 (固定名 .bak, 每次覆盖, 始终只留一个)
if [ -f "$EXE_PATH" ]; then
  mv -f "$EXE_PATH" "$EXE_PATH.bak"
fi
install -m 0755 "$BUILT_BINARY" "$EXE_PATH"

if [ -e "$CLI_LINK" ] && [ ! -L "$CLI_LINK" ]; then
  log "refusing to replace non-symlink CLI path: $CLI_LINK"
  exit 1
fi
install -d -m 0755 "$(dirname "$CLI_LINK")"
ln -sfn "$EXE_PATH" "$CLI_LINK"
log "CLI link: $CLI_LINK -> $EXE_PATH"

# ---------------------------------------------------------------------------
# 3. 复刻 main.rs::resolve_static_dir 的解析顺序，决定目标 static_dir
# ---------------------------------------------------------------------------
TARGET_STATIC_DIR=""

if [ -n "${WEBCLX_STATIC_DIR:-}" ] && [ -d "${WEBCLX_STATIC_DIR}" ]; then
  TARGET_STATIC_DIR="${WEBCLX_STATIC_DIR}"
fi

if [ -z "$TARGET_STATIC_DIR" ] && [ -f "$APP_DIR/static/index.html" ]; then
  TARGET_STATIC_DIR="$APP_DIR/static"
fi

if [ -z "$TARGET_STATIC_DIR" ]; then
  if [ -n "$EXE_PATH" ]; then
    EXE_DIR="$(dirname "$EXE_PATH")"
    # main.rs 向上回溯最多 4 层祖先。
    for ancestor in "$EXE_DIR" \
                    "$(dirname "$EXE_DIR")" \
                    "$(dirname "$(dirname "$EXE_DIR")")" \
                    "$(dirname "$(dirname "$(dirname "$EXE_DIR")")")"; do
      if [ -f "$ancestor/static/index.html" ]; then
        TARGET_STATIC_DIR="$ancestor/static"
        break
      fi
    done
  fi
fi

if [ -z "$TARGET_STATIC_DIR" ]; then
  log "cannot determine running service static_dir from cwd/exe ancestry."
  log "fallback to: $APP_DIR/static (will be created if missing)"
  TARGET_STATIC_DIR="$APP_DIR/static"
fi

# 源和目标相同就跳过
SOURCE_REAL="$(cd "$SOURCE_STATIC_DIR" && pwd -P)"
TARGET_REAL="$(mkdir -p "$TARGET_STATIC_DIR" && cd "$TARGET_STATIC_DIR" && pwd -P)"
if [ "$SOURCE_REAL" = "$TARGET_REAL" ]; then
  log "source and target static dir are identical ($SOURCE_REAL); skipping static sync."
  log "binary is already installed above; continuing to restart webClx service."
else
  # -------------------------------------------------------------------------
  # 4. 同步：保留目标目录的运行时文件（.bak-*、.webclx-*），用 rsync 优先，
  #    没有 rsync 时退而用 cp -ru。
  # -------------------------------------------------------------------------
  log "syncing static assets"
  log "  source: $SOURCE_STATIC_DIR"
  log "  target: $TARGET_STATIC_DIR"

  if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete --exclude='.bak-*' --exclude='.webclx-*' \
      "$SOURCE_STATIC_DIR/" "$TARGET_STATIC_DIR/"
  else
    # cp -ru 不会删除目标多余文件；先清理目标里源不再有的同名条目（保留 .bak-*/.webclx-*）。
    if [ -d "$SOURCE_STATIC_DIR" ]; then
      find "$TARGET_STATIC_DIR" -mindepth 1 -maxdepth 1 \
        ! -name '.bak-*' ! -name '.webclx-*' \
        -exec rm -rf {} +
      cp -r "$SOURCE_STATIC_DIR"/. "$TARGET_STATIC_DIR"/
    fi
  fi

  log "static sync complete"
fi

log "restarting webClx service"
systemctl restart webclx.service
