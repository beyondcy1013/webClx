#!/usr/bin/env bash
# 统一只读检查入口（docs/codex/tasks/repo-check-entry.md）：
#   rust    cargo fmt --all --check / cargo check --workspace / cargo test --workspace
#   node    find tests -name '*.test.mjs' 的全部 Node 契约测试
#   browser 固定清单的关键 Playwright 浏览器冒烟
# 每条命令记录退出码、耗时和日志路径，报告写入 report.json 与 report.txt。
# 本机运行 webClx 自身编译/测试须走编译队列；rust 组仅在允许直接构建的环境使用。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"

cd "$PROJECT_DIR"

GROUP_RUST=0
GROUP_NODE=0
GROUP_BROWSER=0
LIST_ONLY=0
FILTER=""
LOG_ROOT="${WEBCLX_REPO_CHECK_LOG_DIR:-/tmp/webclx-repo-check}"
TEST_TIMEOUT="${WEBCLX_REPO_CHECK_TEST_TIMEOUT:-600}"
BROWSER_SMOKE_TESTS=(
  tests/terminal-session-switch-focus.browser.py
  tests/workspace_project_icons.browser.py
  tests/codex_status_compact.browser.py
)

usage() {
  cat <<'EOF'
用法: scripts/repo-check.sh [--rust] [--node] [--browser] [--all] [--list] [--filter SUBSTR]
                            [--log-dir DIR] [--timeout SECS]

  --rust           Rust 检查：cargo fmt --all --check、cargo check --workspace、cargo test --workspace
  --node           全部 Node 契约测试（find tests -name '*.test.mjs'，逐条 timeout --signal=KILL）
  --browser        关键浏览器冒烟（固定清单，Playwright 自包含，无需真实服务）
  --all            三组全部执行（默认）
  --list           只打印将执行的命令集，不执行（用于两次触发比对）
  --filter SUBSTR  只保留路径含 SUBSTR 的测试（node/browser 组）
  --log-dir DIR    日志与报告目录（默认 /tmp/webclx-repo-check）
  --timeout SECS   单条测试命令的 timeout --signal=KILL 上限（默认 600）

环境变量: WEBCLX_REPO_CHECK_LOG_DIR、WEBCLX_REPO_CHECK_TEST_TIMEOUT 与对应参数等价。
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --rust) GROUP_RUST=1 ;;
    --node) GROUP_NODE=1 ;;
    --browser) GROUP_BROWSER=1 ;;
    --all) GROUP_RUST=1; GROUP_NODE=1; GROUP_BROWSER=1 ;;
    --list) LIST_ONLY=1 ;;
    --filter) FILTER="${2:?--filter 需要参数}"; shift ;;
    --log-dir) LOG_ROOT="${2:?--log-dir 需要参数}"; shift ;;
    --timeout) TEST_TIMEOUT="${2:?--timeout 需要参数}"; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "未知参数: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [ "$GROUP_RUST$GROUP_NODE$GROUP_BROWSER" = "000" ]; then
  GROUP_RUST=1; GROUP_NODE=1; GROUP_BROWSER=1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
COMMIT="$(git rev-parse --short=12 HEAD)"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  DIRTY=true
else
  DIRTY=false
fi
NODE_VERSION="$(node --version)"
PYTHON_VERSION="$(python3 --version 2>&1 | awk '{print $2}')"
KERNEL="$(uname -s)-$(uname -m)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

collect_node_tests() {
  find tests -name '*.test.mjs' | sort | while IFS= read -r t; do
    if [ -z "$FILTER" ] || [[ "${t#tests/}" == *"$FILTER"* ]]; then
      printf '%s\n' "$t"
    fi
  done
}

collect_browser_tests() {
  local t
  for t in "${BROWSER_SMOKE_TESTS[@]}"; do
    if [ -z "$FILTER" ] || [[ "${t#tests/}" == *"$FILTER"* ]]; then
      printf '%s\n' "$t"
    fi
  done
}

print_plan_line() {
  printf '%s\n' "$*"
}

CMD_IDS=()
CMD_ARGVS=()
CMD_RCS=()
CMD_DURATIONS=()
CMD_LOGS=()

run_cmd() {
  local id="$1"; shift
  local log="$1"; shift
  local argv_display start end rc
  argv_display="$*"
  CMD_IDS+=("$id")
  CMD_ARGVS+=("$argv_display")
  CMD_LOGS+=("$log")
  if [ "$LIST_ONLY" = "1" ]; then
    print_plan_line "$argv_display"
    CMD_RCS+=("")
    CMD_DURATIONS+=("")
    return 0
  fi
  mkdir -p "$(dirname "$log")"
  start="$(date +%s%3N)"
  set +e
  timeout --signal=KILL "$TEST_TIMEOUT" "$@" >"$log" 2>&1
  rc=$?
  set -e
  end="$(date +%s%3N)"
  CMD_RCS+=("$rc")
  CMD_DURATIONS+=("$((end - start))")
  printf '[rc=%s %sms] %s\n' "$rc" "$((end - start))" "$argv_display"
  return 0
}

LOG_DIR="${LOG_ROOT}/${STAMP}-${COMMIT}"
if [ "$DIRTY" = "true" ]; then
  LOG_DIR="${LOG_DIR}-dirty"
fi

RUST_TARGET_NOTE=""
if [ "$GROUP_RUST" = "1" ]; then
  if [ -z "${CARGO_TARGET_DIR:-}" ] || [[ "${CARGO_TARGET_DIR:-}" != /data/cargo-target/* ]]; then
    CARGO_TARGET_DIR="/data/cargo-target/local/webclx-repo-check"
    export CARGO_TARGET_DIR
    RUST_TARGET_NOTE="CARGO_TARGET_DIR 未指向 /data/cargo-target/，已改用 ${CARGO_TARGET_DIR}"
  fi
fi

NODE_TESTS="$(collect_node_tests)"
BROWSER_TESTS_SELECTED="$(collect_browser_tests)"

if [ "$GROUP_NODE" = "1" ] && [ -z "$NODE_TESTS" ]; then
  echo "repo-check: --filter \"${FILTER}\" 未匹配任何 Node 测试" >&2
  exit 2
fi
if [ "$GROUP_BROWSER" = "1" ] && [ -z "$BROWSER_TESTS_SELECTED" ]; then
  echo "repo-check: --filter \"${FILTER}\" 未匹配任何浏览器冒烟测试" >&2
  exit 2
fi

echo "repo-check ${VERSION} @ ${COMMIT} (dirty=${DIRTY}, ${KERNEL}, node ${NODE_VERSION}, python ${PYTHON_VERSION})"
echo "日志目录: ${LOG_DIR}"

if [ "$GROUP_RUST" = "1" ]; then
  run_cmd "rust/cargo-fmt-check" "${LOG_DIR}/rust/cargo-fmt-check.log" cargo fmt --all --check
  run_cmd "rust/cargo-check" "${LOG_DIR}/rust/cargo-check.log" cargo check --workspace
  run_cmd "rust/cargo-test" "${LOG_DIR}/rust/cargo-test.log" cargo test --workspace
fi

if [ "$GROUP_NODE" = "1" ]; then
  while IFS= read -r t; do
    [ -n "$t" ] || continue
    name="${t#tests/}"; name="${name//\//__}"
    run_cmd "node/${name}" "${LOG_DIR}/node/${name}.log" node "$t"
  done <<<"$NODE_TESTS"
fi

if [ "$GROUP_BROWSER" = "1" ]; then
  while IFS= read -r t; do
    [ -n "$t" ] || continue
    name="${t#tests/}"; name="${name//\//__}"
    run_cmd "browser/${name}" "${LOG_DIR}/browser/${name}.log" python3 "$t"
  done <<<"$BROWSER_TESTS_SELECTED"
fi

if [ "$LIST_ONLY" = "1" ]; then
  exit 0
fi

FAILED_IDS=()
PASS_COUNT=0
FAIL_COUNT=0
for i in "${!CMD_IDS[@]}"; do
  rc="${CMD_RCS[$i]}"
  if [ "$rc" = "0" ]; then
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_IDS+=("${CMD_IDS[$i]}")
  fi
done

JSONL="$(mktemp)"
: >"$JSONL"
for i in "${!CMD_IDS[@]}"; do
  jq -n --arg id "${CMD_IDS[$i]}" --arg cmd "${CMD_ARGVS[$i]}" --argjson rc "${CMD_RCS[$i]}" \
    --argjson ms "${CMD_DURATIONS[$i]}" --arg log "${CMD_LOGS[$i]}" \
    '{id: $id, command: $cmd, exit_code: $rc, duration_ms: $ms, log: $log}' >>"$JSONL"
done

FAILED_JSON="$(printf '%s\n' "${FAILED_IDS[@]:-}" | jq -R . | jq -s 'map(select(length > 0))')"

jq -n \
  --arg version "$VERSION" \
  --arg commit "$COMMIT" \
  --argjson dirty "$DIRTY" \
  --arg kernel "$KERNEL" \
  --arg node_version "$NODE_VERSION" \
  --arg python_version "$PYTHON_VERSION" \
  --arg stamp "$STAMP" \
  --arg log_dir "$LOG_DIR" \
  --arg rust_target_note "$RUST_TARGET_NOTE" \
  --arg timeout "$TEST_TIMEOUT" \
  --argjson commands "$(jq -s '.' "$JSONL")" \
  --argjson failed "$FAILED_JSON" \
  '{version: $version, commit: $commit, dirty: $dirty, kernel: $kernel,
    node: $node_version, python: $python_version, started_at_utc: $stamp,
    per_command_timeout_secs: ($timeout | tonumber),
    cargo_target_note: $rust_target_note,
    summary: {total: ($commands | length), passed: ($commands | map(select(.exit_code == 0)) | length),
              failed: ($failed | length), failed_ids: $failed},
    commands: $commands}' >"${LOG_DIR}/report.json"
rm -f "$JSONL"

{
  echo "repo-check ${VERSION} @ ${COMMIT} dirty=${DIRTY} ${KERNEL}"
  echo "日志目录: ${LOG_DIR}"
  echo "结果: 总计 $((PASS_COUNT + FAIL_COUNT))，通过 ${PASS_COUNT}，失败 ${FAIL_COUNT}"
  if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "失败命令:"
    for id in "${FAILED_IDS[@]}"; do
      echo "  - ${id}"
    done
  fi
  echo "报告: ${LOG_DIR}/report.json"
} | tee "${LOG_DIR}/report.txt"

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 1
fi
echo "repo-check 全部通过"
