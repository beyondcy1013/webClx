#!/usr/bin/env bash
# webClx 项目级 rebuild + deploy 脚本。
#
# 解决的问题：webClx 把 `static/` 通过 `include_dir!` 嵌入到二进制中，
# 但运行时优先从磁盘读取（main.rs::resolve_static_dir），
# 源码仓库 static/ 是权威实体，运行时 /home/bin/webclx/static 必须链接到该目录。
# 本脚本校验该权威链接并把 release 二进制原子安装到运行中服务；链接损坏时
# 中止由人工修复，绝不自动创建第二份物理副本。
#
# 用法：
#   bash scripts/rebuild-and-deploy.sh             # 构建 release、校验权威链接并部署
#   bash scripts/rebuild-and-deploy.sh --skip-build # 不重新构建：安装二进制、校验链接并重启
#   WEBCLX_DEPLOY_SKIP_BUILD=1 bash ...             # queue-friendly build skip
#   bash scripts/rebuild-and-deploy.sh --help
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"
SOURCE_STATIC_DIR="$PROJECT_DIR/static"
CLI_LINK="${WEBCLX_CLI_LINK:-/usr/local/bin/webclx}"

LISTEN_PORT="${WEBCLX_PORT:-11111}"
SKIP_BUILD=false
CARGO_TARGET_DIR_ARG=""
EXPECTED_ARTIFACT_SHA256=""
if [ "${WEBCLX_DEPLOY_SKIP_BUILD:-}" = "1" ]; then
  SKIP_BUILD=true
fi

usage() {
  cat <<USAGE
用法: bash scripts/rebuild-and-deploy.sh [--skip-build] [--port <port>] [--cargo-target-dir <path>] [--expected-artifact-sha256 <hex>]

  --skip-build                 跳过 cargo build --release，但仍会安装 Cargo target
                               下现有 release 二进制、校验静态权威链接并重启 webClx 服务
  --port <port>                指定 webClx 监听端口（默认 11111）
  --cargo-target-dir <path>    指定 Cargo target 目录；canonical 解析后必须位于
                               /data/cargo-target/ 之下，构建与产物选取共用此目录
  --expected-artifact-sha256 <hex>
                               期望的 release 二进制 SHA-256（64 位小写十六进制）；
                               不匹配时在改动运行环境之前中止
  --help                       显示本帮助
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
    --cargo-target-dir)
      CARGO_TARGET_DIR_ARG="${2:-}"
      shift 2
      ;;
    --expected-artifact-sha256)
      EXPECTED_ARTIFACT_SHA256="${2:-}"
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

if [ -n "$EXPECTED_ARTIFACT_SHA256" ] \
  && ! [[ "$EXPECTED_ARTIFACT_SHA256" =~ ^[0-9a-f]{64}$ ]]; then
  echo "--expected-artifact-sha256 must be 64 lowercase hex chars" >&2
  exit 2
fi

if [ -n "$CARGO_TARGET_DIR_ARG" ]; then
  resolved_target="$(realpath -m -- "$CARGO_TARGET_DIR_ARG")"
  case "$resolved_target" in
    /data/cargo-target/*) ;;
    *)
      echo "--cargo-target-dir must canonical-resolve under /data/cargo-target/ (got $resolved_target)" >&2
      exit 2
      ;;
  esac
  export CARGO_TARGET_DIR="$resolved_target"
fi

# ---------------------------------------------------------------------------
# 0. 阻止项目图标身份、回退渲染或静态资源版本回归进入运行环境
# ---------------------------------------------------------------------------
log "running workspace icon deployment gate"
bash "$SCRIPT_DIR/check-workspace-icon-gate.sh"

# ---------------------------------------------------------------------------
# 1. 解析唯一的 Cargo target 目录，再构建 release
# ---------------------------------------------------------------------------
# 产物选取、安装与哈希核验必须指向同一份 target 物理目录：无论来自
# --cargo-target-dir、环境 CARGO_TARGET_DIR 还是 cargo metadata 默认
# 值，canonical 后都必须位于 /data/cargo-target/ 之下，否则部署链可能
# 核验一份产物却安装另一份。
TARGET_DIR="$(cd "$PROJECT_DIR" && cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')"
TARGET_DIR="$(realpath -m -- "$TARGET_DIR")"
case "$TARGET_DIR" in
  /data/cargo-target/*) ;;
  *)
    log "cargo target_directory resolves outside /data/cargo-target: $TARGET_DIR"
    exit 2
    ;;
esac
BUILT_BINARY="$TARGET_DIR/release/webclx"
log "cargo_target_dir=$TARGET_DIR"

if [ "$SKIP_BUILD" = false ]; then
  log "running: cargo build --release (project_dir=$PROJECT_DIR)"
  ( cd "$PROJECT_DIR" && cargo build --release )
fi

# ---------------------------------------------------------------------------
# 1.5 运行环境改动前的产物预检：存在性 + SHA-256 + 期望哈希比对
# ---------------------------------------------------------------------------
# 无论刚构建还是复用既有产物（--skip-build），安装步骤都依赖该二进制。
if [ ! -x "$BUILT_BINARY" ]; then
  log "built binary not found at $BUILT_BINARY"
  exit 1
fi
ARTIFACT_SHA256="$(sha256sum "$BUILT_BINARY" | awk '{print $1}')"
log "artifact_sha256=$ARTIFACT_SHA256"
if [ -n "$EXPECTED_ARTIFACT_SHA256" ] && [ "$ARTIFACT_SHA256" != "$EXPECTED_ARTIFACT_SHA256" ]; then
  log "artifact sha256 mismatch: expected $EXPECTED_ARTIFACT_SHA256, got $ARTIFACT_SHA256"
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
  log "no running webClx process listening on port $LISTEN_PORT; skip deploy."
  log "if you expected a deploy, start the service first or set WEBCLX_PORT."
  exit 0
fi

APP_DIR="$(readlink "/proc/$RUNNING_PID/cwd" 2>/dev/null || true)"
EXE_PATH="$(readlink "/proc/$RUNNING_PID/exe" 2>/dev/null || true)"
if [ -z "$APP_DIR" ] || [ ! -d "$APP_DIR" ]; then
  log "cannot resolve /proc/$RUNNING_PID/cwd; aborting deploy."
  exit 1
fi
log "running webClx pid=$RUNNING_PID cwd=$APP_DIR"

if [ -z "$EXE_PATH" ]; then
  log "cannot resolve running executable path; aborting binary deploy."
  exit 1
fi

# ---------------------------------------------------------------------------
# 3. 预检 static_dir：复刻 main.rs::resolve_static_dir 的解析顺序，
#    并验证符号链接权威——必须在替换运行中二进制之前完成。
# ---------------------------------------------------------------------------
TARGET_STATIC_DIR=""

if [ -n "${WEBCLX_STATIC_DIR:-}" ] && [ -d "${WEBCLX_STATIC_DIR}" ]; then
  TARGET_STATIC_DIR="${WEBCLX_STATIC_DIR}"
fi

if [ -z "$TARGET_STATIC_DIR" ] && [ -f "$APP_DIR/static/index.html" ]; then
  TARGET_STATIC_DIR="$APP_DIR/static"
fi

if [ -z "$TARGET_STATIC_DIR" ]; then
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

if [ -z "$TARGET_STATIC_DIR" ]; then
  log "cannot determine running service static_dir from cwd/exe ancestry."
  log "fallback to: $APP_DIR/static (must resolve to repository static)"
  TARGET_STATIC_DIR="$APP_DIR/static"
fi

SOURCE_REAL="$(cd "$SOURCE_STATIC_DIR" && pwd -P)"
# static 权威唯一实体是源码目录：运行链接与实际服务目录必须 canonical
# 解析到同一 SOURCE_REAL，否则中止；不迁移或同步第二份物理副本。
TARGET_REAL="$(readlink -f -- "$TARGET_STATIC_DIR")"
if [ "$TARGET_REAL" != "$SOURCE_REAL" ]; then
  log "static directory must resolve to the authoritative source static; repair runtime symlink before deployment"
  exit 1
fi

if [ -e "$CLI_LINK" ] && [ ! -L "$CLI_LINK" ]; then
  log "refusing to replace non-symlink CLI path: $CLI_LINK"
  exit 1
fi

# ---------------------------------------------------------------------------
# 4. 替换运行中二进制：stage → 校验 → 备份 → 原子替换 → 校验后重启
# ---------------------------------------------------------------------------
# 先把构建产物安装到同目录的自有临时文件并复核哈希，只有与所选产物一致
# 才触碰运行中的二进制与备份；trap 只清理本脚本创建的 stage 文件。
STAGED_BINARY="$(mktemp "$(dirname "$EXE_PATH")/.webclx-install.XXXXXX")"
trap 'rm -f "$STAGED_BINARY"' EXIT
install -m 0755 "$BUILT_BINARY" "$STAGED_BINARY"
STAGED_SHA256="$(sha256sum "$STAGED_BINARY" | awk '{print $1}')"
if [ "$STAGED_SHA256" != "$ARTIFACT_SHA256" ]; then
  log "staged binary sha256 mismatch (artifact=$ARTIFACT_SHA256 staged=$STAGED_SHA256); aborting before touching runtime."
  exit 1
fi
log "installing binary"
log "  source: $BUILT_BINARY"
log "  target: $EXE_PATH"
# 更新二进制前保留一个备份 (固定名 .bak, 每次覆盖, 始终只留一个)
if [ -f "$EXE_PATH" ]; then
  cp -p "$EXE_PATH" "$EXE_PATH.bak"
fi
mv -f "$STAGED_BINARY" "$EXE_PATH"
INSTALLED_SHA256="$(sha256sum "$EXE_PATH" | awk '{print $1}')"
log "installed_sha256=$INSTALLED_SHA256"
if [ "$INSTALLED_SHA256" != "$ARTIFACT_SHA256" ]; then
  log "installed binary sha256 mismatch (artifact=$ARTIFACT_SHA256 installed=$INSTALLED_SHA256); aborting before restart."
  exit 1
fi

# Keep the compile worker in the runtime tree aligned with the deployed source.
# It is executed by transient compile units and is not embedded in the binary.
WORKER_SOURCE="$PROJECT_DIR/docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh"
WORKER_TARGET="$APP_DIR/docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh"
if [ -f "$WORKER_SOURCE" ] && [ -d "$(dirname "$WORKER_TARGET")" ]; then
  install -m 0755 "$WORKER_SOURCE" "$WORKER_TARGET"
fi

install -d -m 0755 "$(dirname "$CLI_LINK")"
ln -sfn "$EXE_PATH" "$CLI_LINK"
log "CLI link: $CLI_LINK -> $EXE_PATH"

# ---------------------------------------------------------------------------
# 5. 静态资源：权威链接已在二进制替换前验证，共享 static 无需同步
# ---------------------------------------------------------------------------
log "shared static already authoritative: $TARGET_STATIC_DIR -> $SOURCE_REAL"

log "restarting webClx service"
systemctl restart webclx.service
