#!/usr/bin/env bash
# Debounced concurrent build worker for POST /api/build/compile and /api/build/deploy.
set -euo pipefail

# The worker is launched via systemd-run, which inherits a near-empty environment
# (PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin, HOME=). Without the
# rustup-managed cargo on PATH, builds silently fall back to /usr/bin/cargo (apt
# package, no toolchain targets) and fail with E0463 "can't find crate for core"
# on any --target build. Restore HOME and prepend ~/.cargo/bin so the rustup
# toolchain (with windows/android targets) is the cargo that actually runs.
if [ -z "${HOME:-}" ] || [ "$HOME" = "/" ]; then
  case "$(id -u)" in
    0) _worker_home="$(getent passwd 0 | cut -d: -f6)" ;;
    *) _worker_home="$(getent passwd "$(id -u)" | cut -d: -f6)" ;;
  esac
  if [ -n "${_worker_home:-}" ] && [ -d "$_worker_home" ]; then
    export HOME="$_worker_home"
  fi
  unset _worker_home
fi
if [ -n "${HOME:-}" ] && [ -d "$HOME/.cargo/bin" ]; then
  case ":${PATH:-}:" in
    *":$HOME/.cargo/bin":*) ;;
    *) export PATH="$HOME/.cargo/bin:${PATH:-}" ;;
  esac
fi
if [ -n "${HOME:-}" ] && [ -d "$HOME/.rustup" ] && [ -z "${RUSTUP_HOME:-}" ]; then
  export RUSTUP_HOME="$HOME/.rustup"
fi
if [ -n "${HOME:-}" ] && [ -d "$HOME/.cargo" ] && [ -z "${CARGO_HOME:-}" ]; then
  export CARGO_HOME="$HOME/.cargo"
fi

QUEUE_DIR=""
WORK_DIR=""
BASE_URL="http://127.0.0.1:11111"
REPO_DIR="/home/codes/webClx"
QUIET_SECS=0
COMMAND_TIMEOUT_SECS="${WEBCLX_COMMAND_TIMEOUT_SECS:-600}"
MAX_CONCURRENCY="${WEBCLX_COMPILE_MAX_CONCURRENCY:-5}"
CALLBACK_RETRY_COUNT="${WEBCLX_CALLBACK_RETRY_COUNT:-300}"
CALLBACK_RETRY_MAX_TIME="${WEBCLX_CALLBACK_RETRY_MAX_TIME:-300}"

local_token_file() {
  if [ -n "${WEBCLX_LOCAL_TOKEN_FILE:-}" ]; then
    printf '%s\n' "$WEBCLX_LOCAL_TOKEN_FILE"
  else
    printf '%s/.webclx-local-api-token\n' "$(dirname "$QUEUE_DIR")"
  fi
}

refresh_local_auth_args() {
  local file token
  LOCAL_AUTH_ARGS=()
  file="$(local_token_file)"
  if [ -r "$file" ]; then
    token="$(tr -d '\r\n' <"$file")"
    if [[ "$token" =~ ^[0-9A-Fa-f]{64}$ ]]; then
      LOCAL_AUTH_ARGS=(-H "X-WebClx-Local-Token: $token")
    fi
  fi
}

LOCAL_AUTH_ARGS=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --queue-dir)
      QUEUE_DIR="${2:-}"
      shift 2
      ;;
    --base-url)
      BASE_URL="${2:-}"
      shift 2
      ;;
    --repo-dir)
      REPO_DIR="${2:-}"
      shift 2
      ;;
    --work-dir)
      WORK_DIR="${2:-}"
      shift 2
      ;;
    --quiet-secs)
      QUIET_SECS="${2:-0}"
      shift 2
      ;;
    --command-timeout)
      COMMAND_TIMEOUT_SECS="${2:-600}"
      shift 2
      ;;
    --max-concurrency)
      MAX_CONCURRENCY="${2:-5}"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [ -z "$QUEUE_DIR" ]; then
  QUEUE_DIR="$(pwd)/compile"
fi

# The heavy build caches (per-project CARGO_TARGET_DIR trees, temp dirs) can
# grow to hundreds of GB. By default they live under QUEUE_DIR/work on the same
# filesystem as the source repo; set --work-dir (or WEBCLX_COMPILE_WORK_DIR) to
# relocate them onto a larger data partition while keeping the lightweight
# queue metadata (requests/runs) next to the webClx app dir.
if [ -z "$WORK_DIR" ]; then
  WORK_DIR="${WEBCLX_COMPILE_WORK_DIR:-$QUEUE_DIR/work}"
fi

REQUESTS_DIR="$QUEUE_DIR/requests"
RUNS_DIR="$QUEUE_DIR/runs"
LOCK_FILE="$QUEUE_DIR/worker.lock"
CLAIM_LOCK_FILE="$QUEUE_DIR/claim.lock"
RESOURCE_LOCKS_DIR="$QUEUE_DIR/resource-locks"
SLOT_LOCKS_DIR="$QUEUE_DIR/concurrency-slots"
WORK_TMP_DIR="$WORK_DIR/tmp"
WORK_CARGO_TARGET_DIR="$WORK_DIR/cargo-target"
DEFAULT_COMMAND_JSON='["bash","docs/codex/skills/webclx-rebuild/scripts/rebuild-and-deploy.sh"]'

# Directories whose recursive sha256 provides no deploy-audit value but can take
# hours (e.g. a 4.2GB cargo target/ tree). The directory snapshot prunes these
# via find -prune in path_snapshot_json (kept explicit there for readability).
SNAPSHOT_PRUNE_NAMES=('.git' 'target' 'node_modules' 'dist' 'build' 'out' '.next' '__pycache__' 'venv' '.venv' '.cache')
# Hard cap on how many regular files a single directory snapshot will hash.
# Beyond this the snapshot records aggregate metadata instead of a content hash.
SNAPSHOT_MAX_FILES=2000

is_safe_snapshot_path() {
  local path="$1"
  case "$path" in
    ""|/|//*|/dev|/dev/*|/proc|/proc/*|/sys|/sys/*) return 1 ;;
  esac
  case "$path" in
    *..*) return 1 ;;
  esac
  if [ ! -e "$path" ]; then
    return 1
  fi
  local canonical_path
  canonical_path="$(realpath -e -- "$path" 2>/dev/null)" || return 1
  case "$canonical_path" in
    /|/dev|/dev/*|/proc|/proc/*|/sys|/sys/*) return 1 ;;
  esac
  return 0
}

# Human-readable timeout label, e.g. 600 -> "10 分", 3600 -> "60 分", 90 -> "90 秒".
timeout_label() {
  local secs="$1"
  if [ "$secs" -ge 60 ] && [ $((secs % 60)) -eq 0 ]; then
    printf '%s 分' "$((secs / 60))"
  elif [ "$secs" -ge 60 ]; then
    printf '%s 分 %s 秒' "$((secs / 60))" "$((secs % 60))"
  else
    printf '%s 秒' "$secs"
  fi
}

# Run a command under a hard timeout. On timeout it writes a marker file so the
# notifier can report a SEVERE timeout instead of an ordinary failure, and the
# worker can never hang forever holding the single-flight flock. The marker path
# is echoed on stdout for the caller to use. Exit status propagates the command's
# (124 from coreutils `timeout` when killed).
run_command_with_timeout() {
  local marker="$1"
  shift
  timeout --signal=TERM --kill-after=15s "$COMMAND_TIMEOUT_SECS" "$@"
  local rc=$?
  if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
    printf '%s\n' "$(timeout_label "$COMMAND_TIMEOUT_SECS")" >"$marker"
  fi
  return "$rc"
}

if [[ ! "$MAX_CONCURRENCY" =~ ^[0-9]+$ ]] || [ "$MAX_CONCURRENCY" -lt 1 ]; then
  MAX_CONCURRENCY=1
elif [ "$MAX_CONCURRENCY" -gt 32 ]; then
  MAX_CONCURRENCY=32
fi
if [[ ! "$CALLBACK_RETRY_COUNT" =~ ^[0-9]+$ ]] || [ "$CALLBACK_RETRY_COUNT" -lt 1 ]; then
  CALLBACK_RETRY_COUNT=300
fi
if [[ ! "$CALLBACK_RETRY_MAX_TIME" =~ ^[0-9]+$ ]] || [ "$CALLBACK_RETRY_MAX_TIME" -lt 1 ]; then
  CALLBACK_RETRY_MAX_TIME=300
fi

mkdir -p "$REQUESTS_DIR" "$RUNS_DIR" "$WORK_TMP_DIR" "$WORK_CARGO_TARGET_DIR" "$RESOURCE_LOCKS_DIR" "$SLOT_LOCKS_DIR"
DEPLOY_TARGET_LOCK_FDS=()
declare -A SPEC_ENV_ORIGINAL_PRESENT=()
declare -A SPEC_ENV_ORIGINAL_VALUES=()
declare -A ACTIVE_SPEC_ENV_KEYS=()
if [ -z "${TMPDIR:-}" ] || [ "$TMPDIR" = "/tmp" ]; then
  export TMPDIR="$WORK_TMP_DIR/worker"
fi
mkdir -p "$TMPDIR"

exec 9>"$LOCK_FILE"
# Builds share the maintenance lock. Cargo-cache migration takes it exclusively,
# so it still waits for every active build and prevents new ones from starting.
flock -s 9

now_secs() {
  date +%s
}

set_spec_environment_value() {
  local key="$1"
  local value="$2"
  if [[ ! ${SPEC_ENV_ORIGINAL_PRESENT[$key]+_} ]]; then
    if [[ -v $key ]]; then
      SPEC_ENV_ORIGINAL_PRESENT["$key"]=1
      SPEC_ENV_ORIGINAL_VALUES["$key"]="${!key}"
    else
      SPEC_ENV_ORIGINAL_PRESENT["$key"]=0
      SPEC_ENV_ORIGINAL_VALUES["$key"]=""
    fi
  fi
  export "$key=$value"
  ACTIVE_SPEC_ENV_KEYS["$key"]=1
}

restore_spec_environment() {
  local key
  for key in "${!ACTIVE_SPEC_ENV_KEYS[@]}"; do
    if [ "${SPEC_ENV_ORIGINAL_PRESENT[$key]}" = "1" ]; then
      export "$key=${SPEC_ENV_ORIGINAL_VALUES[$key]}"
    else
      unset "$key"
    fi
  done
  ACTIVE_SPEC_ENV_KEYS=()
}

display_run_id() {
  local value="$1"
  case "$value" in
    [0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]T[0-9][0-9][0-9][0-9][0-9][0-9]-*)
      printf '%s\n' "${value#????????T}"
      ;;
    *)
      printf '%s\n' "$value"
      ;;
  esac
}

display_clock_time() {
  local value="$1"
  case "$value" in
    [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]\ [0-9][0-9]:[0-9][0-9]:[0-9][0-9])
      printf '%s\n' "${value#????-??-?? }"
      ;;
    *)
      printf '%s\n' "$value"
      ;;
  esac
}

json_get() {
  local file="$1"
  local expr="$2"
  jq -r "$expr // \"\"" "$file" 2>/dev/null || true
}

safe_path_slug() {
  local value="$1"
  value="$(printf '%s' "$value" | tr -c '[:alnum:]_.-' '_' | sed -E 's/^_+//; s/_+$//')"
  if [ -z "$value" ]; then
    value="project"
  fi
  printf '%s\n' "$value"
}

default_cargo_target_dir() {
  local project="$1"
  local project_dir="$2"
  local slug hash
  slug="$(safe_path_slug "$project")"
  hash="$(printf '%s' "$project_dir" | sha256sum | awk '{print $1}' | cut -c1-16)"
  printf '%s/%s-%s\n' "$WORK_CARGO_TARGET_DIR" "$slug" "$hash"
}

cargo_project_dir() {
  local project_dir="$1"
  if [ -f "$project_dir/Cargo.toml" ]; then
    printf '%s\n' "$project_dir"
  elif [ -f "$project_dir/rust/Cargo.toml" ]; then
    printf '%s\n' "$project_dir/rust"
  fi
}

is_linked_git_worktree() {
  local project_dir="$1"
  local cargo_dir git_root main_root
  cargo_dir="$(cargo_project_dir "$project_dir")"
  [ -n "$cargo_dir" ] || return 1
  git_root="$(git -C "$cargo_dir" rev-parse --show-toplevel 2>/dev/null || true)"
  [ -n "$git_root" ] || return 1
  main_root="$(git -C "$cargo_dir" worktree list --porcelain 2>/dev/null | awk '/^worktree / {sub(/^worktree /, ""); print; exit}')"
  [ -n "$main_root" ] || return 1
  [ "$(realpath -m "$git_root")" != "$(realpath -m "$main_root")" ]
}

default_sccache_server_uds() {
  local cache_dir="$1"
  local cache_size="$2"
  local key preferred
  key="$(printf '%s\n%s\n' "$cache_dir" "$cache_size" | sha256sum | awk '{print substr($1, 1, 16)}')"
  preferred="$WORK_TMP_DIR/s-$key.sock"
  if [ "$(printf '%s' "$preferred" | wc -c)" -lt 100 ]; then
    printf '%s\n' "$preferred"
  else
    printf '/tmp/webclx-s-%s.sock\n' "$key"
  fi
}

configure_cargo_build_environment() {
  local project="$1"
  local project_dir="$2"
  local fallback_target_dir="$3"
  [ -n "$(cargo_project_dir "$project_dir")" ] || return 0

  # Cargo fingerprints contain absolute source paths. Sharing one writable
  # target tree across linked worktrees can therefore reuse an rmeta built
  # from another branch. Give every linked worktree a stable private target;
  # sccache still reuses identical compiler outputs across those directories.
  if ! [[ -v CARGO_TARGET_DIR ]] && is_linked_git_worktree "$project_dir"; then
    local cargo_dir target_entry lock_key
    cargo_dir="$(cargo_project_dir "$project_dir")"
    target_entry="$cargo_dir/target"
    if [ -L "$target_entry" ] \
       || [ ! -e "$target_entry" ] \
       || { [ -d "$target_entry" ] && [ -z "$(find "$target_entry" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]; }; then
      lock_key="$(printf '%s' "$target_entry" | sha256sum | awk '{print $1}')"
      exec {worktree_target_fd}>"$RESOURCE_LOCKS_DIR/worktree-target-$lock_key.lock"
      flock "$worktree_target_fd"
      if [ -L "$target_entry" ]; then
        rm "$target_entry"
      elif [ -d "$target_entry" ] && [ -z "$(find "$target_entry" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]; then
        rmdir "$target_entry"
      fi
      mkdir -p "$fallback_target_dir"
      [ -e "$target_entry" ] || ln -s "$fallback_target_dir" "$target_entry"
      flock -u "$worktree_target_fd"
      eval "exec ${worktree_target_fd}>&-"
      set_spec_environment_value CARGO_TARGET_DIR "$fallback_target_dir"
      set_spec_environment_value CARGO_INCREMENTAL "${CARGO_INCREMENTAL:-0}"
    fi
  fi

  if ! [[ -v RUSTC_WRAPPER ]] && command -v sccache >/dev/null 2>&1; then
    set_spec_environment_value SCCACHE_DIR "${SCCACHE_DIR:-$WORK_DIR/sccache}"
    set_spec_environment_value SCCACHE_CACHE_SIZE "${SCCACHE_CACHE_SIZE:-10G}"
    if ! [[ -v SCCACHE_SERVER_UDS ]] && ! [[ -v SCCACHE_SERVER_PORT ]]; then
      set_spec_environment_value SCCACHE_SERVER_UDS "$(default_sccache_server_uds "$SCCACHE_DIR" "$SCCACHE_CACHE_SIZE")"
    fi
    if ! [[ -v SCCACHE_IDLE_TIMEOUT ]]; then
      set_spec_environment_value SCCACHE_IDLE_TIMEOUT 0
    fi
    set_spec_environment_value RUSTC_WRAPPER "$(command -v sccache)"
  fi
}

resolved_cargo_target_dir() {
  local project_dir="$1"
  local fallback_target_dir="$2"
  local cargo_dir metadata target_dir
  cargo_dir="$(cargo_project_dir "$project_dir")"
  if [ -z "$cargo_dir" ]; then
    printf '%s\n' "$fallback_target_dir"
    return 0
  fi
  metadata="$(cd "$cargo_dir" && cargo metadata --format-version 1 --no-deps 2>/dev/null)" || true
  target_dir="$(jq -r '.target_directory // empty' <<<"$metadata" 2>/dev/null || true)"
  if [ -z "$target_dir" ]; then
    target_dir="$fallback_target_dir"
  fi
  printf '%s\n' "$target_dir"
}

acquire_resource_lock() {
  local identity="$1"
  local key
  key="$(printf '%s' "$identity" | sha256sum | awk '{print $1}')"
  exec {RESOURCE_LOCK_FD}>"$RESOURCE_LOCKS_DIR/$key.lock"
  flock "$RESOURCE_LOCK_FD"
}

try_acquire_coalescing_lock() {
  local spec="$1"
  local key
  key=$(printf '%s' "$spec" | spec_key)
  exec {COALESCE_LOCK_FD}>"$RESOURCE_LOCKS_DIR/coalesce-$key.lock"
  if flock -n "$COALESCE_LOCK_FD"; then
    return 0
  fi
  eval "exec ${COALESCE_LOCK_FD}>&-"
  COALESCE_LOCK_FD=""
  return 1
}

release_coalescing_lock() {
  if [ -n "${COALESCE_LOCK_FD:-}" ]; then
    flock -u "$COALESCE_LOCK_FD" || true
    eval "exec ${COALESCE_LOCK_FD}>&-"
    COALESCE_LOCK_FD=""
  fi
}

acquire_deploy_lock() {
  local identity="$1"
  local key
  key="$(printf '%s' "$identity" | sha256sum | awk '{print $1}')"
  exec {DEPLOY_LOCK_FD}>"$RESOURCE_LOCKS_DIR/deploy-$key.lock"
  flock "$DEPLOY_LOCK_FD"
}

deployment_target_keys() {
  local spec="$1"
  local project paths path
  project=$(printf '%s' "$spec" | jq -r '.project // ""')
  paths=""
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    paths+="$(realpath -m -- "$path")"$'\n'
  done < <(printf '%s' "$spec" | jq -r '(.audit_paths // [])[] | select(type == "string" and length > 0)')
  if [ -n "$paths" ]; then
    while IFS= read -r path; do
      printf 'audit:%s' "$path" | sha256sum | awk '{print $1}'
    done < <(printf '%s' "$paths" | sort -u)
  else
    printf 'project:%s' "$project" | sha256sum | awk '{print $1}'
  fi
}

acquire_deploy_target_locks() {
  local target_keys="$1"
  local target_key target_fd
  DEPLOY_TARGET_LOCK_FDS=()
  while IFS= read -r target_key; do
    [ -n "$target_key" ] || continue
    exec {target_fd}>"$RESOURCE_LOCKS_DIR/deploy-target-$target_key.lock"
    flock "$target_fd"
    DEPLOY_TARGET_LOCK_FDS+=("$target_fd")
  done < <(printf '%s\n' "$target_keys" | sort -u)
}

deployment_target_keys_overlap() {
  local left="$1"
  local right="$2"
  [ -n "$(comm -12 <(printf '%s\n' "$left" | sort -u) <(printf '%s\n' "$right" | sort -u) | head -n 1)" ]
}

acquire_concurrency_slot() {
  local slot candidate_fd
  while true; do
    for slot in $(seq 1 "$MAX_CONCURRENCY"); do
      exec {candidate_fd}>"$SLOT_LOCKS_DIR/$slot.lock"
      if flock -n "$candidate_fd"; then
        CONCURRENCY_SLOT_FD="$candidate_fd"
        CONCURRENCY_SLOT="$slot"
        return 0
      fi
      eval "exec ${candidate_fd}>&-"
    done
    sleep 1
  done
}

release_build_locks() {
  if [ -n "${CONCURRENCY_SLOT_FD:-}" ]; then
    flock -u "$CONCURRENCY_SLOT_FD" || true
    eval "exec ${CONCURRENCY_SLOT_FD}>&-"
    CONCURRENCY_SLOT_FD=""
  fi
  if [ -n "${DEPLOY_LOCK_FD:-}" ]; then
    flock -u "$DEPLOY_LOCK_FD" || true
    eval "exec ${DEPLOY_LOCK_FD}>&-"
    DEPLOY_LOCK_FD=""
  fi
  for target_fd in "${DEPLOY_TARGET_LOCK_FDS[@]}"; do
    flock -u "$target_fd" || true
    eval "exec ${target_fd}>&-"
  done
  DEPLOY_TARGET_LOCK_FDS=()
  if [ -n "${RESOURCE_LOCK_FD:-}" ]; then
    flock -u "$RESOURCE_LOCK_FD" || true
    eval "exec ${RESOURCE_LOCK_FD}>&-"
    RESOURCE_LOCK_FD=""
  fi
}

notification_subject() {
  local project="$1"
  local project_path="$2"
  if [ -z "$project" ] && [ -z "$project_path" ]; then
    return 0
  fi
  if [ "$project" = "$project_path" ]; then
    printf '%s\n' "$project"
    return 0
  fi
  if [ -z "$project" ]; then
    printf '%s\n' "$project_path"
    return 0
  fi
  if [ -z "$project_path" ]; then
    printf '%s\n' "$project"
    return 0
  fi
  printf '%s（%s）\n' "$project" "$project_path"
}

extract_error_summary() {
  # Extract the most relevant error lines from a build log so the consuming
  # agent can often fix the problem without a separate log-reading tool call.
  # Recognizes Rust compile errors (error[E0xxx]: + --> file:line), cargo
  # `error:` lines, Rust test failures (test ... FAILED + panicked at),
  # Node.js AssertionErrors, and generic Error:/SyntaxError lines.
  local log_file="$1"
  [ -f "$log_file" ] || return 0
  tr '\r' '\n' <"$log_file" \
    | awk '
      function flush() {
        if (pending_msg != "") {
          print (pending_loc != "" ? pending_msg " @" pending_loc : pending_msg)
          pending_msg=""; pending_loc=""
        }
      }
      /^error\[[E0-9]+\]:/ {
        flush()
        msg=$0; sub(/^error\[[E0-9]+\]: /, "", msg)
        pending_msg=substr(msg, 1, 110); pending_loc=""
        next
      }
      /^error: / && !/could not compile|error: test failed|error: aborting|error: cargo|error: the/ {
        flush()
        msg=$0; sub(/^error: /, "", msg)
        print substr(msg, 1, 110)
        next
      }
      pending_msg != "" && /^ *--> / {
        loc=$0; sub(/^ *--> /, "", loc); sub(/:[0-9]+:[0-9]+$/, "", loc)
        if (pending_loc == "") pending_loc=loc
        next
      }
      pending_msg != "" { flush() }
      /^test .* \.\.\. FAILED$/ {
        flush()
        name=$0; sub(/^test /, "", name); sub(/ \.\.\. FAILED$/, "", name)
        print "test FAILED: " substr(name, 1, 90)
        next
      }
      /^thread '"'"'.*'"'"' panicked at / {
        flush()
        file=$0; sub(/.*panicked at /, "", file); sub(/:.*/, "", file)
        print "panic @ " file
        next
      }
      /^AssertionError/ { flush(); print substr($0, 1, 110); next }
      END { flush() }
    ' \
    | awk '!seen[$0]++' \
    | head -3 \
    | paste -sd " | " -
}

request_spec() {
  local file="$1"
  jq -c --arg repo "$REPO_DIR" --argjson default_command "$DEFAULT_COMMAND_JSON" '
    {
      request_kind: (.request_kind // "compile"),
      project: (.project // "webClx"),
      project_dir: (.project_dir // $repo),
      project_path: (.project_path // .project // "webClx"),
      command: (if (.command | type) == "array" and (.command | length) > 0 then .command else $default_command end),
      compile_environment: (if (.compile_environment | type) == "array" then .compile_environment else [] end),
      install_command: (if (.install_command | type) == "array" then .install_command else [] end),
      audit_paths: (if (.audit_paths | type) == "array" then .audit_paths else [] end),
      required_artifacts: (if (.required_artifacts | type) == "array" then .required_artifacts else [] end)
    }
  ' "$file"
}

spec_key() {
  sha256sum | awk '{print $1}'
}

newer_successful_deploy_request() {
  local current_requested_at="$1"
  local current_request_id="$2"
  local current_target_keys="$3"
  local current_run_dir="$4"
  local file candidate_run_dir candidate_id candidate_requested_at candidate_spec candidate_key candidate_target_keys
  local newest_id="" newest_requested_at=0

  while IFS= read -r -d '' file; do
    candidate_run_dir=$(dirname "$file")
    [ "$candidate_run_dir" != "$current_run_dir" ] || continue
    candidate_id=$(json_get "$file" '.request_id')
    [ -n "$candidate_id" ] || continue
    [ "$(json_get "$file" '.request_kind')" = "deploy" ] || continue
    candidate_requested_at=$(json_get "$file" '.requested_at')
    [[ "$candidate_requested_at" =~ ^[0-9]+$ ]] || candidate_requested_at=0
    if [ "$candidate_requested_at" -lt "$current_requested_at" ]; then
      continue
    fi
    if [ "$candidate_requested_at" -eq "$current_requested_at" ] && [[ "$candidate_id" < "$current_request_id" || "$candidate_id" = "$current_request_id" ]]; then
      continue
    fi
    candidate_spec=$(request_spec "$file")
    candidate_target_keys=$(deployment_target_keys "$candidate_spec")
    deployment_target_keys_overlap "$candidate_target_keys" "$current_target_keys" || continue
    candidate_key=$(printf '%s' "$candidate_spec" | spec_key)
    [ -f "$candidate_run_dir/deploy-succeeded-$candidate_key" ] || continue
    if [ "$candidate_requested_at" -gt "$newest_requested_at" ] \
       || { [ "$candidate_requested_at" -eq "$newest_requested_at" ] && [[ "$candidate_id" > "$newest_id" ]]; }; then
      newest_requested_at="$candidate_requested_at"
      newest_id="$candidate_id"
    fi
  done < <(find "$RUNS_DIR" -mindepth 2 -maxdepth 2 -type f -name '*.json' -print0 2>/dev/null)

  [ -n "$newest_id" ] || return 1
  printf '%s\n' "$newest_id"
}

count_requests() {
  find "$REQUESTS_DIR" -maxdepth 1 -type f -name '*.json' 2>/dev/null | wc -l
}

absorb_matching_pending_requests() {
  local selected_spec="$1"
  local destination_dir="$2"
  local file
  while IFS= read -r -d '' file; do
    if [ "$(request_spec "$file")" = "$selected_spec" ]; then
      mv "$file" "$destination_dir/"
    fi
  done < <(find "$REQUESTS_DIR" -maxdepth 1 -type f -name '*.json' -print0 | sort -z)
}

latest_request_mtime() {
  local latest=0
  local file mtime
  while IFS= read -r -d '' file; do
    mtime=$(stat -c %Y "$file" 2>/dev/null || echo 0)
    if [ "$mtime" -gt "$latest" ]; then
      latest="$mtime"
    fi
  done < <(find "$REQUESTS_DIR" -maxdepth 1 -type f -name '*.json' -print0 2>/dev/null)
  echo "$latest"
}

effective_quiet_secs() {
  local value="$QUIET_SECS"
  local file file_value
  while IFS= read -r -d '' file; do
    file_value=$(json_get "$file" '.debounce_secs')
    if [[ "$file_value" =~ ^[0-9]+$ ]] && [ "$file_value" -gt "$value" ]; then
      value="$file_value"
    fi
  done < <(find "$REQUESTS_DIR" -maxdepth 1 -type f -name '*.json' -print0 2>/dev/null)
  echo "$value"
}

write_run_progress() {
  local project="$1"
  local phase="$2"
  local spec_index="$3"
  local spec_count="$4"
  local command_json="$5"
  local packages_completed="${6:-}"
  local packages_total="${7:-}"
  local current_package="${8:-}"
  local current_log_path="${9:-}"
  local progress_tmp="$run_dir/.progress-$$-${RANDOM}.tmp"

  jq -nc \
    --arg project "$project" \
    --arg phase "$phase" \
    --argjson spec_index "$spec_index" \
    --argjson spec_count "$spec_count" \
    --argjson command "$command_json" \
    --arg packages_completed "$packages_completed" \
    --arg packages_total "$packages_total" \
    --arg current_package "$current_package" \
    --arg log_path "$current_log_path" \
    --arg updated_at "$(date '+%F %T')" \
    '{
      project: $project,
      phase: $phase,
      spec_index: $spec_index,
      spec_count: $spec_count,
      command: $command,
      packages_completed: (if $packages_completed == "" then null else ($packages_completed | tonumber) end),
      packages_total: (if $packages_total == "" then null else ($packages_total | tonumber) end),
      current_package: (if $current_package == "" then null else $current_package end),
      log_path: (if $log_path == "" then null else $log_path end),
      updated_at: $updated_at
    }' >"$progress_tmp"
  mv -f "$progress_tmp" "$run_dir/progress.json"
}

update_cargo_progress_from_log() {
  local project="$1"
  local spec_index="$2"
  local spec_count="$3"
  local command_json="$4"
  local spec_log_file="$5"
  local recent progress_line package_line completed="" total="" current_package=""

  [ -f "$spec_log_file" ] || return 0
  recent=$(tr '\r' '\n' <"$spec_log_file" | tail -n 240)
  progress_line=$(printf '%s\n' "$recent" \
    | grep -vE '^(command|install_command)=' \
    | grep -E '[0-9]+/[0-9]+:[[:space:]]*[^[:space:]]+' \
    | tail -n 1 || true)
  if [[ "$progress_line" =~ ([0-9]+)/([0-9]+):[[:space:]]*([^[:space:]]+) ]]; then
    completed="${BASH_REMATCH[1]}"
    total="${BASH_REMATCH[2]}"
    current_package="${BASH_REMATCH[3]}"
  else
    package_line=$(printf '%s\n' "$recent" | grep -E '^[[:space:]]*(Compiling|Checking)[[:space:]]+[^[:space:]]+' | tail -n 1 || true)
    if [[ "$package_line" =~ (Compiling|Checking)[[:space:]]+([^[:space:]]+) ]]; then
      current_package="${BASH_REMATCH[2]}"
    fi
  fi
  write_run_progress "$project" "compile" "$spec_index" "$spec_count" "$command_json" "$completed" "$total" "$current_package" "$spec_log_file"
}

monitor_cargo_progress() {
  local project="$1"
  local spec_index="$2"
  local spec_count="$3"
  local command_json="$4"
  local spec_log_file="$5"
  while true; do
    update_cargo_progress_from_log "$project" "$spec_index" "$spec_count" "$command_json" "$spec_log_file"
    sleep 0.5
  done
}

monitor_install_progress() {
  local project="$1"
  local spec_index="$2"
  local spec_count="$3"
  local install_command_json="$4"
  local spec_log_file="$5"
  local previous_size=""
  local current_size
  while true; do
    current_size=$(stat -c %s "$spec_log_file" 2>/dev/null || echo 0)
    if [ "$current_size" != "$previous_size" ]; then
      write_run_progress "$project" "install" "$spec_index" "$spec_count" "$install_command_json" "" "" "" "$spec_log_file"
      previous_size="$current_size"
    fi
    sleep 1
  done
}

notify_build_complete() {
  local request_id="$1"
  local payload response
  [ -n "$request_id" ] || return 0
  payload=$(jq -nc --arg request_id "$request_id" '{request_id:$request_id}')
  refresh_local_auth_args
  response=$(curl -fsS \
    --noproxy '*' \
    --retry "$CALLBACK_RETRY_COUNT" \
    --retry-max-time "$CALLBACK_RETRY_MAX_TIME" \
    --retry-delay 1 \
    --retry-connrefused \
    "${LOCAL_AUTH_ARGS[@]}" \
    -H 'Content-Type: application/json' \
    -X POST \
    "$BASE_URL/api/build/compile/complete" \
    -d "$payload") || return 1
  printf '%s' "$response" | jq -e '.ok == true' >/dev/null
}

complete_interrupted_run() {
  local signal_name="$1"
  local exit_status="$2"
  local interrupted_at specs_file spec key file request_id payload
  [ -n "${ACTIVE_RUN_DIR:-}" ] || return 0
  [ -d "$ACTIVE_RUN_DIR" ] || return 0

  interrupted_at="$(date '+%F %T')"
  specs_file="$ACTIVE_RUN_DIR/specs.jsonl"
  if [ -f "$specs_file" ]; then
    while IFS= read -r spec; do
      [ -n "$spec" ] || continue
      key=$(printf '%s' "$spec" | spec_key)
      if [ ! -f "$ACTIVE_RUN_DIR/status-$key" ]; then
        printf '%s\n' "$exit_status" >"$ACTIVE_RUN_DIR/status-$key"
        printf '%s\n' "$interrupted_at" >"$ACTIVE_RUN_DIR/finished-$key"
      fi
    done <"$specs_file"
  fi
  printf '%s\n' "$signal_name" >"$ACTIVE_RUN_DIR/run-interrupted-signal"
  printf '%s\n' "$interrupted_at" >"$ACTIVE_RUN_DIR/run-finished-at"
  rm -f "$ACTIVE_RUN_DIR/progress.json"

  while IFS= read -r -d '' file; do
    request_id=$(json_get "$file" '.request_id')
    [ -n "$request_id" ] || continue
    payload=$(jq -nc --arg request_id "$request_id" '{request_id:$request_id}')
    refresh_local_auth_args
    curl -fsS \
      --noproxy '*' \
      --connect-timeout 2 \
      --max-time 5 \
      "${LOCAL_AUTH_ARGS[@]}" \
      -H 'Content-Type: application/json' \
      -X POST \
      "$BASE_URL/api/build/compile/complete" \
      -d "$payload" >/dev/null 2>&1 || true
  done < <(find "$ACTIVE_RUN_DIR" -maxdepth 1 -type f -name '*.json' -print0)
}

handle_worker_signal() {
  local signal_name="$1"
  local exit_status="$2"
  trap - INT TERM HUP
  set +e
  complete_interrupted_run "$signal_name" "$exit_status"
  exit "$exit_status"
}

notify_terminal() {
  local message_target_terminal_name="$1"
  local data="$2"
  local delivery_id="$3"
  local payload response
  payload=$(jq -nc \
    --arg target "$message_target_terminal_name" \
    --arg data "$data" \
    --arg delivery_id "$delivery_id" \
    '{target:$target,data:$data,submit:true,submit_enters:1,bracketed_paste:true,verify_submission:true,delivery_id:$delivery_id,completed_build_request_id:$delivery_id}')
  refresh_local_auth_args
  response=$(curl -fsS \
    --noproxy '*' \
    --retry "$CALLBACK_RETRY_COUNT" \
    --retry-max-time "$CALLBACK_RETRY_MAX_TIME" \
    --retry-delay 1 \
    --retry-connrefused \
    "${LOCAL_AUTH_ARGS[@]}" \
    -H 'Content-Type: application/json' \
    -X POST \
    "$BASE_URL/api/terminal/sessions/message" \
    -d "$payload") || return 1
  printf '%s' "$response" | jq -e '.ok == true and .submitted == true' >/dev/null
}

notify_terminal_toast() {
  local message_target_terminal_name="$1"
  local message="$2"
  local tone="$3"
  local payload
  payload=$(jq -nc \
    --arg target "$message_target_terminal_name" \
    --arg message "$message" \
    --arg tone "$tone" \
    '{target:$target,message:$message,tone:$tone}')
  refresh_local_auth_args
  curl -fsS \
    --noproxy '*' \
    --retry "$CALLBACK_RETRY_COUNT" \
    --retry-max-time "$CALLBACK_RETRY_MAX_TIME" \
    --retry-delay 1 \
    --retry-connrefused \
    "${LOCAL_AUTH_ARGS[@]}" \
    -H 'Content-Type: application/json' \
    -X POST \
    "$BASE_URL/api/build/compile/notify" \
    -d "$payload" >/dev/null
}

notify_request_file() {
  local file="$1"
  local source_terminal_id source_terminal_name source_tmux_session tmux_derived_id notify_target request_id note spec key request_kind project project_dir project_path status started_at finished_at
  local install_report request_log_file log_label report_label report_summary summary next_step run_context subject message toast_message toast_tone lock_key lock_file shown_started_at shown_finished_at is_noop_deploy notification_failed error_summary superseded_by
  source_terminal_id=$(json_get "$file" '.source_terminal_id')
  source_terminal_name=$(json_get "$file" '.source_terminal_name')
  source_tmux_session=$(json_get "$file" '.source_tmux_session')
  tmux_derived_id=""
  case "$source_tmux_session" in
    webclx_s[0-9]*)
      tmux_derived_id="${source_tmux_session#webclx_}"
      ;;
  esac
  notify_target="$source_terminal_id"
  if [ -z "$notify_target" ]; then
    notify_target="$tmux_derived_id"
  fi
  if [ -z "$notify_target" ]; then
    notify_target="$source_terminal_name"
  fi
  request_id=$(json_get "$file" '.request_id')
  note=$(json_get "$file" '.note')
  notification_failed=0
  if ! notify_build_complete "$request_id"; then
    echo "failed to complete build lifecycle request=$request_id" >>"$log_file"
    notification_failed=1
  fi
  if [ -z "$notify_target" ]; then
    return "$notification_failed"
  fi
  spec=$(request_spec "$file")
  key=$(printf '%s' "$spec" | spec_key)
  request_kind=$(printf '%s' "$spec" | jq -r '.request_kind // "compile"')
  project=$(printf '%s' "$spec" | jq -r '.project')
  project_dir=$(printf '%s' "$spec" | jq -r '.project_dir')
  project_path=$(printf '%s' "$spec" | jq -r '.project_path')
  is_noop_deploy=$(printf '%s' "$spec" | jq -r '
    if (.request_kind // "compile") == "deploy"
       and ((.install_command // []) | map(tostring) | any(endswith("/noop-deploy.sh") or . == "noop-deploy.sh"))
    then "true" else "false" end
  ')
  request_log_file=$(cat "$run_dir/log-$key.path" 2>/dev/null || echo "$log_file")
  status=$(cat "$run_dir/status-$key" 2>/dev/null || echo 1)
  superseded_by=$(cat "$run_dir/deploy-superseded-$key" 2>/dev/null || echo "")
  started_at=$(cat "$run_dir/started-$key" 2>/dev/null || echo "")
  finished_at=$(cat "$run_dir/finished-$key" 2>/dev/null || echo "")
  install_report=$(cat "$run_dir/install-report-$key.path" 2>/dev/null || echo "")
  log_label="$request_log_file"
  report_label=""
  if [ -n "$install_report" ]; then
    report_label="$install_report"
    report_summary=$(jq -r '
      .summary as $s
      | "created=\($s.created // 0), modified=\($s.modified // 0), removed=\($s.removed // 0), missing=\($s.missing // 0), unchanged=\($s.unchanged // 0)"
    ' "$install_report" 2>/dev/null || echo "")
  else
    report_summary=""
  fi
  shown_started_at=$(display_clock_time "$started_at")
  shown_finished_at=$(display_clock_time "$finished_at")
  timedout_label=""
  if [ -f "$run_dir/timedout-$key" ]; then
    timedout_label="$(cat "$run_dir/timedout-$key" 2>/dev/null || echo "")"
  fi
  if [ "$status" -eq 0 ] && [ -n "$superseded_by" ]; then
    summary="部署已跳过（被较新成功请求取代）"
    next_step="较新请求 ${superseded_by} 已成功部署，本请求未执行安装，避免旧版本回滚。"
    toast_tone="ok"
  elif [ "$status" -eq 0 ]; then
    if [ "$request_kind" = "deploy" ] && [ "$is_noop_deploy" != "true" ]; then
      summary="部署完成"
    else
      summary="编译完成"
    fi
    next_step="如果原任务还有未完成任务，请继续。"
    toast_tone="ok"
  elif [ -n "$timedout_label" ]; then
    # SEVERE: a command exceeded the global timeout and was force-killed. Without
    # this branch the user would wait forever for a callback that the hung worker
    # could never send; here we explicitly surface the timeout as a serious fault.
    if [ "$request_kind" = "deploy" ] && [ "$is_noop_deploy" != "true" ]; then
      summary="严重：部署超时（超过 ${timedout_label} 被强制终止）"
      next_step="部署命令卡住被超时杀掉。常见原因：scp/rsync 到远端网络不通、systemctl restart 等待依赖、deploy 脚本死锁。请查看日志尾部确认卡在哪一步，修复部署脚本或网络后重试。"
    else
      summary="严重：编译/测试超时（超过 ${timedout_label} 被强制终止）"
      next_step="编译或测试命令卡住被超时杀掉。常见原因：某个测试死锁/hang、等待交互输入、网络请求无响应。请查看日志尾部确认卡在哪个 target/测试，修复后重试。"
    fi
    toast_tone="warn"
  else
    if [ "$request_kind" = "deploy" ] && [ "$is_noop_deploy" != "true" ]; then
      summary="部署失败(status=$status)"
      next_step="请先查看日志和安装审计报告定位部署失败原因，修复后再继续原任务。"
    else
      summary="编译失败(status=$status)"
      next_step="请先查看日志定位编译失败原因，修复后再继续原任务。"
    fi
    toast_tone="warn"
  fi
  error_summary=""
  if [ "$status" -ne 0 ]; then
    error_summary=$(extract_error_summary "$request_log_file")
  fi
  if [ "$request_count" -gt 1 ]; then
    run_context="请求 ${request_id}；本轮合并 ${request_count} 个请求"
  else
    run_context="请求 ${request_id}"
  fi
  subject=$(notification_subject "$project" "$project_path")
  if [ -n "$subject" ]; then
    message="[from webClx-compile-api] ${subject} ${summary}；${run_context}；时间 ${shown_started_at}-${shown_finished_at}；webClx 集中日志：${log_label}"
    toast_message="${subject} ${summary}；请求 ${request_id}；完成时间 ${shown_finished_at}"
  else
    message="[from webClx-compile-api] ${summary}；${run_context}；时间 ${shown_started_at}-${shown_finished_at}；webClx 集中日志：${log_label}"
    toast_message="${summary}；请求 ${request_id}；完成时间 ${shown_finished_at}"
  fi
  if [ -n "$superseded_by" ]; then
    message="$message；替代请求：${superseded_by}"
  fi
  if [ -n "$error_summary" ]; then
    message="$message；关键错误：${error_summary}"
  fi
  if [ -n "$report_label" ]; then
    message="$message；webClx 集中审计：${report_label}"
  fi
  if [ -n "$report_summary" ]; then
    message="$message；差异：${report_summary}"
  fi
  message="$message。${next_step}"
  if [ -n "$note" ]; then
    message="$message 原始备注：$note。"
  fi
  lock_key=$(printf '%s' "$notify_target" | sha256sum | awk '{print $1}')
  lock_file="$run_dir/notify-$lock_key.lock"
  if ! (
    flock 8
    callback_failed=0
    if ! notify_terminal_toast "$notify_target" "$toast_message" "$toast_tone"; then
      echo "failed to notify source terminal toast source_terminal_id=$source_terminal_id source_terminal_name=$source_terminal_name source_tmux_session=$source_tmux_session notify_target=$notify_target request=$request_id" >>"$log_file"
      callback_failed=1
    fi
    if ! notify_terminal "$notify_target" "$message" "$request_id"; then
      echo "failed to notify source terminal source_terminal_id=$source_terminal_id source_terminal_name=$source_terminal_name source_tmux_session=$source_tmux_session notify_target=$notify_target request=$request_id" >>"$log_file"
      callback_failed=1
    fi
    exit "$callback_failed"
  ) 8>"$lock_file"; then
    notification_failed=1
  fi
  return "$notification_failed"
}

is_plausible_audit_candidate() {
  local path="$1"
  local project_dir="$2"
  local normalized normalized_project

  case "$path" in
    ""|//*) return 1 ;;
    /dev|/dev/*|/proc|/proc/*|/sys|/sys/*|/run|/run/*) return 1 ;;
  esac
  [[ "$path" = /* ]] || return 1
  normalized="$(realpath -m -- "$path" 2>/dev/null)" || return 1
  normalized_project="$(realpath -m -- "$project_dir" 2>/dev/null)" || return 1
  [ "$normalized" != "/" ] || return 1
  [ "$normalized" != "$normalized_project" ] || return 1
  [[ ! "$normalized" =~ ^/[^/]+$ ]]
}

collect_command_path_candidates() {
  local project_dir="$1"
  local command_json="$2"
  local audit_json="$3"
  local candidates_file scripts_file script_path
  candidates_file="$(mktemp)"
  scripts_file="$(mktemp)"
  jq -nr \
    --arg project_dir "$project_dir" \
    --argjson command "$command_json" \
    --argjson audit "$audit_json" '
      def command_args:
        [$command[] | tostring];
      def shellish:
        command_args | join(" ");
      def no_shell_meta:
        test("[[:space:]\"'\''`;&|<>]") | not;
      def absolute_mentions:
        [ shellish
          | scan("(^|[^[:alnum:]_$])(/[^[:space:]\"'\''`;&|<>]+)")
          | .[1]
        ];
      def relative_script_mentions:
        [ shellish
          | scan("(^|[[:space:]\"'\''`;&|<>])((?:\\./)?scripts/[^[:space:]\"'\''`;&|<>]+|\\.\\./[^[:space:]\"'\''`;&|<>]+\\.(?:sh|bash)|[^/[:space:]\"'\''`;&|<>][^[:space:]\"'\''`;&|<>]*\\.(?:sh|bash))")
          | .[1]
        ];
      def script_paths:
        [ command_args[]
          | select(no_shell_meta)
          | select(test("\\.(sh|bash)$") or startswith("./") or startswith("scripts/"))
        ];
      def path_args:
        [ command_args[]
          | select(no_shell_meta)
          | select(startswith("/") or startswith("../"))
        ];
      def explicit_paths:
        [ $audit[]
          | tostring
          | select(length > 0)
        ];
      def candidate_paths:
        (absolute_mentions + relative_script_mentions + path_args + script_paths + explicit_paths)
        | map(select(length > 0))
        | map(gsub("[,.;:)\\]]+$"; ""))
        | map(select(startswith("//") | not))
        | map(select(. != "/dev/null"))
        | map(select(contains("$") | not))
        | map(select(test("[*?\\[\\]{}]") | not))
        | map(select((startswith("/") | not) or test("^/[[:alnum:]_.-]")))
        | map(if startswith("/") then . else ($project_dir + "/" + .) end)
        | map(select(test("^/[[:alnum:]_./@+=,:-]+$")))
        | map(select(. != $project_dir))
        | unique;
      candidate_paths[]
    ' | tee "$candidates_file" | awk '/\.(sh|bash)$/ || /\/scripts\// {print}' >"$scripts_file"

  while IFS= read -r script_path; do
    if [ -f "$script_path" ]; then
      jq -Rr -s --arg project_dir "$project_dir" '
        split("\n")
        | map(select(test("^[[:space:]]*#") | not))
        | join("\n")
        | [ scan("(^|[^[:alnum:]_$])(/[^[:space:]\"'\''`;&|<>]+)") | .[1] ]
        | map(gsub("[,.;:)\\]]+$"; ""))
        | map(select(length > 0))
        | map(select(startswith("//") | not))
        | map(select(. != "/dev/null"))
        | map(select(contains("$") | not))
        | map(select(test("[*?\\[\\]{}]") | not))
        | map(select(test("^/[[:alnum:]_./@+=,:-]+$")))
        | map(select(. != $project_dir))
        | .[]
      ' "$script_path" 2>/dev/null || true
    fi
  done <"$scripts_file" >>"$candidates_file"

  while IFS= read -r candidate; do
    if is_plausible_audit_candidate "$candidate" "$project_dir"; then
      printf '%s\n' "$candidate"
    fi
  done < <(sort -u "$candidates_file")
  rm -f "$candidates_file" "$scripts_file"
}

collect_cargo_binary_candidates() {
  local project_dir="$1"
  local command_json="$2"
  local fallback_target_dir="$3"
  local cargo_dir metadata target_dir target_triple profile release_dir
  cargo_dir="$(cargo_project_dir "$project_dir")"
  if [ -z "$cargo_dir" ]; then
    return 0
  fi
  if ! jq -e '
    type == "array"
    and (
      (((index("cargo") != null) or (.[0] // "" | test("(^|/)cargo$"))) and (index("build") != null))
      or (join(" ") | test("rebuild-and-deploy\\.sh|cargo[[:space:]]+build"))
    )
  ' <<<"$command_json" >/dev/null 2>&1; then
    return 0
  fi
  metadata="$(cd "$cargo_dir" && cargo metadata --format-version 1 --no-deps 2>/dev/null)" || return 0
  target_dir="$(jq -r '.target_directory // empty' <<<"$metadata")"
  if [ -z "$target_dir" ]; then
    target_dir="$fallback_target_dir"
  fi
  target_triple="$(jq -r '
    . as $args
    | def arg_after($name):
        reduce range(0; ($args | length)) as $i (""; if . == "" and $args[$i] == $name then ($args[$i + 1] // "") else . end);
      ([ $args[] | select(startswith("--target=")) | sub("^--target="; "") ][0] // arg_after("--target") // "")
  ' <<<"$command_json")"
  if jq -e 'index("--release") != null or (join(" ") | test("--release|rebuild-and-deploy\\.sh"))' <<<"$command_json" >/dev/null 2>&1; then
    profile="release"
  else
    profile="debug"
  fi
  if [ -n "$target_triple" ]; then
    release_dir="$target_dir/$target_triple/$profile"
  else
    release_dir="$target_dir/$profile"
  fi
  jq -r --arg release_dir "$release_dir" '
    .packages[0].targets[]?
    | select((.kind // []) | index("bin"))
    | "\($release_dir)/\(.name)"
  ' <<<"$metadata"
}

collect_running_webclx_candidates() {
  local project="$1"
  local project_dir="$2"
  local command_json="$3"
  local is_webclx pid app_dir exe_path
  is_webclx=false
  if [ "$project" = "webClx" ] || [ "$project" = "webclx" ]; then
    is_webclx=true
  elif [ -f "$project_dir/Cargo.toml" ] && grep -Eq '^[[:space:]]*name[[:space:]]*=[[:space:]]*"webclx"' "$project_dir/Cargo.toml"; then
    is_webclx=true
  fi
  if [ "$is_webclx" != true ]; then
    return 0
  fi
  if ! jq -e 'type == "array" and (join(" ") | contains("rebuild-and-deploy.sh"))' <<<"$command_json" >/dev/null 2>&1; then
    return 0
  fi
  pid=$(ss -ltnp 2>/dev/null |
    awk '/:11111/ {print}' |
    grep -oE 'pid=[0-9]+' |
    head -n 1 |
    cut -d= -f2)
  if [ -n "$pid" ]; then
    exe_path="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
    app_dir="$(readlink "/proc/$pid/cwd" 2>/dev/null || true)"
    [ -n "$exe_path" ] && printf '%s\n' "$exe_path"
    [ -n "$app_dir" ] && [ -d "$app_dir/static" ] && printf '%s\n' "$app_dir/static"
  fi
}

collect_explicit_audit_candidates() {
  local project_dir="$1"
  local audit_json="$2"
  local candidate
  while IFS= read -r candidate; do
    if [[ "$candidate" != /* ]]; then
      candidate="$project_dir/$candidate"
    fi
    if is_plausible_audit_candidate "$candidate" "$project_dir"; then
      realpath -m -- "$candidate"
    fi
  done < <(jq -r '.[] | strings | select(length > 0)' <<<"$audit_json")
}

collect_deploy_audit_candidates() {
  local project="$1"
  local project_dir="$2"
  local command_json="$3"
  local install_command_json="$4"
  local audit_json="$5"
  local cargo_target_dir="$6"
  if jq -e 'length > 0' <<<"$audit_json" >/dev/null 2>&1; then
    collect_explicit_audit_candidates "$project_dir" "$audit_json" | awk 'NF' | sort -u
    return 0
  fi
  {
    collect_command_path_candidates "$project_dir" "$install_command_json" '[]'
    collect_cargo_binary_candidates "$project_dir" "$command_json" "$cargo_target_dir"
    collect_running_webclx_candidates "$project" "$project_dir" "$command_json"
  } | awk 'NF' | sort -u
}

path_snapshot_json() {
  local path="$1"
  local type="missing"
  local size="" mtime="" sha=""
  # Build the find prune expression once from SNAPSHOT_PRUNE_NAMES.
  # Pruning build artifacts (.git/target/node_modules/...) is essential: a prior
  # version hashed the whole project tree and a single 4.2GB cargo target/ dir
  # held the compile worker's single-flight lock for hours, stalling every later
  # queued request. Those dirs change on every build and carry no deploy-audit
  # signal, so excluding them both speeds up the snapshot and keeps it meaningful.
  local prune_expr=()
  local name
  for name in "${SNAPSHOT_PRUNE_NAMES[@]}"; do
    prune_expr+=(-name "$name" -o)
  done
  unset 'prune_expr[${#prune_expr[@]}-1]' # drop trailing -o
  if [ ! -e "$path" ]; then
    type="missing"
  elif [ -f "$path" ]; then
    type="file"
    size=$(stat -c '%s' "$path" 2>/dev/null || echo "")
    mtime=$(stat -c '%Y' "$path" 2>/dev/null || echo "")
    sha=$(sha256sum "$path" 2>/dev/null | awk '{print $1}' || true)
  elif [ -d "$path" ]; then
    type="directory"
    if mountpoint -q -- "$path" 2>/dev/null; then
      # Deployment scripts may create storage roots such as /mnt/lyydata. If
      # the candidate itself is mounted, auditing its contents would scan user
      # data unrelated to the deployment. Root metadata is enough to confirm
      # that the intended mount point exists without crossing that boundary.
      size=$(stat -c '%s' "$path" 2>/dev/null || echo "")
      mtime=$(stat -c '%Y' "$path" 2>/dev/null || echo "")
      sha=""
    else
      size=$(find "$path" -xdev \( -type d \( "${prune_expr[@]}" \) -prune \) -o -type f -printf '%s\n' 2>/dev/null | awk '{sum += $1} END {print sum + 0}')
      mtime=$(find "$path" -xdev -printf '%T@\n' 2>/dev/null | sort -nr | head -n 1 | cut -d. -f1)
      local file_count
      file_count=$(find "$path" -xdev \( -type d \( "${prune_expr[@]}" \) -prune \) -o -type f -printf '.' 2>/dev/null | wc -c)
      if [ "$file_count" -gt "$SNAPSHOT_MAX_FILES" ]; then
        # Too large to hash cheaply; record metadata only (size + mtime) so the
        # install report can still detect changes without a multi-hour hash walk.
        sha=""
      else
        sha=$(
          (
            cd "$path" &&
            while IFS= read -r -d '' rel_path; do
              file_hash=$(sha256sum "./$rel_path" 2>/dev/null | awk '{print $1}' || true)
              printf '%s  %s\n' "$file_hash" "$rel_path"
            done < <(find . -xdev \( -type d \( "${prune_expr[@]}" \) -prune \) -o -type f -printf '%P\0' 2>/dev/null | sort -z)
          ) | sha256sum | awk '{print $1}' || true
        )
      fi
    fi
  fi
  jq -nc \
    --arg path "$path" \
    --arg type "$type" \
    --arg size "$size" \
    --arg mtime "$mtime" \
    --arg sha "$sha" \
    '{
      path: $path,
      type: $type,
      exists: ($type != "missing"),
      size: (if $size == "" then null else ($size | tonumber) end),
      mtime: (if $mtime == "" then null else ($mtime | tonumber) end),
      sha256: (if $sha == "" then null else $sha end)
    }'
}

snapshot_install_audit() {
  local paths_file="$1"
  local output_file="$2"
  jq -Rn '
    [inputs | select(length > 0)]
    | unique
    | map({
        path: .,
        exists: false,
        size: null,
        mtime: null,
        sha256: null
      })
  ' <"$paths_file" >"$output_file"

  local tmp_file
  tmp_file="$output_file.tmp"
  jq -c '.[]' "$output_file" |
    while IFS= read -r item; do
      path=$(printf '%s' "$item" | jq -r '.path')
      if is_safe_snapshot_path "$path"; then
        path_snapshot_json "$path"
      else
        # A legitimate intended output may not exist before install. Keep its
        # placeholder so the after snapshot can report it as created.
        printf '%s\n' "$item"
      fi
    done | jq -s '.' >"$tmp_file"
  mv "$tmp_file" "$output_file"
}

verify_required_artifacts() {
  local project_dir="$1"
  shift
  local artifact path missing=0
  for artifact in "$@"; do
    if [[ ! "$artifact" =~ [^[:space:]] ]]; then
      echo "required artifact missing: <empty path>"
      missing=1
      continue
    fi
    if [[ "$artifact" = /* ]]; then
      path="$artifact"
    else
      path="$project_dir/$artifact"
    fi
    if [ -e "$path" ]; then
      echo "required artifact present: $path"
    else
      echo "required artifact missing: $path"
      missing=1
    fi
  done
  [ "$missing" -eq 0 ]
}

write_install_report() {
  local report_file="$1"
  local before_file="$2"
  local after_file="$3"
  local project="$4"
  local project_dir="$5"
  local command_json="$6"
  local install_command_json="$7"
  jq -n \
    --arg run_id "$run_id" \
    --arg project "$project" \
    --arg project_dir "$project_dir" \
    --arg generated_at "$(date '+%F %T')" \
    --argjson command "$command_json" \
    --argjson install_command "$install_command_json" \
    --slurpfile before "$before_file" \
    --slurpfile after "$after_file" '
      def index_by_path($items): reduce $items[] as $item ({}; .[$item.path] = $item);
      def status($before; $after):
        if (($before.exists // false) | not) and ($after.exists // false) then "created"
        elif ($before.exists // false) and (($after.exists // false) | not) then "removed"
        elif (($before.exists // false) | not) and (($after.exists // false) | not) then "missing"
        elif ($before.size != $after.size or $before.mtime != $after.mtime or $before.sha256 != $after.sha256) then "modified"
        else "unchanged"
        end;
      ($before[0] // []) as $before_items
      | ($after[0] // []) as $after_items
      | index_by_path($before_items) as $before_map
      | index_by_path($after_items) as $after_map
      | (($before_map | keys) + ($after_map | keys) | unique) as $paths
      | [
          $paths[] as $path
          | ($before_map[$path] // {path:$path, exists:false}) as $before
          | ($after_map[$path] // {path:$path, exists:false}) as $after
          | {
              path: $path,
              status: status($before; $after),
              before: $before,
              after: $after
            }
        ] as $files
      | {
          run_id: $run_id,
          project: $project,
          project_dir: $project_dir,
          generated_at: $generated_at,
          command: $command,
          install_command: $install_command,
          files: $files,
          summary: {
            created: ([$files[] | select(.status == "created")] | length),
            modified: ([$files[] | select(.status == "modified")] | length),
            removed: ([$files[] | select(.status == "removed")] | length),
            unchanged: ([$files[] | select(.status == "unchanged")] | length),
            missing: ([$files[] | select(.status == "missing")] | length)
          }
        }
    ' >"$report_file"
}

ACTIVE_RUN_DIR=""
trap 'handle_worker_signal INT 130' INT
trap 'handle_worker_signal TERM 143' TERM
trap 'handle_worker_signal HUP 129' HUP

while true; do
  if [ "$(count_requests)" -eq 0 ]; then
    exit 0
  fi

  stable_count=-1
  while true; do
    current_count=$(count_requests)
    if [ "$current_count" -eq 0 ]; then
      exit 0
    fi
    quiet_secs=$(effective_quiet_secs)
    latest_mtime=$(latest_request_mtime)
    age=$(( $(now_secs) - latest_mtime ))
    if [ "$current_count" -eq "$stable_count" ] && [ "$age" -ge "$quiet_secs" ]; then
      break
    fi
    stable_count="$current_count"
    sleep 5
  done

  exec 7>"$CLAIM_LOCK_FILE"
  flock 7
  if [ "$(count_requests)" -eq 0 ]; then
    flock -u 7
    exit 0
  fi

  selected_spec=""
  while IFS= read -r -d '' file; do
    candidate_spec=$(request_spec "$file")
    if try_acquire_coalescing_lock "$candidate_spec"; then
      selected_spec="$candidate_spec"
      break
    fi
  done < <(find "$REQUESTS_DIR" -maxdepth 1 -type f -name '*.json' -print0 | sort -z)
  if [ -z "$selected_spec" ]; then
    flock -u 7
    sleep 1
    continue
  fi

  run_id="$(date +%Y%m%dT%H%M%S)-$$-${RANDOM}"
  display_run_id_value=$(display_run_id "$run_id")
  run_dir="$RUNS_DIR/$run_id"
  ACTIVE_RUN_DIR="$run_dir"
  run_output_dir="$run_dir"
  log_file="$run_dir/run.log"
  mkdir -p "$run_output_dir"
  absorb_matching_pending_requests "$selected_spec" "$run_dir"
  flock -u 7

  request_count=$(find "$run_dir" -maxdepth 1 -type f -name '*.json' | wc -l)
  if [ "$request_count" -eq 0 ]; then
    release_coalescing_lock
    continue
  fi

  specs_file="$run_dir/specs.jsonl"
  printf '%s\n' "$selected_spec" >"$specs_file"
  spec_count=$(wc -l <"$specs_file" | tr -d ' ')
  spec_index=0

  run_started_at="$(date '+%F %T')"
  printf '%s\n' "$run_started_at" >"$run_dir/run-started-at"
  {
    echo "run_id=$run_id"
    echo "requests=$request_count"
    echo "started_at=$run_started_at"
    echo
  } >"$log_file"

  while IFS= read -r spec; do
    if [ -z "$spec" ]; then
      continue
    fi
    spec_index=$((spec_index + 1))
    key=$(printf '%s' "$spec" | spec_key)
    request_kind=$(printf '%s' "$spec" | jq -r '.request_kind // "compile"')
    project=$(printf '%s' "$spec" | jq -r '.project')
    project_dir=$(printf '%s' "$spec" | jq -r '.project_dir')
    if [ "$spec_count" -gt 1 ]; then
      spec_file_suffix="$spec_index"
    else
      spec_file_suffix="1"
    fi
    spec_log_file="$run_output_dir/build-$spec_file_suffix.log"
    command_json=$(printf '%s' "$spec" | jq -c '.command')
    install_command_json=$(printf '%s' "$spec" | jq -c '.install_command // []')
    audit_paths_json=$(printf '%s' "$spec" | jq -c '.audit_paths // []')
    deploy_target_keys=""
    if [ "$request_kind" = "deploy" ]; then
      deploy_target_keys=$(deployment_target_keys "$spec")
    fi
    # User-configured extra environment for compile/install commands (e.g. PATH,
    # CARGO_HOME, RUSTUP_HOME pointing at a specific rustup). Exported before
    # running the command so it overrides the worker's bootstrap defaults.
    compile_env_json=$(printf '%s' "$spec" | jq -c '.compile_environment // []')
    export_compile_environment() {
      # Export each key=value from compile_env_json, skipping empty keys.
      # Keys are restricted to [_A-Za-z][_A-Za-z0-9]* by the backend sanitizer.
      local count
      count=$(printf '%s' "$compile_env_json" | jq 'length')
      local i=0
      while [ "$i" -lt "$count" ]; do
        local k v
        k=$(printf '%s' "$compile_env_json" | jq -r ".[$i].key // empty")
        v=$(printf '%s' "$compile_env_json" | jq -r ".[$i].value // empty")
        if [ -n "$k" ]; then
          set_spec_environment_value "$k" "$v"
        fi
        i=$((i + 1))
      done
    }

    mapfile -t command < <(printf '%s' "$spec" | jq -r '.command[]')
    mapfile -t install_command < <(printf '%s' "$spec" | jq -r '(.install_command // [])[]')
    mapfile -t required_artifacts < <(printf '%s' "$spec" | jq -r '(.required_artifacts // [])[]')
    # Keep request temp paths short enough for tools such as sccache, which can
    # create a Unix-domain socket below TMPDIR (Linux sun_path is 108 bytes).
    # The run ID is unique, and spec_index is unique within this run.
    spec_tmp_dir="$WORK_TMP_DIR/$display_run_id_value-$spec_index"
    fallback_cargo_target_dir="$(default_cargo_target_dir "$project" "$project_dir")"
    restore_spec_environment
    export_compile_environment
    configure_cargo_build_environment "$project" "$project_dir" "$fallback_cargo_target_dir"
    spec_cargo_target_dir="$(resolved_cargo_target_dir "$project_dir" "$fallback_cargo_target_dir")"
    if [ -n "$(cargo_project_dir "$project_dir")" ]; then
      build_resource="target:$(realpath -m "$spec_cargo_target_dir")"
    else
      build_resource="project:$(realpath -m "$project_dir")"
    fi
    write_run_progress "$project" "waiting" "$spec_index" "$spec_count" "$command_json" "" "" "" "$spec_log_file"
    acquire_resource_lock "$build_resource"
    if [ "$request_kind" = "deploy" ]; then
      deploy_resource="$(realpath -m "$project_dir")"
      acquire_deploy_lock "$deploy_resource"
    else
      deploy_resource=""
    fi
    acquire_concurrency_slot
    # This spec owned the coalescing lock while waiting for shared build and
    # deploy resources. Take one final snapshot before compilation starts.
    flock 7
    absorb_matching_pending_requests "$spec" "$run_dir"
    request_count=$(find "$run_dir" -maxdepth 1 -type f -name '*.json' | wc -l)
    current_requested_at=0
    current_request_id=""
    while IFS= read -r -d '' request_file; do
      candidate_request_id=$(json_get "$request_file" '.request_id')
      [ -n "$candidate_request_id" ] || continue
      candidate_requested_at=$(json_get "$request_file" '.requested_at')
      [[ "$candidate_requested_at" =~ ^[0-9]+$ ]] || candidate_requested_at=0
      if [ "$candidate_requested_at" -gt "$current_requested_at" ] \
         || { [ "$candidate_requested_at" -eq "$current_requested_at" ] && [[ "$candidate_request_id" > "$current_request_id" ]]; }; then
        current_requested_at="$candidate_requested_at"
        current_request_id="$candidate_request_id"
      fi
    done < <(find "$run_dir" -maxdepth 1 -type f -name '*.json' -print0)
    release_coalescing_lock
    flock -u 7
    audit_paths_file="$run_dir/install-audit-paths-$key.txt"
    before_file="$run_dir/install-before-$key.json"
    after_file="$run_dir/install-after-$key.json"
    report_file="$run_output_dir/install-report-$spec_file_suffix.json"

    started_at="$(date '+%F %T')"
    mkdir -p "$spec_tmp_dir" 2>/dev/null || true
    printf '%s\n' "$spec_log_file" >"$run_dir/log-$key.path"
    printf '%s\n' "$started_at" >"$run_dir/started-$key"
    write_run_progress "$project" "preparing" "$spec_index" "$spec_count" "$command_json" "" "" "" "$spec_log_file"
    set +e
    {
      # The Cargo project owns target-directory selection. Clear any value
      # inherited from the long-lived coordinator, then apply only the
      # request's explicit compile environment before resolving metadata.
      mkdir -p "$spec_cargo_target_dir" 2>/dev/null || true
      echo "===== project=$project key=$key ====="
      echo "request_kind=$request_kind"
      echo "project_dir=$project_dir"
      echo "started_at=$started_at"
      printf 'command='
      printf '%q ' "${command[@]}"
      echo
      echo "tmp_dir=$spec_tmp_dir"
      echo "concurrency_slot=$CONCURRENCY_SLOT/$MAX_CONCURRENCY"
      echo "build_resource=$build_resource"
      if [ -n "$deploy_resource" ]; then
        echo "deploy_resource=$deploy_resource"
      fi
      if [ -n "$(cargo_project_dir "$project_dir")" ]; then
        echo "cargo_target_dir=$spec_cargo_target_dir"
      fi
      if [ "$request_kind" = "deploy" ]; then
        printf 'install_command='
        printf '%q ' "${install_command[@]}"
        echo
      fi
      echo
      if [ ! -d "$project_dir" ]; then
        echo "project_dir does not exist: $project_dir"
        false
      else
        export CARGO_TERM_PROGRESS_WHEN="${CARGO_TERM_PROGRESS_WHEN:-always}"
        export CARGO_TERM_PROGRESS_WIDTH="${CARGO_TERM_PROGRESS_WIDTH:-100}"
        write_run_progress "$project" "compile" "$spec_index" "$spec_count" "$command_json" "" "" "" "$spec_log_file"
        monitor_cargo_progress "$project" "$spec_index" "$spec_count" "$command_json" "$spec_log_file" &
        progress_monitor_pid=$!
        (cd "$project_dir" && TMPDIR="$spec_tmp_dir" run_command_with_timeout "$run_dir/timedout-$key" "${command[@]}")
        compile_status=$?
        kill "$progress_monitor_pid" 2>/dev/null || true
        wait "$progress_monitor_pid" 2>/dev/null || true
        update_cargo_progress_from_log "$project" "$spec_index" "$spec_count" "$command_json" "$spec_log_file"
        if [ "$compile_status" -ne 0 ]; then
          echo "compile command failed with status=$compile_status"
          [ -f "$run_dir/timedout-$key" ] && echo "compile command timed out after $(cat "$run_dir/timedout-$key")"
          false
        elif ! verify_required_artifacts "$project_dir" "${required_artifacts[@]}"; then
          echo "required artifact verification failed"
          false
        elif [ "$request_kind" = "deploy" ]; then
          if [ "${#install_command[@]}" -eq 0 ]; then
            echo "deploy request missing install_command"
            false
          else
            acquire_deploy_target_locks "$deploy_target_keys"
            superseded_by=$(newer_successful_deploy_request "$current_requested_at" "$current_request_id" "$deploy_target_keys" "$run_dir" || true)
            if [ -n "$superseded_by" ]; then
              printf '%s\n' "$superseded_by" >"$run_dir/deploy-superseded-$key"
              echo "install skipped: newer successful deployment request=$superseded_by"
            else
              collect_deploy_audit_candidates "$project" "$project_dir" "$command_json" "$install_command_json" "$audit_paths_json" "$spec_cargo_target_dir" >"$audit_paths_file"
              snapshot_install_audit "$audit_paths_file" "$before_file"
              echo "audit_candidates=$(wc -l <"$audit_paths_file" | tr -d ' ')"
              if [ -s "$audit_paths_file" ]; then
                sed 's/^/audit_candidate=/' "$audit_paths_file"
              fi
              write_run_progress "$project" "install" "$spec_index" "$spec_count" "$install_command_json" "" "" "" "$spec_log_file"
              echo
              echo "running install command"
              monitor_install_progress "$project" "$spec_index" "$spec_count" "$install_command_json" "$spec_log_file" &
              install_progress_monitor_pid=$!
              (cd "$project_dir" && TMPDIR="$spec_tmp_dir" run_command_with_timeout "$run_dir/timedout-$key" "${install_command[@]}")
              install_status=$?
              kill "$install_progress_monitor_pid" 2>/dev/null || true
              wait "$install_progress_monitor_pid" 2>/dev/null || true
              [ -f "$run_dir/timedout-$key" ] && echo "install command timed out after $(cat "$run_dir/timedout-$key")"
              snapshot_install_audit "$audit_paths_file" "$after_file"
              write_install_report "$report_file" "$before_file" "$after_file" "$project" "$project_dir" "$command_json" "$install_command_json"
              cp "$report_file" "$run_dir/install-report-$key.json"
              printf '%s\n' "$report_file" >"$run_dir/install-report-$key.path"
              echo "install_report=$report_file"
              jq -r '
                .summary as $s
                | "audit_summary created=\($s.created // 0) modified=\($s.modified // 0) removed=\($s.removed // 0) missing=\($s.missing // 0) unchanged=\($s.unchanged // 0)"
              ' "$report_file" 2>/dev/null || true
              if [ "$install_status" -ne 0 ]; then
                echo "install command failed with status=$install_status"
                false
              else
                : >"$run_dir/deploy-succeeded-$key"
              fi
            fi
          fi
        fi
      fi
    } >>"$spec_log_file" 2>&1
    status=$?
    set -e
    release_build_locks
    finished_at="$(date '+%F %T')"
    {
      echo "status=$status"
      echo "finished_at=$finished_at"
      echo
    } >>"$spec_log_file"

    printf '%s\n' "$status" >"$run_dir/status-$key"
    printf '%s\n' "$finished_at" >"$run_dir/finished-$key"
  done <"$specs_file"

  printf '%s\n' "$(date '+%F %T')" >"$run_dir/run-finished-at"
  rm -f "$run_dir/progress.json"

  notify_pids=()
  while IFS= read -r -d '' file; do
    notify_request_file "$file" &
    notify_pids+=("$!")
  done < <(find "$run_dir" -maxdepth 1 -type f -name '*.json' -print0 | sort -z)
  notify_status=0
  for pid in "${notify_pids[@]}"; do
    if ! wait "$pid"; then
      notify_status=1
    fi
  done
  if [ "$notify_status" -ne 0 ]; then
    echo "one or more compile completion notification tasks failed" >>"$log_file"
    exit 1
  fi
  ACTIVE_RUN_DIR=""
done
