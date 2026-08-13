#!/usr/bin/env bash
# Queue a pure compile through /api/build/compile, then deploy through
# /api/service/deploy after it succeeds.
set -euo pipefail

BASE_URL="http://127.0.0.1:11111"
LOCAL_TOKEN_FILE="${WEBCLX_LOCAL_TOKEN_FILE:-}"
LOCAL_AUTH_ARGS=()
SOURCE_TERMINAL_ID=""
SOURCE_TERMINAL_NAME=""
SOURCE_TMUX_SESSION=""
EXPLICIT_SOURCE_TERMINAL_NAME=false
PROJECT_DIR="$(pwd -P)"
PROJECT_NAME="$(basename "$PROJECT_DIR")"
PROJECT_PATH="${WEBCLX_PROJECT_PATH:-$PROJECT_NAME}"
NOTE=""
DEBOUNCE_SECS=""
COMMAND_JSON=""
COMMAND_ARGS=()
SERVICE_NAME=""
BINARY_PATH=""
DEPLOY_SCRIPT=""
DEPLOY_SCRIPT_FILE=""
AUDIT_PATHS=()
REQUIRED_ARTIFACTS=()
PRINT_PAYLOAD=false
DRY_RUN=false
SKIP_COMPILE=false

refresh_local_auth_args() {
  local token host
  LOCAL_AUTH_ARGS=()
  host="${BASE_URL#*://}"
  host="${host%%[:/]*}"
  case "$host" in 127.0.0.1|localhost|\[::1\]) ;; *) return 0 ;; esac
  [ -n "$LOCAL_TOKEN_FILE" ] && [ -r "$LOCAL_TOKEN_FILE" ] || return 0
  token="$(tr -d '\r\n' <"$LOCAL_TOKEN_FILE")"
  [[ "$token" =~ ^[0-9A-Fa-f]{64}$ ]] || return 0
  LOCAL_AUTH_ARGS=(-H "X-WebClx-Local-Token: $token")
}

# ============================================================================
# Built-in service registry — maps known project directories to their service
# name, target binary path, and build command. When --service-name or
# --binary-path are omitted but the current project directory matches a known
# entry, the script fills in the defaults automatically.
#
# Format: PROJECT_DIR|SERVICE_NAME|BINARY_PATH|BUILD_CMD
# BUILD_CMD may be empty to let the script infer from Cargo.toml / package.json.
# ============================================================================
KNOWN_SERVICES=(
  "/home/codes/webClx|webclx.service|/home/bin/webclx/webClx|cargo build --release"
  "/home/codes/signIn|signin.service|/home/bin/signIn/signIn|cargo build --release"
  "/home/codes/feishuFwd|feishu-fwd-web.service|/home/bin/feishuFwd/feishu_fwd_web|cargo build --release"
  "/home/codes/quoteGateway|quoteGateway.service|/home/bin/quoteGateway/quoteGateway|cargo build --release"
  "/home/codes/stockJiepan|stockJiepan.service|/home/bin/stockJiepan/stock-jiepan|cargo build --release"
  "/home/codes/stockScreener|stockScreener.service|/home/bin/stockScreener/stockScreener|cargo build --release"
  "/home/codes/stockInfo|stockInfo.service|/home/bin/stockInfo/stock-info|cargo build --release"
  "/home/codes/stockF10|stockF10-web.service|/home/bin/stockF10/stockF10|cargo build --release"
  "/home/codes/stockAgent|stock-agent.service|/usr/local/bin/stock-agent|cargo build --release"
  "/home/codes/newsKB|newsKB-web-rs-frontdoor.service|/home/bin/newsKB-web-rs/news-kb-web|"
  "/home/codes/systemGuard|systemGuard.service|/home/bin/systemGuard/systemGuard|cargo build --release"
  "/home/third_party/sub2api|sub2api.service|/home/third_party/bin/sub2api/sub2api|make"
  "/home/third_party/sub2freeApi|sub2freeApi.service|/home/third_party/bin/sub2freeApi/sub2freeApi|make"
)

# Look up a known service entry for the given project directory.
# Sets SERVICE_NAME, BINARY_PATH, and KNOWN_BUILD_CMD if found.
lookup_known_service() {
  local dir="$1"
  for entry in "${KNOWN_SERVICES[@]}"; do
    IFS='|' read -r known_dir known_svc known_bin known_build <<<"$entry"
    if [ "$dir" = "$known_dir" ]; then
      SERVICE_NAME="${SERVICE_NAME:-$known_svc}"
      BINARY_PATH="${BINARY_PATH:-$known_bin}"
      KNOWN_BUILD_CMD="$known_build"
      return 0
    fi
  done
  return 1
}

json_array_from_args() {
  if [ "$#" -eq 0 ]; then
    return 1
  fi
  printf '%s\0' "$@" | jq -Rs 'split("\u0000")[:-1]'
}

validate_shell_command_json() {
  local field="$1" command_json="$2"
  if ! jq -e '
    def shell_name: split("/") | last;
    def trimmed: gsub("^[[:space:]]+|[[:space:]]+$"; "");
    . as $argv
    | (($argv[0] // "") | shell_name) as $program
    | if (["bash", "sh", "dash", "zsh", "ksh"] | index($program)) == null then true
      elif (($argv[1] // "") | test("^-[^-]*c")) then
        if ($argv | length) < 3 or (($argv[2] | type) != "string") or (($argv[2] | trimmed) == "") then false
        elif (($argv | length) > 3) and
             ((["bash", "sh", "dash", "zsh", "ksh"] | index(($argv[2] | trimmed | shell_name))) != null)
        then false
        else true
        end
      else true
      end
  ' <<<"$command_json" >/dev/null; then
    echo "$field shell -c/-lc expects a single command string; do not split the command and script into separate argv entries." >&2
    return 1
  fi
}

infer_command_json() {
  local dir="$1"
  if [ -f "$dir/Cargo.toml" ]; then
    jq -nc '["cargo","build","--release"]'
  elif [ -f "$dir/package.json" ]; then
    jq -nc '["npm","run","build"]'
  elif [ -f "$dir/Makefile" ] || [ -f "$dir/makefile" ]; then
    jq -nc '["make"]'
  else
    echo "cannot infer compile command for $dir. Pass --command '<shell command>' or --cmd/--arg." >&2
    exit 2
  fi
}

terminal_sessions_json() {
  refresh_local_auth_args
  curl -fsS --noproxy '*' "${LOCAL_AUTH_ARGS[@]}" "$BASE_URL/api/terminal/sessions?all=true"
}

require_api_available() {
  if ! terminal_sessions_json >/dev/null 2>&1; then
    echo "webClx API unavailable at $BASE_URL; start or repair webClx before queueing this compile/deploy workflow." >&2
    exit 1
  fi
}

current_tmux_session_name() {
  tmux display-message -p '#S' 2>/dev/null || true
}

current_tmux_terminal_id() {
  local tmux_session
  tmux_session="$(current_tmux_session_name)"
  case "$tmux_session" in
    webclx_s[0-9]*)
      printf '%s\n' "${tmux_session#webclx_}"
      ;;
  esac
}

resolve_terminal_id_by_name() {
  local sessions="$1" name="$2"
  if [ -z "$sessions" ] || [ -z "$name" ]; then echo ""; return 0; fi
  jq -r --arg name "$name" '
    [ .sessions[] | select(.name == $name) | .id ]
    | if length == 1 then .[0] else "" end
  ' <<<"$sessions"
}

terminal_name_by_id() {
  local sessions="$1" terminal_id="$2"
  if [ -z "$sessions" ] || [ -z "$terminal_id" ]; then echo ""; return 0; fi
  jq -r --arg id "$terminal_id" '.sessions[] | select(.id == $id) | .name' <<<"$sessions" | head -n 1
}

terminal_name_by_unique_connected_name() {
  local sessions="$1" terminal_name="$2"
  if [ -z "$sessions" ] || [ -z "$terminal_name" ]; then echo ""; return 0; fi
  jq -r --arg name "$terminal_name" '
    [ .sessions[] | select(.name == $name) | select((.connected // false) == true) | .name ]
    | if length == 1 then .[0] else "" end
  ' <<<"$sessions"
}

tmux_session_for_terminal_id() {
  local terminal_id="$1"
  if [ -n "$terminal_id" ]; then printf 'webclx_%s\n' "$terminal_id"; else echo ""; fi
}

refresh_current_terminal_identity() {
  local sessions terminal_id terminal_name tmux_terminal_id resolved_name
  sessions="$(terminal_sessions_json)" || return 1
  terminal_id="${WEBCLX_TERMINAL_ID:-}"
  terminal_name="${WEBCLX_TERMINAL_NAME:-}"
  tmux_terminal_id="$(current_tmux_terminal_id)"

  if [ -n "$terminal_id" ]; then
    resolved_name="$(terminal_name_by_id "$sessions" "$terminal_id")"
    if [ -n "$resolved_name" ]; then
      SOURCE_TERMINAL_ID="$terminal_id"
      SOURCE_TERMINAL_NAME="$resolved_name"
      SOURCE_TMUX_SESSION="$(tmux_session_for_terminal_id "$SOURCE_TERMINAL_ID")"
      return 0
    fi
  fi

  if [ -n "$tmux_terminal_id" ]; then
    resolved_name="$(terminal_name_by_id "$sessions" "$tmux_terminal_id")"
    if [ -n "$resolved_name" ]; then
      SOURCE_TERMINAL_ID="$tmux_terminal_id"
      SOURCE_TERMINAL_NAME="$resolved_name"
      SOURCE_TMUX_SESSION="$(tmux_session_for_terminal_id "$SOURCE_TERMINAL_ID")"
      return 0
    fi
  fi

  if [ -n "$terminal_name" ]; then
    resolved_name="$(terminal_name_by_unique_connected_name "$sessions" "$terminal_name")"
    if [ -n "$resolved_name" ]; then
      SOURCE_TERMINAL_ID="$(resolve_terminal_id_by_name "$sessions" "$resolved_name")"
      SOURCE_TERMINAL_NAME="$resolved_name"
      SOURCE_TMUX_SESSION="$(tmux_session_for_terminal_id "$SOURCE_TERMINAL_ID")"
      return 0
    fi
  fi

  return 1
}

resolve_explicit_source_terminal_identity() {
  local sessions resolved_id
  sessions="$(terminal_sessions_json)" || return 0
  resolved_id="$(resolve_terminal_id_by_name "$sessions" "$SOURCE_TERMINAL_NAME")"
  if [ -n "$resolved_id" ]; then
    SOURCE_TERMINAL_ID="$resolved_id"
    SOURCE_TMUX_SESSION="$(tmux_session_for_terminal_id "$SOURCE_TERMINAL_ID")"
  fi
}

usage() {
  cat <<'USAGE'
Usage: request-webclx-compile-and-deploy.sh [options]

Two-stage compile + deploy through the webClx API:
  1) /api/build/compile (compile only)
  2) /api/service/deploy (install binary + restart service)

Compile options:
  --project NAME             Project name (default: current directory basename)
  --project-dir DIR          Working directory for compile (default: pwd)
  --project-path LABEL       Workspace label for callbacks (default: project name)
  --command 'SHELL CMD'      Compile command as a shell string
  --cmd BIN --arg ARG ...    Compile command as argv
  --command-json JSON        Compile command as JSON string array
  --debounce-secs N          Debounce window before compile (default: 0)
  --note MESSAGE             Note for compile request

Deploy options:
  --service-name NAME        systemd service to restart (auto-detected for known projects)
  --binary-path PATH         Target binary to replace (auto-detected for known projects)
  --deploy-script CONTENT    Inline deploy script content
  --deploy-script-file PATH  Read deploy script from file
  --audit-path PATH          Extra file to audit (repeatable)
  --required-artifact PATH   Build output that must exist before deploy (repeatable)

Source terminal:
  --source-terminal-name N   Explicit webClx terminal name

Other:
  --skip-compile             Skip compile, only run deploy step
  --base-url URL             API base URL (default: http://127.0.0.1:11111)
  --dry-run                  Print compile + deploy payloads, do not execute
USAGE
  exit 2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --help|-h) usage ;;
    --base-url) BASE_URL="${2:-}"; shift 2 ;;
    --source-terminal-name) SOURCE_TERMINAL_NAME="${2:-}"; EXPLICIT_SOURCE_TERMINAL_NAME=true; shift 2 ;;
    --project|--project-name) PROJECT_NAME="${2:-}"; shift 2 ;;
    --project-dir) PROJECT_DIR="${2:-}"; shift 2 ;;
    --project-path) PROJECT_PATH="${2:-}"; shift 2 ;;
    --note) NOTE="${2:-}"; shift 2 ;;
    --debounce-secs) DEBOUNCE_SECS="${2:-}"; shift 2 ;;
    --command-json) COMMAND_JSON="${2:-}"; shift 2 ;;
    --command) COMMAND_ARGS=("bash" "-lc" "${2:-}"); shift 2 ;;
    --cmd) COMMAND_ARGS=("${2:-}"); shift 2 ;;
    --arg) COMMAND_ARGS+=("${2:-}"); shift 2 ;;
    --service-name) SERVICE_NAME="${2:-}"; shift 2 ;;
    --binary-path) BINARY_PATH="${2:-}"; shift 2 ;;
    --deploy-script) DEPLOY_SCRIPT="${2:-}"; shift 2 ;;
    --deploy-script-file) DEPLOY_SCRIPT_FILE="${2:-}"; shift 2 ;;
    --audit-path) AUDIT_PATHS+=("${2:-}"); shift 2 ;;
    --required-artifact) REQUIRED_ARTIFACTS+=("${2:-}"); shift 2 ;;
    --skip-compile) SKIP_COMPILE=true; shift ;;
    --print-payload|--dry-run) DRY_RUN=true; PRINT_PAYLOAD=true; shift ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
done

# ---- validate ----
if [ "$SKIP_COMPILE" = false ]; then
  PROJECT_DIR="$(cd "$PROJECT_DIR" && pwd -P)"
  if [ -z "$PROJECT_NAME" ]; then PROJECT_NAME="$(basename "$PROJECT_DIR")"; fi
  if [ -z "$PROJECT_PATH" ]; then PROJECT_PATH="$PROJECT_NAME"; fi

  # Resolve known service defaults before validating
  lookup_known_service "$PROJECT_DIR" || true

  if [ -z "$COMMAND_JSON" ]; then
    if ! COMMAND_JSON="$(json_array_from_args "${COMMAND_ARGS[@]}")"; then
      if [ -n "${KNOWN_BUILD_CMD:-}" ]; then
        # Use the known build command from registry — split by jq
        COMMAND_JSON="$(printf '%s' "$KNOWN_BUILD_CMD" | jq -R 'split(" ")')" || true
      fi
      if [ -z "$COMMAND_JSON" ]; then
        COMMAND_JSON="$(infer_command_json "$PROJECT_DIR")"
      fi
    fi
  fi

  if ! jq -e 'type == "array" and length > 0 and all(.[]; type == "string")' <<<"$COMMAND_JSON" >/dev/null; then
    echo "--command-json must be a non-empty JSON string array" >&2
    exit 2
  fi
  validate_shell_command_json "command" "$COMMAND_JSON"
  required_artifacts_json="$(json_array_from_args "${REQUIRED_ARTIFACTS[@]}" || jq -nc '[]')"
  if ! jq -e 'type == "array" and all(.[]; type == "string" and test("[^[:space:]]"))' <<<"$required_artifacts_json" >/dev/null; then
    echo "--required-artifact must not be empty" >&2
    exit 2
  fi
fi

if [ -z "$SERVICE_NAME" ] || [ -z "$BINARY_PATH" ]; then
  lookup_known_service "$PROJECT_DIR" || true
fi

if [ -z "$SERVICE_NAME" ]; then
  echo "--service-name is required (systemd service to restart after deploy)" >&2
  echo "Known projects: webclx, signIn, feishuFwd, quote-gateway, stockJiepan, stockScreener, stockInfo, stockF10, stockAgent, newsKB, systemGuard, sub2api, sub2freeApi" >&2
  exit 2
fi
if [ -z "$BINARY_PATH" ]; then
  echo "--binary-path is required (target binary to replace)" >&2
  exit 2
fi

# Resolve deploy script
if [ -z "$DEPLOY_SCRIPT" ] && [ -n "$DEPLOY_SCRIPT_FILE" ]; then
  DEPLOY_SCRIPT="$(cat "$DEPLOY_SCRIPT_FILE")"
fi
if [ -z "$DEPLOY_SCRIPT" ]; then
  # Auto-generate deploy script from the same Cargo target directory used by
  # direct builds and by the compile worker. A few service repos (newsKB) keep
  # their Cargo workspace under rust/ while the API project_dir is the parent.
  cargo_project_dir=""
  if [ -f "$PROJECT_DIR/Cargo.toml" ]; then
    cargo_project_dir="$PROJECT_DIR"
  elif [ -f "$PROJECT_DIR/rust/Cargo.toml" ]; then
    cargo_project_dir="$PROJECT_DIR/rust"
  fi
  cargo_target_dir="$PROJECT_DIR/target"
  cargo_metadata=""
  if [ -n "$cargo_project_dir" ]; then
    cargo_metadata="$(cd "$cargo_project_dir" && cargo metadata --format-version 1 --no-deps)"
    cargo_target_dir="$(jq -r '.target_directory' <<<"$cargo_metadata")"
  fi

  # Prefer a Cargo bin target name, then the deployed binary's basename.
  binary_name=""
  if [ -n "$cargo_metadata" ]; then
    binary_name="$(jq -r '[.packages[].targets[] | select((.kind // []) | index("bin")) | .name][0] // empty' <<<"$cargo_metadata")"
  fi
  if [ -z "$binary_name" ]; then
    binary_name="$(basename "$BINARY_PATH")"
  fi
  deployed_binary_name="$(basename "$BINARY_PATH")"

  DEPLOY_SCRIPT="#!/bin/bash
set -euo pipefail
# 更新二进制前保留一个备份 (固定名 .bak, 每次覆盖, 始终只留一个)
if [ -f \"$BINARY_PATH\" ]; then
  mv -f \"$BINARY_PATH\" \"$BINARY_PATH.bak\"
fi
BUILT=\"\"
for candidate in \
  \"$cargo_target_dir/release/$binary_name\" \
  \"$cargo_target_dir/release/$deployed_binary_name\"; do
  if [ -f \"\$candidate\" ]; then
    BUILT=\"\$candidate\"
    break
  fi
done
if [ -z \"\$BUILT\" ]; then
  BUILT=\"\$(find \"$cargo_target_dir\" -mindepth 3 -maxdepth 3 -type f -path '*/release/*' \\( -name \"$binary_name\" -o -name \"$deployed_binary_name\" \\) -printf '%T@ %p\\n' 2>/dev/null | sort -nr | head -n 1 | cut -d' ' -f2-)\"
fi
if [ -z \"\$BUILT\" ] || [ ! -f \"\$BUILT\" ]; then
  echo \"ERROR: built binary not found below $cargo_target_dir for $binary_name or $deployed_binary_name\" >&2
  exit 1
fi
install -m 0755 \"\$BUILT\" \"$BINARY_PATH\""
fi

# ---- resolve terminal identity ----
if [ "$DRY_RUN" = false ]; then
  require_api_available
fi

if [ "$EXPLICIT_SOURCE_TERMINAL_NAME" = false ]; then
  refresh_current_terminal_identity || true
else
  resolve_explicit_source_terminal_identity || true
fi

if [ -z "$SOURCE_TERMINAL_NAME" ]; then
  refresh_current_terminal_identity || true
fi

if [ -z "$SOURCE_TERMINAL_NAME" ]; then
  echo "cannot refresh current webClx source terminal name. Pass --source-terminal-name '<terminal name>'." >&2
  exit 1
fi

# ---- step 1: compile (unless skipped) ----
if [ "$SKIP_COMPILE" = false ]; then
  compile_note="${NOTE:-当前代理请求编译当前项目，编译完成后自动部署。}"

  compile_payload=$(jq -nc \
    --arg source_terminal_id "$SOURCE_TERMINAL_ID" \
    --arg source_terminal_name "$SOURCE_TERMINAL_NAME" \
    --arg source_tmux_session "$SOURCE_TMUX_SESSION" \
    --arg project_path "$PROJECT_PATH" \
    --arg project "$PROJECT_NAME" \
    --arg project_dir "$PROJECT_DIR" \
    --arg note "$compile_note" \
    --arg debounce "$DEBOUNCE_SECS" \
    --argjson command "$COMMAND_JSON" \
    --argjson required_artifacts "$required_artifacts_json" \
    '{
      source_terminal_name: $source_terminal_name,
      project_path: $project_path,
      project: $project,
      project_name: $project,
      project_dir: $project_dir,
      note: $note,
      command: $command
    }
    + (if ($required_artifacts | length) > 0 then {required_artifacts: $required_artifacts} else {} end)
    + (if $source_terminal_id != "" then {source_terminal_id: $source_terminal_id} else {} end)
    + (if $source_tmux_session != "" then {source_tmux_session: $source_tmux_session} else {} end)
    + (if ($debounce | test("^[0-9]+$")) then {debounce_secs: ($debounce | tonumber)} else {} end)')

  if [ "$DRY_RUN" = true ]; then
    echo "=== STEP 1: COMPILE PAYLOAD ==="
    printf '%s\n\n' "$compile_payload"
  else
    echo ">>> 提交编译请求..." >&2
    refresh_local_auth_args
    compile_response=$(curl -fsS --noproxy '*' \
      "${LOCAL_AUTH_ARGS[@]}" \
      -X POST "$BASE_URL/api/build/compile" \
      -H 'Content-Type: application/json' \
      -d "$compile_payload")
    compile_ok=$(jq -r '.ok // false' <<<"$compile_response")
    compile_id=$(jq -r '.request_id // "?"' <<<"$compile_response")
    if [ "$compile_ok" != "true" ]; then
      echo "编译请求失败: $compile_response" >&2
      exit 1
    fi
    echo "编译已入队 (request_id=$compile_id)，等待编译完成回调..." >&2
    echo "$compile_response"
  fi
fi

# ---- step 2: deploy ----
deploy_payload=$(jq -nc \
  --arg service_name "$SERVICE_NAME" \
  --arg script "$DEPLOY_SCRIPT" \
  --arg binary_path "$BINARY_PATH" \
  --arg source_terminal_name "$SOURCE_TERMINAL_NAME" \
  --arg source_terminal_id "$SOURCE_TERMINAL_ID" \
  '{
    service_name: $service_name,
    script: $script,
    binary_path: $binary_path
  }
  + (if $source_terminal_name != "" then {source_terminal_name: $source_terminal_name} else {} end)
  + (if $source_terminal_id != "" then {source_terminal_id: $source_terminal_id} else {} end)')

if [ "$DRY_RUN" = true ]; then
  echo "=== STEP 2: DEPLOY PAYLOAD ==="
  printf '%s\n\n' "$deploy_payload"
  echo ">>> When compile completes, run the deploy step manually or re-run with --skip-compile"
else
  echo ""
  echo "=== 编译完成后请执行部署步骤 ==="
  echo ""
  echo "当编译成功回调到达后，用以下命令执行部署："
  echo ""
  refresh_local_auth_args
  echo "  curl -fsS --noproxy '*' -X POST '$BASE_URL/api/service/deploy' \\"
  if [ "${#LOCAL_AUTH_ARGS[@]}" -gt 0 ]; then
    echo "    -H 'X-WebClx-Local-Token: <read from WEBCLX_LOCAL_TOKEN_FILE>' \\"
  fi
  echo "    -H 'Content-Type: application/json' \\"
  printf "    -d '%s'\n" "$deploy_payload"
  echo ""
  echo "或重新运行本脚本并加 --skip-compile："
  echo ""
  echo "  bash $0 --skip-compile --service-name '$SERVICE_NAME' --binary-path '$BINARY_PATH' --source-terminal-name '$SOURCE_TERMINAL_NAME'"
  echo ""
fi
