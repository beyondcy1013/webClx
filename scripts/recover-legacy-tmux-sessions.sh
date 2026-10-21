#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${WEBCLX_APP_DIR:-/home/bin/webclx}"
REGISTRY="${WEBCLX_TERMINAL_REGISTRY:-$APP_DIR/.webclx-terminal-sessions.json}"
SERVICE_NAME="${WEBCLX_SERVICE_NAME:-webclx.service}"
TMUX_SOCKET_DIR="${WEBCLX_TMUX_SOCKET_DIR:-/tmp/tmux-$(id -u)}"
APPLY=false
SESSION_IDS=()
declare -A PATH_OVERRIDES=()

usage() {
  printf 'Usage: %s [--apply] --session sNNNN [--path sNNNN=/absolute/path ...]\n' "$0"
}

while (($#)); do
  case "$1" in
    --apply)
      APPLY=true
      shift
      ;;
    --session)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      SESSION_IDS+=("$2")
      shift 2
      ;;
    --path)
      [[ $# -ge 2 && "$2" == *=/* ]] || { usage >&2; exit 2; }
      path_session_id="${2%%=*}"
      PATH_OVERRIDES["$path_session_id"]="${2#*=}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

((${#SESSION_IDS[@]})) || { usage >&2; exit 2; }
[[ -f "$REGISTRY" ]] || { printf 'Registry not found: %s\n' "$REGISTRY" >&2; exit 1; }
jq -e '.sessions | type == "array"' "$REGISTRY" >/dev/null

declare -A SEEN_IDS=()
declare -A SEEN_NAMES=()
declare -A PATHS=()
declare -A NAMES=()
declare -A USERS=()
declare -A CREATED_AT=()

while IFS= read -r name; do
  [[ -n "$name" ]] && SEEN_NAMES["$name"]=1
done < <(jq -r '.sessions[].name // empty' "$REGISTRY")

for session_id in "${SESSION_IDS[@]}"; do
  [[ "$session_id" =~ ^s[0-9]+$ ]] || {
    printf 'Invalid session id: %s\n' "$session_id" >&2
    exit 1
  }
  [[ -z "${SEEN_IDS[$session_id]:-}" ]] || {
    printf 'Duplicate requested session id: %s\n' "$session_id" >&2
    exit 1
  }
  SEEN_IDS["$session_id"]=1

  if jq -e --arg id "$session_id" '.sessions[] | select(.id == $id)' "$REGISTRY" >/dev/null; then
    printf 'Already registered: %s\n' "$session_id" >&2
    exit 1
  fi

  tmux_target="webclx_$session_id"
  tmux -L default has-session -t "$tmux_target" 2>/dev/null || {
    printf 'Legacy tmux session is not alive: %s\n' "$tmux_target" >&2
    exit 1
  }

  isolated_socket="$TMUX_SOCKET_DIR/webclx_$session_id"
  if tmux -L "webclx_$session_id" has-session -t "$tmux_target" 2>/dev/null; then
    printf 'Isolated socket is already active for unregistered session: %s\n' "$session_id" >&2
    exit 1
  fi
  if [[ -e "$isolated_socket" && ! -S "$isolated_socket" ]]; then
    printf 'Refusing to replace non-socket path: %s\n' "$isolated_socket" >&2
    exit 1
  fi

  pane_pid="$(tmux -L default display-message -p -t "$tmux_target" '#{pane_pid}')"
  path="$(tmux -L default display-message -p -t "$tmux_target" '#{pane_current_path}')"
  if [[ -z "$path" ]]; then
    path="$(tmux -L default display-message -p -t "$tmux_target" '#{session_path}')"
  fi
  if [[ -n "${PATH_OVERRIDES[$session_id]:-}" ]]; then
    path="${PATH_OVERRIDES[$session_id]}"
  fi
  [[ -d "$path" ]] || {
    printf 'Session %s has no usable cwd: %s\n' "$session_id" "$path" >&2
    exit 1
  }

  terminal_name="$(tr '\0' '\n' < "/proc/$pane_pid/environ" 2>/dev/null | sed -n 's/^WEBCLX_TERMINAL_NAME=//p' | head -1)"
  user_name="$(tr '\0' '\n' < "/proc/$pane_pid/environ" 2>/dev/null | sed -n 's/^USER=//p' | head -1)"
  [[ -n "$terminal_name" ]] || terminal_name="$tmux_target"
  [[ -n "$user_name" ]] || user_name="root"

  unique_name="$terminal_name"
  if [[ -n "${SEEN_NAMES[$unique_name]:-}" ]]; then
    unique_name="${terminal_name}_${session_id}"
  fi
  SEEN_NAMES["$unique_name"]=1

  PATHS["$session_id"]="$path"
  NAMES["$session_id"]="$unique_name"
  USERS["$session_id"]="$user_name"
  CREATED_AT["$session_id"]="$(( $(tmux -L default display-message -p -t "$tmux_target" '#{session_created}') * 1000 ))"
  printf 'RECOVER|%s|%s|%s|%s\n' "$session_id" "$unique_name" "$user_name" "$path"
done

if [[ "$APPLY" != true ]]; then
  printf 'Dry run only; pass --apply to recover %s session(s).\n' "${#SESSION_IDS[@]}"
  exit 0
fi

[[ $(id -u) -eq 0 ]] || { printf '%s requires root for --apply.\n' "$0" >&2; exit 1; }

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup="$APP_DIR/terminal-recovery-backup-$timestamp.json"
tmp_registry="$(mktemp "$APP_DIR/.webclx-terminal-sessions.recover.XXXXXX")"
service_stopped=false

cleanup() {
  rm -f "$tmp_registry"
  if [[ "$service_stopped" == true ]]; then
    systemctl start "$SERVICE_NAME" || true
  fi
}
trap cleanup EXIT

systemctl stop "$SERVICE_NAME"
service_stopped=true
cp --preserve=mode,ownership,timestamps "$REGISTRY" "$backup"

cp "$REGISTRY" "$tmp_registry"
now_millis="$(( $(date +%s) * 1000 ))"
for session_id in "${SESSION_IDS[@]}"; do
  socket_path="$TMUX_SOCKET_DIR/webclx_$session_id"
  if [[ -S "$socket_path" || -L "$socket_path" ]]; then
    unlink "$socket_path"
  fi
  ln -s default "$socket_path"

  next_registry="${tmp_registry}.next"
  jq \
    --arg id "$session_id" \
    --arg path "${PATHS[$session_id]}" \
    --arg user_name "${USERS[$session_id]}" \
    --arg name "${NAMES[$session_id]}" \
    --argjson created_at "${CREATED_AT[$session_id]}" \
    --argjson last_opened_at "$now_millis" \
    '.sessions += [{
      id: $id,
      path: $path,
      user_name: $user_name,
      name: $name,
      title: "",
      codex_api_preset_name: "",
      codex_api_base_url: "",
      origin: "normal",
      owner_key: "recovered-legacy-tmux",
      manually_renamed: true,
      idle: false,
      created_at: $created_at,
      last_opened_at: $last_opened_at
    }]' "$tmp_registry" > "$next_registry"
  mv "$next_registry" "$tmp_registry"
done

jq -e --argjson expected "${#SESSION_IDS[@]}" \
  '[.sessions[] | select(.owner_key == "recovered-legacy-tmux")] | length >= $expected' \
  "$tmp_registry" >/dev/null
chmod --reference="$REGISTRY" "$tmp_registry"
chown --reference="$REGISTRY" "$tmp_registry"
mv "$tmp_registry" "$REGISTRY"

systemctl start "$SERVICE_NAME"
service_stopped=false
trap - EXIT

printf 'Recovered %s legacy tmux session(s). Backup: %s\n' "${#SESSION_IDS[@]}" "$backup"
