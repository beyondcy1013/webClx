#!/usr/bin/env bash
# Queue a webClx compile through the HTTP deploy API with an explicit no-op deploy script.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd -P)"
NOOP_DEPLOY_SCRIPT="${WEBCLX_NOOP_DEPLOY:-$REPO_ROOT/.codex/skills/webclx-compile-and-deploy/scripts/noop-deploy.sh}"
BASE_URL="http://127.0.0.1:11111"
SOURCE_TERMINAL_ID=""
SOURCE_TERMINAL_NAME=""
SOURCE_TMUX_SESSION=""
EXPLICIT_SOURCE_TERMINAL_NAME=false
PROJECT_PATH="${WEBCLX_PROJECT_PATH:-webClx}"
NOTE="Codex 请求通过 /api/build/deploy 编译 webClx，并显式提交空部署脚本。"
DEBOUNCE_SECS=""
PRINT_PAYLOAD=false

terminal_sessions_json() {
  curl -fsS --noproxy '*' "$BASE_URL/api/terminal/sessions?all=true"
}

current_tmux_terminal_id() {
  local tmux_session
  tmux_session="$(tmux display-message -p '#S' 2>/dev/null || true)"
  case "$tmux_session" in
    webclx_s[0-9]*)
      printf '%s\n' "${tmux_session#webclx_}"
      ;;
  esac
}

resolve_terminal_id_by_name() {
  local name="$1"
  local sessions="$2"
  if [ -z "$name" ] || [ -z "$sessions" ]; then
    echo ""
    return 0
  fi
  printf '%s' "$sessions" | jq -r --arg name "$name" '
    [ .sessions[]
      | select(.name == $name)
      | .id
    ] | if length == 1 then .[0] else "" end
  '
}

tmux_session_for_terminal_id() {
  local terminal_id="$1"
  if [ -n "$terminal_id" ]; then
    printf 'webclx_%s\n' "$terminal_id"
  else
    echo ""
  fi
}

refresh_current_terminal_name() {
  local sessions terminal_id terminal_name tmux_terminal_id refreshed
  sessions="$(terminal_sessions_json)" || return 1
  terminal_id="${WEBCLX_TERMINAL_ID:-}"
  terminal_name="${WEBCLX_TERMINAL_NAME:-}"
  tmux_terminal_id="$(current_tmux_terminal_id)"

  if [ -n "$terminal_id" ]; then
    refreshed="$(printf '%s' "$sessions" | jq -r --arg id "$terminal_id" '
      .sessions[]
      | select(.id == $id)
      | .name
    ' | head -n 1)"
    if [ -n "$refreshed" ]; then
      printf '%s\n' "$refreshed"
      return 0
    fi
  fi

  if [ -n "$tmux_terminal_id" ]; then
    refreshed="$(printf '%s' "$sessions" | jq -r --arg id "$tmux_terminal_id" '
      .sessions[]
      | select(.id == $id)
      | .name
    ' | head -n 1)"
    if [ -n "$refreshed" ]; then
      printf '%s\n' "$refreshed"
      return 0
    fi
  fi

  if [ -n "$terminal_name" ]; then
    refreshed="$(printf '%s' "$sessions" | jq -r --arg name "$terminal_name" '
      [ .sessions[]
        | select(.name == $name)
        | select((.connected // false) == true)
        | .name
      ] | if length == 1 then .[0] else "" end
    ')"
    if [ -n "$refreshed" ]; then
      printf '%s\n' "$refreshed"
      return 0
    fi
  fi

  echo ""
}

normalize_note_for_submit() {
  local note="$1"
  case "$note" in
    "Codex requested webClx rebuild through /api/build/compile.")
      printf '%s\n' "Codex 请求通过 /api/build/deploy 编译 webClx，并显式提交空部署脚本。"
      ;;
    "Codex requested rebuild and will continue after callback")
      printf '%s\n' "Codex 请求重建 webClx，并将在收到回调后继续原任务。"
      ;;
    "Codex requested rebuild")
      printf '%s\n' "Codex 请求重建 webClx。"
      ;;
    "Add settings tab for auto-continue scheduled tasks")
      printf '%s\n' "为自动继续定时任务添加设置页标签页。"
      ;;
    *)
      printf '%s\n' "$note"
      ;;
  esac
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

if [ "$EXPLICIT_SOURCE_TERMINAL_NAME" = false ]; then
  SOURCE_TERMINAL_NAME="$(refresh_current_terminal_name || true)"
fi

if [ -z "$SOURCE_TERMINAL_NAME" ]; then
  echo "cannot infer current webClx terminal. Pass --source-terminal-name '<terminal name>'." >&2
  exit 1
fi
sessions_json="$(terminal_sessions_json || true)"
SOURCE_TERMINAL_ID="$(resolve_terminal_id_by_name "$SOURCE_TERMINAL_NAME" "$sessions_json")"
if [ -z "$SOURCE_TERMINAL_ID" ] && [ "$EXPLICIT_SOURCE_TERMINAL_NAME" = false ]; then
  SOURCE_TERMINAL_ID="${WEBCLX_TERMINAL_ID:-}"
fi
if [ -z "$SOURCE_TERMINAL_ID" ] && [ "$EXPLICIT_SOURCE_TERMINAL_NAME" = false ]; then
  SOURCE_TERMINAL_ID="$(current_tmux_terminal_id)"
fi
SOURCE_TMUX_SESSION="$(tmux_session_for_terminal_id "$SOURCE_TERMINAL_ID")"
NOTE="$(normalize_note_for_submit "$NOTE")"

payload=$(jq -nc \
  --arg source_terminal_id "$SOURCE_TERMINAL_ID" \
  --arg source_terminal_name "$SOURCE_TERMINAL_NAME" \
  --arg source_tmux_session "$SOURCE_TMUX_SESSION" \
  --arg project_path "$PROJECT_PATH" \
  --arg note "$NOTE" \
  --arg debounce "$DEBOUNCE_SECS" \
  --arg noop_deploy_script "$NOOP_DEPLOY_SCRIPT" \
  '{
    source_terminal_name: $source_terminal_name,
    project: "webClx",
    project_name: "webClx",
    project_dir: "/home/codes/webClx",
    project_path: $project_path,
    note: $note,
    command: ["cargo", "build", "--release"],
    install_command: ["bash", $noop_deploy_script],
    audit_paths: []
  }
  + (if $source_terminal_id != "" then {source_terminal_id: $source_terminal_id} else {} end)
  + (if $source_tmux_session != "" then {source_tmux_session: $source_tmux_session} else {} end)
  + (if ($debounce | test("^[0-9]+$")) then {debounce_secs: ($debounce | tonumber)} else {} end)')

if [ "$PRINT_PAYLOAD" = true ]; then
  printf '%s\n' "$payload"
  exit 0
fi

curl -fsS --noproxy '*' \
  -X POST "$BASE_URL/api/build/deploy" \
  -H 'Content-Type: application/json' \
  -d "$payload"

echo
