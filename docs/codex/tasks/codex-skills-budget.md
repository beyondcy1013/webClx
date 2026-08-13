# Codex Skills 2% Context Budget

## Symptom

Codex shows a startup notice such as:

> Skill descriptions were shortened to fit the 2% skills context budget. Codex can still see every skill, but some descriptions are shorter.

The full system prompt auto-truncates each `description:` line in `SKILL.md` frontmatter, but the truncated version still does not always fit. Long descriptions in active skills drive the warning.

## Scope

- Skill roots: `r0 = /home/root/.codex/skills`, `r1 = /home/root/.codex/skills/.system`.
- The disabled-skills allowlist already lives in `~/.codex/config.toml` under `[[skills.config]] enabled = false`. Add paths there to drop a skill entirely.
- Trimming the `description:` field in the `SKILL.md` frontmatter is the second lever; it preserves the skill and its trigger info while shrinking the system prompt's footprint.

## Verified Approach (2026-06-02)

1. Walked the active skill list (the entries shown in the Codex system prompt "Available skills" section).
2. Parsed the YAML frontmatter of each `SKILL.md` and replaced `description:` with a trimmed version (target <= 110 chars, preserving any `Use when ...` clause when present).
3. Special name/path mappings that differ from the directory name:
   - `design-taste-frontend` -> `/home/root/.codex/skills/taste-skill/SKILL.md`
   - `executing-plans` / `systematic-debugging` / `test-driven-development` / `using-superpowers` / `verification-before-completion` / `writing-plans` -> `superpowers-*` directories.
4. Result for the 77 Codex skills visible in the system prompt: total description length dropped from ~14k chars to ~6.5k chars, well under the 2% of a 200k context window (16k chars / 4k tokens).

## Verified Cleanup (2026-07-22)

The warning returned after the global catalog grew through new skills, project-skill symlinks, and backup entries.

Baseline from `webClx`:

- 149 active catalog entries and 146 canonical targets.
- 25,195 raw description characters, including six system-managed skills.
- 94 user-managed descriptions over 100 characters.
- 41 active global symlink entries, three `.bak.*` entries, four duplicate names, and 56 stale disabled paths.

Final verified state:

- 122 active catalog entries and 120 canonical targets.
- 11,623 raw description characters and zero user-managed descriptions over 100 characters.
- 17 active global symlinks, zero scanned backups, zero duplicate names, zero stale disabled paths, and zero malformed frontmatter.
- Fresh ephemeral `codex exec` startup returned successfully with no skills-budget warning.
- Equivalent audits passed from `/home/codes/stockScreener` (125 skills, 12,108 description characters) and `/home/codes/kidsAI` (117 skills, 11,372 description characters).

Use the checked audit instead of ad hoc counts:

```bash
bash tests/audit-codex-skills-budget.test.sh
bash scripts/audit-codex-skills-budget.sh \
  --config /home/root/.codex/config.toml \
  --root /home/root/.codex/skills \
  --root /home/codes/webClx/.codex/skills \
  --max-description-chars 13000 \
  --max-active-skills 120
```

The reversible archive is `/home/root/.codex/skills-archive/2026-07-22/`. It contains the original descriptions, moved backup entries, archived project aliases, sync-generated copies, and path manifests. The pre-cleanup config backup is `/home/root/.codex/config.toml.skills-budget-20260722.bak`.

Current policy:

1. Keep user-managed `description:` values at no more than 100 characters and preserve distinctive `Use when ...` triggers.
2. Leave `.system` descriptions untouched; count them in the total budget.
3. Keep project-specific skills under the owning project's `.codex/skills`. Add a global symlink only when the skill must route requests from unrelated working directories.
4. Keep active descriptions below 13,000 characters and retain at least modest headroom rather than targeting the exact warning boundary.
5. Run the audit after adding or globally linking a skill.

## Cautions

- Trimming loses trigger words; when adding a brand new skill, keep the description at or below 100 chars from the start.
- `superpowers-*` skills use unquoted YAML `description:` lines; the script handled both quoted and unquoted forms.
- The `/home/beyondcy/.claude/skills/` root is also scanned by Codex but is not part of the displayed "Available skills" list; trimming it is optional.
- The blacklist in `~/.codex/config.toml` is the only reliable way to remove a skill from the system prompt entirely. Trimming is cheaper than blacklisting if the skill is still useful.
- `skill-bidirectional-sync` atomically replaces targets and can turn a Codex skill-directory symlink into a copied directory while leaving the original symlink as `.bak.<timestamp>`. After syncing a globally linked project skill, restore the canonical symlink and move the generated copy/backup outside `/home/root/.codex/skills` before auditing.
