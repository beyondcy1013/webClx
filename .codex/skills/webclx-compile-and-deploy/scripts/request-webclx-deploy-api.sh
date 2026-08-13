#!/usr/bin/env bash
# Queue a project compile plus install through the running webClx deploy API.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
COMPILE_WRAPPER="$SCRIPT_DIR/request-webclx-compile-api.sh"

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
NOTE="当前代理请求通过 /api/build/deploy 编译并安装当前项目。"
DEBOUNCE_SECS=""
COMMAND_JSON=""
COMMAND_ARGS=()
INSTALL_COMMAND_JSON=""
INSTALL_COMMAND_ARGS=()
AUDIT_PATHS=()
REQUIRED_ARTIFACTS=()
PRINT_PAYLOAD=false

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
    echo "webClx API unavailable at $BASE_URL; start or repair webClx before queueing this deploy." >&2
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
  local sessions="$1"
  local name="$2"
  if [ -z "$sessions" ] || [ -z "$name" ]; then
    echo ""
    return 0
  fi
  jq -r --arg name "$name" '
    [ .sessions[]
      | select(.name == $name)
      | .id
    ] | if length == 1 then .[0] else "" end
  ' <<<"$sessions"
}

terminal_name_by_id() {
  local sessions="$1"
  local terminal_id="$2"
  if [ -z "$sessions" ] || [ -z "$terminal_id" ]; then
    echo ""
    return 0
  fi
  jq -r --arg id "$terminal_id" '.sessions[] | select(.id == $id) | .name' <<<"$sessions" | head -n 1
}

terminal_name_by_unique_connected_name() {
  local sessions="$1"
  local terminal_name="$2"
  if [ -z "$sessions" ] || [ -z "$terminal_name" ]; then
    echo ""
    return 0
  fi
  jq -r --arg name "$terminal_name" '
    [ .sessions[] | select(.name == $name) | select((.connected // false) == true) | .name ]
    | if length == 1 then .[0] else "" end
  ' <<<"$sessions"
}

tmux_session_for_terminal_id() {
  local terminal_id="$1"
  if [ -n "$terminal_id" ]; then
    printf 'webclx_%s\n' "$terminal_id"
  else
    echo ""
  fi
}

refresh_current_terminal_identity() {
  local sessions terminal_id terminal_name tmux_terminal_id resolved_name resolved_id
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
      resolved_id="$(resolve_terminal_id_by_name "$sessions" "$resolved_name")"
      SOURCE_TERMINAL_ID="$resolved_id"
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

while [ "$#" -gt 0 ]; do
  case "$1" in
    --base-url)
      BASE_URL="${2:-}"
      shift 2
      ;;
    --source-terminal-name)
      SOURCE_TERMINAL_NAME="${2:-}"
      EXPLICIT_SOURCE_TERMINAL_NAME=true
      shift 2
      ;;
    --project|--project-name)
      PROJECT_NAME="${2:-}"
      shift 2
      ;;
    --project-dir)
      PROJECT_DIR="${2:-}"
      shift 2
      ;;
    --project-path)
      PROJECT_PATH="${2:-}"
      shift 2
      ;;
    --note)
      NOTE="${2:-}"
      shift 2
      ;;
    --debounce-secs)
      DEBOUNCE_SECS="${2:-}"
      shift 2
      ;;
    --command-json)
      COMMAND_JSON="${2:-}"
      shift 2
      ;;
    --command)
      COMMAND_ARGS=("bash" "-lc" "${2:-}")
      shift 2
      ;;
    --cmd)
      COMMAND_ARGS=("${2:-}")
      shift 2
      ;;
    --arg)
      COMMAND_ARGS+=("${2:-}")
      shift 2
      ;;
    --install-command-json)
      INSTALL_COMMAND_JSON="${2:-}"
      shift 2
      ;;
    --install-command)
      INSTALL_COMMAND_ARGS=("bash" "-lc" "${2:-}")
      shift 2
      ;;
    --install-cmd)
      INSTALL_COMMAND_ARGS=("${2:-}")
      shift 2
      ;;
    --install-arg)
      INSTALL_COMMAND_ARGS+=("${2:-}")
      shift 2
      ;;
    --audit-path)
      AUDIT_PATHS+=("${2:-}")
      shift 2
      ;;
    --required-artifact)
      REQUIRED_ARTIFACTS+=("${2:-}")
      shift 2
      ;;
    --print-payload|--dry-run)
      PRINT_PAYLOAD=true
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

PROJECT_DIR="$(cd "$PROJECT_DIR" && pwd -P)"
if [ -z "$PROJECT_NAME" ]; then
  PROJECT_NAME="$(basename "$PROJECT_DIR")"
fi
if [ -z "$PROJECT_PATH" ]; then
  PROJECT_PATH="$PROJECT_NAME"
fi

if [ -z "$COMMAND_JSON" ]; then
  if ! COMMAND_JSON="$(json_array_from_args "${COMMAND_ARGS[@]}")"; then
    COMMAND_JSON="$(infer_command_json "$PROJECT_DIR")"
  fi
fi

if [ -z "$INSTALL_COMMAND_JSON" ]; then
  if ! INSTALL_COMMAND_JSON="$(json_array_from_args "${INSTALL_COMMAND_ARGS[@]}")"; then
    echo "deploy requires --install-command '<shell command>' or --install-cmd/--install-arg or --install-command-json." >&2
    exit 2
  fi
fi

if ! jq -e 'type == "array" and length > 0 and all(.[]; type == "string")' <<<"$COMMAND_JSON" >/dev/null; then
  echo "--command-json must be a non-empty JSON string array" >&2
  exit 2
fi
if ! jq -e 'type == "array" and length > 0 and all(.[]; type == "string")' <<<"$INSTALL_COMMAND_JSON" >/dev/null; then
  echo "--install-command-json must be a non-empty JSON string array" >&2
  exit 2
fi
validate_shell_command_json "command" "$COMMAND_JSON"
validate_shell_command_json "install_command" "$INSTALL_COMMAND_JSON"
required_artifacts_json="$(json_array_from_args "${REQUIRED_ARTIFACTS[@]}" || jq -nc '[]')"
if ! jq -e 'type == "array" and all(.[]; type == "string" and test("[^[:space:]]"))' <<<"$required_artifacts_json" >/dev/null; then
  echo "--required-artifact must not be empty" >&2
  exit 2
fi

if [ "$PRINT_PAYLOAD" = false ]; then
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

audit_paths_json="$(json_array_from_args "${AUDIT_PATHS[@]}" || jq -nc '[]')"
payload=$(jq -nc \
  --arg source_terminal_id "$SOURCE_TERMINAL_ID" \
  --arg source_terminal_name "$SOURCE_TERMINAL_NAME" \
  --arg source_tmux_session "$SOURCE_TMUX_SESSION" \
  --arg project_path "$PROJECT_PATH" \
  --arg project "$PROJECT_NAME" \
  --arg project_dir "$PROJECT_DIR" \
  --arg note "$NOTE" \
  --arg debounce "$DEBOUNCE_SECS" \
  --argjson command "$COMMAND_JSON" \
  --argjson install_command "$INSTALL_COMMAND_JSON" \
  --argjson audit_paths "$audit_paths_json" \
  --argjson required_artifacts "$required_artifacts_json" \
  '{
    source_terminal_name: $source_terminal_name,
    project_path: $project_path,
    project: $project,
    project_name: $project,
    project_dir: $project_dir,
    note: $note,
    command: $command,
    install_command: $install_command,
    audit_paths: $audit_paths
  }
  + (if ($required_artifacts | length) > 0 then {required_artifacts: $required_artifacts} else {} end)
  + (if $source_terminal_id != "" then {source_terminal_id: $source_terminal_id} else {} end)
  + (if $source_tmux_session != "" then {source_tmux_session: $source_tmux_session} else {} end)
  + (if ($debounce | test("^[0-9]+$")) then {debounce_secs: ($debounce | tonumber)} else {} end)')

if [ "$PRINT_PAYLOAD" = true ]; then
  printf '%s\n' "$payload"
  exit 0
fi

refresh_local_auth_args
curl -fsS --noproxy '*' \
  "${LOCAL_AUTH_ARGS[@]}" \
  -X POST "$BASE_URL/api/build/deploy" \
  -H 'Content-Type: application/json' \
  -d "$payload"

echo
