#!/usr/bin/env bash
# Consolidate Cargo target trees on /data while keeping each workspace's
# normal target path as a compatibility symlink.
set -euo pipefail

ROOT="/home/codes"
CACHE_ROOT="/data/cargo-target/webclx-compile/cargo-target"
QUEUE_DIR="/home/codes/webClx/.webclx-compile-queue"
APPLY=false

usage() {
  printf '%s\n' \
    "Usage: bash scripts/unify-cargo-targets.sh [--apply] [--root DIR] [--cache-root DIR] [--queue-dir DIR]" \
    "" \
    "Without --apply, prints the planned workspace/cache mapping only."
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --apply) APPLY=true; shift ;;
    --root) ROOT="${2:-}"; shift 2 ;;
    --cache-root) CACHE_ROOT="${2:-}"; shift 2 ;;
    --queue-dir) QUEUE_DIR="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

for required_command in cargo jq realpath sha256sum rsync flock timeout; do
  command -v "$required_command" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "$required_command" >&2
    exit 1
  }
done

ROOT="$(realpath "$ROOT")"
CACHE_ROOT="$(realpath -m "$CACHE_ROOT")"
QUEUE_DIR="$(realpath -m "$QUEUE_DIR")"
workspace_file="$(mktemp)"
trap 'rm -f "$workspace_file"' EXIT

discover_workspaces() {
  local manifest workspace_manifest workspace
  while IFS= read -r -d '' manifest; do
    workspace_manifest="$(
      timeout --signal=TERM --kill-after=5s 30s \
        env RUSTUP_TOOLCHAIN=stable cargo locate-project --workspace \
        --manifest-path "$manifest" \
        --message-format plain 2>/dev/null
    )" || true
    if [ -n "$workspace_manifest" ] && [ -f "$workspace_manifest" ]; then
      workspace="$(realpath "$(dirname "$workspace_manifest")")"
      printf '%s\n' "$workspace"
    else
      # A manifest that Cargo itself cannot assign to a workspace cannot be
      # built directly either (for example a vendored crate omitted from its
      # parent's members/exclude lists). Keep it visible but do not invent an
      # owner or target path for it.
      printf 'warning=invalid-cargo-manifest manifest=%s\n' "$manifest" >&2
    fi
  done < <(
    find "$ROOT" \
      \( -type d \( \
        -name .git -o \
        -name target -o \
        -name node_modules -o \
        -name .worktrees -o \
        -name '.webclx-deploy-*' \
      \) -prune \) -o \
      -type f -name Cargo.toml -print0
  ) | sort -u >"$workspace_file"
}

safe_slug() {
  local value="$1"
  value="$(printf '%s' "$value" | tr -c '[:alnum:]_.-' '_' | sed -E 's/^_+//; s/_+$//')"
  [ -n "$value" ] || value="workspace"
  printf '%s\n' "$value"
}

workspace_hash() {
  printf '%s' "$1" | sha256sum | awk '{print $1}' | cut -c1-16
}

path_is_below() {
  local path="$1"
  local parent="$2"
  case "$path" in
    "$parent"|"$parent"/*) return 0 ;;
    *) return 1 ;;
  esac
}

cache_matches_for_hash() {
  local hash="$1"
  local candidate
  [ -d "$CACHE_ROOT" ] || return 0
  for candidate in "$CACHE_ROOT"/*-"$hash"; do
    [ -e "$candidate" ] || [ -L "$candidate" ] || continue
    printf '%s\n' "$candidate"
  done
}

preferred_cache_dir() {
  local workspace="$1"
  local target_real="$2"
  local hash slug preferred
  local -a matches=()
  # A target path that differs from Cargo's workspace default is an explicit
  # project choice (including an existing compatibility symlink). Keep it
  # authoritative regardless of which mounted data root it uses.
  if [ "$target_real" != "$workspace/target" ]; then
    printf '%s\n' "$target_real"
    return 0
  fi
  hash="$(workspace_hash "$workspace")"
  slug="$(safe_slug "$(basename "$workspace")")"
  preferred="$CACHE_ROOT/$slug-$hash"
  mapfile -t matches < <(cache_matches_for_hash "$hash")
  if [ -e "$preferred" ] || [ -L "$preferred" ]; then
    printf '%s\n' "$(realpath -m "$preferred")"
  elif [ "${#matches[@]}" -eq 1 ]; then
    printf '%s\n' "$(realpath -m "${matches[0]}")"
  else
    printf '%s\n' "$preferred"
  fi
}

is_cargo_target_tree() {
  local path="$1"
  [ -f "$path/CACHEDIR.TAG" ] ||
    [ -f "$path/.rustc_info.json" ] ||
    [ -d "$path/debug/deps" ] ||
    [ -d "$path/release/deps" ]
}

owning_workspace_for_path() {
  local path="$1"
  local candidate owner=""
  while IFS= read -r candidate; do
    if path_is_below "$path" "$candidate" && [ "${#candidate}" -gt "${#owner}" ]; then
      owner="$candidate"
    fi
  done <"$workspace_file"
  printf '%s\n' "$owner"
}

legacy_target_dirs() {
  local workspace="$1"
  local logical_target="$2"
  local destination="$3"
  local candidate candidate_real ancestor ancestor_hash
  while IFS= read -r -d '' candidate; do
    candidate_real="$(realpath -m "$candidate")"
    [ "$candidate_real" != "$destination" ] || continue
    if [ "$(owning_workspace_for_path "$candidate")" = "$workspace" ] && \
      { [ "$candidate" = "$logical_target" ] || is_cargo_target_tree "$candidate"; }; then
      printf '%s\n' "$candidate"
    fi
  done < <(
    find "$workspace" \
      \( -type d \( -name .git -o -name node_modules \) -prune \) -o \
      -type d -name target -print0
  )

  while IFS= read -r candidate; do
    [ -n "$candidate" ] || continue
    candidate_real="$(realpath -m "$candidate")"
    [ "$candidate_real" != "$destination" ] && printf '%s\n' "$candidate"
  done < <(cache_matches_for_hash "$(workspace_hash "$workspace")")

  # Older wrappers sometimes submitted a project parent while their build
  # script changed into its sole nested Rust workspace (for example newsKB/rust).
  ancestor="$(dirname "$workspace")"
  while path_is_below "$ancestor" "$ROOT" && [ "$ancestor" != "$ROOT" ]; do
    if ! grep -Fqx "$ancestor" "$workspace_file"; then
      ancestor_hash="$(workspace_hash "$ancestor")"
      while IFS= read -r candidate; do
        [ -n "$candidate" ] || continue
        candidate_real="$(realpath -m "$candidate")"
        [ "$candidate_real" != "$destination" ] && printf '%s\n' "$candidate"
      done < <(cache_matches_for_hash "$ancestor_hash")
    fi
    ancestor="$(dirname "$ancestor")"
  done
}

assert_no_direct_cargo_build() {
  local processes
  processes="$(pgrep -a -f '(^|/)(cargo|rustc)( |$)' || true)"
  if [ -n "$processes" ]; then
    printf 'active Cargo/rustc process prevents migration:\n%s\n' "$processes" >&2
    return 1
  fi
}

merge_target_tree() {
  local workspace="$1"
  local source="$2"
  local destination="$3"
  local remaining source_real
  [ -e "$source" ] || [ -L "$source" ] || return 0
  source_real="$(realpath -m "$source")"
  [ "$source_real" != "$destination" ] || return 0
  if [ -L "$source" ]; then
    if [ ! -e "$source" ]; then
      printf 'replace_broken_target_symlink=%s old_destination=%s destination=%s\n' \
        "$source" "$source_real" "$destination"
      rm "$source"
      ln -s "$destination" "$source"
      return 0
    fi
    printf 'refusing to replace divergent target symlink: %s -> %s\n' \
      "$source" "$source_real" >&2
    return 1
  fi
  assert_no_direct_cargo_build
  printf 'migrate_source=%s destination=%s\n' "$source" "$destination"
  rsync -a --update "$source/" "$destination/"
  remaining="$(rsync -anic --update "$source/" "$destination/")"
  if [ -n "$remaining" ]; then
    printf 'migration verification failed for %s:\n%s\n' "$source" "$remaining" >&2
    return 1
  fi
  # Some legacy Cargo trees predate CACHEDIR.TAG, so modern cargo clean refuses
  # to touch them. The rsync dry-run above proves the destination contains an
  # equal or newer copy of every source entry. Delete only within this audited
  # target root, one entry at a time, and never cross filesystem boundaries.
  find "$source" -xdev -depth -mindepth 1 -delete
  rmdir "$source"
  ln -s "$destination" "$source"
}

unify_workspace() {
  local workspace="$1"
  local metadata logical_target target_real destination action source
  local -a sources=()
  metadata="$(
    cd "$workspace" && \
    timeout --signal=TERM --kill-after=5s 30s \
      env -u CARGO_TARGET_DIR RUSTUP_TOOLCHAIN=stable \
      cargo metadata --format-version 1 --no-deps
  )"
  logical_target="$(jq -r '.target_directory' <<<"$metadata")"
  target_real="$(realpath -m "$logical_target")"
  destination="$(preferred_cache_dir "$workspace" "$target_real")"
  destination="$(realpath -m "$destination")"
  mapfile -t sources < <(
    legacy_target_dirs "$workspace" "$logical_target" "$destination" | sort -u
  )

  action="already-unified"
  if [ "${#sources[@]}" -gt 0 ]; then
    action="migrate"
  elif [ ! -e "$workspace/target" ] && [ ! -L "$workspace/target" ]; then
    action="link-new"
  elif [ "$(realpath -m "$workspace/target")" != "$destination" ]; then
    action="migrate"
  fi
  printf 'workspace=%s target=%s destination=%s action=%s sources=%s\n' \
    "$workspace" "$logical_target" "$destination" "$action" "${#sources[@]}"
  [ "$APPLY" = true ] || return 0

  mkdir -p "$destination"
  for source in "${sources[@]}"; do
    merge_target_tree "$workspace" "$source" "$destination"
  done
  if [ "$workspace/target" != "$destination" ]; then
    if [ -e "$workspace/target" ] || [ -L "$workspace/target" ]; then
      if [ "$(realpath -m "$workspace/target")" != "$destination" ]; then
        merge_target_tree "$workspace" "$workspace/target" "$destination"
      fi
    else
      ln -s "$destination" "$workspace/target"
    fi
  fi
}

discover_workspaces
workspace_count="$(wc -l <"$workspace_file" | tr -d ' ')"
printf 'mode=%s root=%s cache_root=%s workspace_count=%s\n' \
  "$([ "$APPLY" = true ] && printf apply || printf dry-run)" \
  "$ROOT" "$CACHE_ROOT" "$workspace_count"

if [ "$APPLY" = true ]; then
  mkdir -p "$CACHE_ROOT" "$QUEUE_DIR"
  exec 9>"$QUEUE_DIR/worker.lock"
  flock 9
  assert_no_direct_cargo_build
fi

while IFS= read -r workspace; do
  [ -n "$workspace" ] || continue
  unify_workspace "$workspace"
done <"$workspace_file"
