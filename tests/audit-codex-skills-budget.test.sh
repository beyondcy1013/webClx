#!/usr/bin/env bash
set -u

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
audit_script="$script_dir/scripts/audit-codex-skills-budget.sh"
fixture_root=$(mktemp -d)
trap 'rm -rf "$fixture_root"' EXIT

write_skill() {
  local path=$1
  local name=$2
  local description=$3
  mkdir -p "$(dirname "$path")"
  printf '%s\n' '---' "name: $name" "description: $description" '---' > "$path"
}

long_description='Use when this fixture needs a deliberately long description that should be reported by the budget audit because it exceeds the one hundred character user-managed description limit.'
mkdir -p "$fixture_root/noisy/normal" "$fixture_root/noisy/disabled" "$fixture_root/noisy/old.bak.2026" "$fixture_root/noisy/duplicate-a" "$fixture_root/noisy/duplicate-b" "$fixture_root/noisy/target"
write_skill "$fixture_root/noisy/normal/SKILL.md" normal 'Use when checking a normal fixture skill.'
write_skill "$fixture_root/noisy/disabled/SKILL.md" disabled 'Use when checking a disabled fixture skill.'
write_skill "$fixture_root/noisy/old.bak.2026/SKILL.md" backup 'Use when checking a backup fixture skill.'
write_skill "$fixture_root/noisy/duplicate-a/SKILL.md" duplicate 'Use when checking duplicate fixture skill A.'
write_skill "$fixture_root/noisy/duplicate-b/SKILL.md" duplicate 'Use when checking duplicate fixture skill B.'
write_skill "$fixture_root/noisy/target/SKILL.md" long-skill "$long_description"
ln -s target "$fixture_root/noisy/link-to-target"
cat > "$fixture_root/noisy-config.toml" <<EOF
[[skills.config]]
path = "$fixture_root/noisy/disabled/SKILL.md"
enabled = false

[[skills.config]]
path = "$fixture_root/noisy/missing/SKILL.md"
enabled = false
EOF

if [ ! -x "$audit_script" ]; then
  printf 'expected RED: audit script is missing\n' >&2
  exit 1
fi

noisy_output=$(
  "$audit_script" \
    --config "$fixture_root/noisy-config.toml" \
    --root "$fixture_root/noisy" \
    --max-description-chars 1000 \
    --max-active-skills 20
)
noisy_status=$?
if [ "$noisy_status" -ne 1 ]; then
  printf 'expected noisy fixture exit 1, got %s\n%s\n' "$noisy_status" "$noisy_output" >&2
  exit 1
fi
printf '%s\n' "$noisy_output" | grep -Fx 'stale_disabled_paths=1' >/dev/null || {
  printf 'missing stale path count\n%s\n' "$noisy_output" >&2
  exit 1
}
printf '%s\n' "$noisy_output" | grep -Fx 'backup_entries=1' >/dev/null || {
  printf 'missing backup count\n%s\n' "$noisy_output" >&2
  exit 1
}
printf '%s\n' "$noisy_output" | grep -Fx 'duplicate_skill_names=1' >/dev/null || {
  printf 'missing duplicate name count\n%s\n' "$noisy_output" >&2
  exit 1
}
printf '%s\n' "$noisy_output" | grep -Fx 'user_descriptions_over_100=1' >/dev/null || {
  printf 'missing overlength description count\n%s\n' "$noisy_output" >&2
  exit 1
}
printf '%s\n' "$noisy_output" | grep -Fx 'global_symlink_entries=1' >/dev/null || {
  printf 'missing symlink count\n%s\n' "$noisy_output" >&2
  exit 1
}

mkdir -p "$fixture_root/clean/project"
write_skill "$fixture_root/clean/project/SKILL.md" clean 'Use when checking a clean fixture skill.'
write_skill "$fixture_root/clean/disabled/SKILL.md" clean-disabled 'Use when checking a clean disabled fixture skill.'
cat > "$fixture_root/clean-config.toml" <<EOF
[[skills.config]]
path = "$fixture_root/clean/disabled/SKILL.md"
enabled = false
EOF

clean_output=$(
  "$audit_script" \
    --config "$fixture_root/clean-config.toml" \
    --root "$fixture_root/clean" \
    --max-description-chars 100 \
    --max-active-skills 2
)
clean_status=$?
if [ "$clean_status" -ne 0 ]; then
  printf 'expected clean fixture exit 0, got %s\n%s\n' "$clean_status" "$clean_output" >&2
  exit 1
fi
printf '%s\n' "$clean_output" | grep -Fx 'status=pass' >/dev/null || {
  printf 'missing pass status\n%s\n' "$clean_output" >&2
  exit 1
}

printf 'audit-codex-skills-budget tests passed\n'
