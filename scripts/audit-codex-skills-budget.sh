#!/usr/bin/env bash
set -u

usage() {
  cat <<'EOF'
Usage: audit-codex-skills-budget.sh [options]

Options:
  --config PATH                    Codex TOML config path
  --root DIR                       Skill root; may be repeated
  --max-description-chars NUMBER   Maximum active description characters
  --max-active-skills NUMBER       Maximum active canonical skills
  -h, --help                       Show this help
EOF
}

die_usage() {
  printf 'error: %s\n' "$1" >&2
  usage >&2
  exit 2
}

is_positive_integer() {
  case "$1" in
    ''|*[!0-9]*|0) return 1 ;;
    *) return 0 ;;
  esac
}

config_path="$HOME/.codex/config.toml"
max_description_chars=13000
max_active_skills=120
roots=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --config)
      [ "$#" -ge 2 ] || die_usage '--config requires a path'
      config_path=$2
      shift 2
      ;;
    --root)
      [ "$#" -ge 2 ] || die_usage '--root requires a directory'
      roots+=("$2")
      shift 2
      ;;
    --max-description-chars)
      [ "$#" -ge 2 ] || die_usage '--max-description-chars requires a number'
      max_description_chars=$2
      shift 2
      ;;
    --max-active-skills)
      [ "$#" -ge 2 ] || die_usage '--max-active-skills requires a number'
      max_active_skills=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die_usage "unknown argument: $1"
      ;;
  esac
done

is_positive_integer "$max_description_chars" || die_usage '--max-description-chars must be a positive integer'
is_positive_integer "$max_active_skills" || die_usage '--max-active-skills must be a positive integer'
[ -r "$config_path" ] || die_usage "config is not readable: $config_path"

if [ "${#roots[@]}" -eq 0 ]; then
roots+=("$HOME/.codex/skills")
fi
for root in "${roots[@]}"; do
  [ -d "$root" ] || die_usage "skill root is not a directory: $root"
done

read_disabled_paths() {
  awk '
    function emit() {
      if (in_skill && path != "" && enabled == "false") print path
    }
    /^\[\[skills\.config\]\][[:space:]]*$/ {
      emit()
      in_skill = 1
      path = ""
      enabled = ""
      next
    }
    /^\[\[/ {
      emit()
      in_skill = 0
      path = ""
      enabled = ""
      next
    }
    in_skill && /^[[:space:]]*path[[:space:]]*=/ {
      value = $0
      sub(/^[^=]*=[[:space:]]*/, "", value)
      sub(/[[:space:]]*$/, "", value)
      if (value ~ /^".*"$/) {
        sub(/^"/, "", value)
        sub(/"$/, "", value)
      }
      path = value
      next
    }
    in_skill && /^[[:space:]]*enabled[[:space:]]*=/ {
      value = $0
      sub(/^[^=]*=[[:space:]]*/, "", value)
      sub(/[[:space:]]*$/, "", value)
      enabled = value
      next
    }
    END { emit() }
  ' "$config_path"
}

discover_skill_files() {
  local root
  for root in "${roots[@]}"; do
    find -L "$root" -mindepth 2 -maxdepth 2 -type f -name SKILL.md -print
    if [ -d "$root/.system" ]; then
      find -L "$root/.system" -mindepth 2 -maxdepth 2 -type f -name SKILL.md -print
    fi
  done | sort -u
}

read_frontmatter_field() {
  local field=$1
  local path=$2
  awk -v wanted="$field" '
    BEGIN { in_frontmatter = 0 }
    /^---[[:space:]]*$/ {
      if (!in_frontmatter) {
        in_frontmatter = 1
        next
      }
      exit
    }
    in_frontmatter {
      prefix = wanted ":"
      if (index($0, prefix) == 1) {
        value = substr($0, length(prefix) + 1)
        sub(/^[[:space:]]*/, "", value)
        print value
        exit
      }
    }
  ' "$path"
}

declare -A disabled_paths=()
declare -A seen_targets=()
declare -A first_name_target=()
issues=()

while IFS= read -r path; do
  [ -n "$path" ] && disabled_paths["$path"]=1
done < <(read_disabled_paths)

stale_disabled_paths=0
for path in "${!disabled_paths[@]}"; do
  if [ ! -e "$path" ]; then
    stale_disabled_paths=$((stale_disabled_paths + 1))
    issues+=("issue=stale_disabled_path path=$path")
  fi
done

catalog_entries=0
active_unique_skills=0
description_chars=0
user_descriptions_over_100=0
system_skills=0
global_symlink_entries=0
backup_entries=0
duplicate_skill_names=0
malformed_skill_frontmatter=0

while IFS= read -r skill_file; do
  canonical_file=$(readlink -f "$skill_file")
  if [ -n "${disabled_paths[$skill_file]+x}" ] || [ -n "${disabled_paths[$canonical_file]+x}" ]; then
    continue
  fi

  catalog_entries=$((catalog_entries + 1))
  skill_dir=${skill_file%/SKILL.md}
  if [ -L "$skill_dir" ]; then
    global_symlink_entries=$((global_symlink_entries + 1))
  fi
  case "${skill_dir##*/}" in
    *.bak.*)
      backup_entries=$((backup_entries + 1))
      issues+=("issue=backup_entry path=$skill_dir")
      ;;
  esac

  if [ -n "${seen_targets[$canonical_file]+x}" ]; then
    continue
  fi
  seen_targets["$canonical_file"]=1
  active_unique_skills=$((active_unique_skills + 1))

  is_system=0
  case "$skill_file" in
    */.system/*) is_system=1 ;;
  esac
  if [ "$is_system" -eq 1 ]; then
    system_skills=$((system_skills + 1))
  fi

  skill_name=$(read_frontmatter_field name "$canonical_file")
  description=$(read_frontmatter_field description "$canonical_file")
  if [ -z "$skill_name" ] || [ -z "$description" ]; then
    malformed_skill_frontmatter=$((malformed_skill_frontmatter + 1))
    issues+=("issue=malformed_frontmatter path=$canonical_file")
    continue
  fi
  case "$skill_name" in
    \"*\") skill_name=${skill_name#\"}; skill_name=${skill_name%\"} ;;
  esac

  if [ -n "${first_name_target[$skill_name]+x}" ] && [ "${first_name_target[$skill_name]}" != "$canonical_file" ]; then
    duplicate_skill_names=$((duplicate_skill_names + 1))
    issues+=("issue=duplicate_skill_name name=$skill_name path=$canonical_file first=${first_name_target[$skill_name]}")
  else
    first_name_target["$skill_name"]=$canonical_file
  fi

  description_length=${#description}
  description_chars=$((description_chars + description_length))
  if [ "$description_length" -gt 100 ]; then
    if [ "$is_system" -eq 1 ]; then
      issues+=("info=system_description_over_100 chars=$description_length path=$canonical_file")
    else
      user_descriptions_over_100=$((user_descriptions_over_100 + 1))
      issues+=("issue=user_description_over_100 chars=$description_length path=$canonical_file")
    fi
  fi
done < <(discover_skill_files)

printf 'catalog_entries=%s\n' "$catalog_entries"
printf 'active_unique_skills=%s\n' "$active_unique_skills"
printf 'description_chars=%s\n' "$description_chars"
printf 'user_descriptions_over_100=%s\n' "$user_descriptions_over_100"
printf 'system_skills=%s\n' "$system_skills"
printf 'global_symlink_entries=%s\n' "$global_symlink_entries"
printf 'backup_entries=%s\n' "$backup_entries"
printf 'duplicate_skill_names=%s\n' "$duplicate_skill_names"
printf 'stale_disabled_paths=%s\n' "$stale_disabled_paths"
printf 'malformed_skill_frontmatter=%s\n' "$malformed_skill_frontmatter"
printf 'max_description_chars=%s\n' "$max_description_chars"
printf 'max_active_skills=%s\n' "$max_active_skills"

status=pass
if [ "$description_chars" -gt "$max_description_chars" ] ||
   [ "$active_unique_skills" -gt "$max_active_skills" ] ||
   [ "$user_descriptions_over_100" -gt 0 ] ||
   [ "$backup_entries" -gt 0 ] ||
   [ "$duplicate_skill_names" -gt 0 ] ||
   [ "$stale_disabled_paths" -gt 0 ] ||
   [ "$malformed_skill_frontmatter" -gt 0 ]; then
  status=fail
fi
printf 'status=%s\n' "$status"

if [ "${#issues[@]}" -gt 0 ]; then
  printf '%s\n' "${issues[@]}" | sort
fi

[ "$status" = pass ]
