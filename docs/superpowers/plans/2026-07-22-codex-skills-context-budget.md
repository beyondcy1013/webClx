# Codex Skills Context Budget Optimization Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the Codex startup warning about shortened skill descriptions while preserving automatic routing for core and project-relevant skills.

**Architecture:** Treat skill discovery as a catalog with four inputs: system skills, user-global skills, global symlinks to project skills, and current-project skills. First make the catalog measurable, then remove duplicate/backup exposure, shorten only user-managed descriptions, and reduce global aliases only if the measured budget still lacks headroom.

**Tech Stack:** Bash, TOML configuration, YAML frontmatter, existing Codex skill discovery, shell fixture tests.

## Global Constraints

- Preserve `/home/root/.codex/skills/.system`; system-managed skill files are read-only for this work.
- Never delete a project skill target. Remove or archive only redundant global aliases and backup directories.
- Do not print or copy unrelated values from `/home/root/.codex/config.toml`; it contains settings outside the skills scope.
- Keep every enabled user-managed `description:` on one line and at no more than 100 characters, preserving the decisive `Use when ...` trigger words.
- Target at most 120 active unique skills and at most 13,000 raw description characters, leaving headroom below the 2% catalog budget.
- Apply changes in two gates: description/deduplication first, global-scope reduction only if a fresh Codex session still warns or the audit target is exceeded.
- When a user skill under `/home/root/.codex/skills` or `/home/root/.claude/skills` changes, use `skill-bidirectional-sync` with explicit per-skill authority before completion.
- Preserve unrelated dirty-worktree changes. This plan requires no webClx compile or deployment.

---

### Task 1: Add A Reproducible Skill Catalog Audit

**Files:**
- Create: `scripts/audit-codex-skills-budget.sh`
- Create: `tests/audit-codex-skills-budget.test.sh`

**Interfaces:**
- Consumes: `--config`, repeatable `--root`, and optional `--max-description-chars` / `--max-active-skills` arguments.
- Produces: counts for catalog entries, canonical targets, enabled targets, description characters, user-managed descriptions over 100 characters, duplicate names, backup entries, global symlinks, system-managed entries, and stale disabled paths.
- Exit contract: `0` when limits pass, `1` when a budget or duplicate check fails, and `2` for invalid arguments or unreadable inputs.

- [x] **Step 1: Write fixture-based failing tests**

  Build a temporary fixture containing one normal skill, one disabled skill, one symlinked project skill, one `.bak.*` skill, one duplicate name, and one missing disabled path. Assert canonical-path deduplication, TOML `[[skills.config]] enabled = false` handling, description character totals, and the three exit codes without reading the real home configuration.

- [x] **Step 2: Run the test to verify RED**

  Run: `bash tests/audit-codex-skills-budget.test.sh`

  Expected: failure because `scripts/audit-codex-skills-budget.sh` does not exist.

- [x] **Step 3: Implement the audit script**

  Enumerate immediate skill directories plus `/home/root/.codex/skills/.system/*/SKILL.md`, follow directory symlinks with `readlink -f`, parse only frontmatter `name:` and `description:` plus `[[skills.config]]` path/enabled pairs, and never echo other TOML keys or values. Count system description characters in the total, but report their individual overlength rows as immutable information rather than failures. Emit stable `key=value` summary lines followed by sorted issue rows so two runs can be diffed.

- [x] **Step 4: Run the fixture and live audits**

  Run:

  ```bash
  bash tests/audit-codex-skills-budget.test.sh
  bash scripts/audit-codex-skills-budget.sh \
    --config /home/root/.codex/config.toml \
    --root /home/root/.codex/skills \
    --root /home/codes/webClx/.codex/skills \
    --max-description-chars 13000 \
    --max-active-skills 120
  ```

  Expected on-disk baseline before cleanup: approximately 149 active catalog entries, 146 canonical targets, and 25,195 description characters when all six system-managed skill files are included. The non-system subset contains 140 canonical targets, 22,975 description characters, and 94 descriptions over 100 characters. The command should exit `1` because both targets are exceeded.

### Task 2: Remove Backup And Duplicate Catalog Exposure

**Files:**
- Move: `/home/root/.codex/skills/browser-qa.bak.20260719102854`
- Move: `/home/root/.codex/skills/rustcommander-deploy.bak.20260714015602`
- Move: `/home/root/.codex/skills/webclx-compile-and-deploy.bak.20260722210654`
- Create outside discovery roots: `/home/root/.codex/skills-archive/2026-07-22/`
- Modify: `/home/root/.codex/config.toml`

**Interfaces:**
- Consumes: Task 1 issue rows for backup entries, duplicate canonical targets, and missing disabled paths.
- Produces: one discoverable entry per logical skill and a skills blacklist containing only paths that still exist.

- [x] **Step 1: Record the exact pre-change state**

  Save path, symlink target, file type, and content hash for the three listed backup entries in `/home/root/.codex/skills-archive/2026-07-22/MANIFEST.txt`. Record the current count of stale `[[skills.config]]` paths; the observed baseline is 56.

- [x] **Step 2: Archive the three backup entries**

  Move only the three exact paths listed above into `/home/root/.codex/skills-archive/2026-07-22/`. Do not follow or move their symlink targets. Confirm the target of each archived symlink still exists.

- [x] **Step 3: Prune stale blacklist blocks**

  Use `apply_patch` to remove only `[[skills.config]]` blocks whose `path` no longer exists and whose `enabled` value is `false`. Leave every existing-path block and every non-skill setting byte-for-byte unchanged.

- [x] **Step 4: Re-run the audit**

  Expected: no `.bak.*` entries, no duplicate canonical target caused by those aliases, and zero stale disabled paths. Description and active-skill limits may still fail at this gate.

### Task 3: Shorten User-Managed Skill Descriptions

**Files:**
- Modify: active non-system `SKILL.md` files reported by Task 1 with descriptions over 100 characters
- Modify after verification: `docs/codex/tasks/codex-skills-budget.md`

**Interfaces:**
- Consumes: the audit's sorted overlength list.
- Produces: one-line descriptions of at most 100 characters that retain project name, user intent, and distinctive trigger terms.

- [x] **Step 1: Generate and review the exact edit manifest**

  Split reported skills into global, global-symlink target, and webClx-local groups. Exclude every path under `/home/root/.codex/skills/.system`. For each description, mark the trigger phrase that must survive before editing.

- [x] **Step 2: Rewrite descriptions in small batches**

  Use `apply_patch` and keep each description in this shape:

  ```yaml
  description: Use when <specific user intent>; handles <distinctive domain keywords>.
  ```

  Remove implementation inventories, host details, guarantees, and examples from frontmatter; retain those details in the skill body. Audit after each batch of 10 skills so an incorrect YAML edit is caught locally.

- [x] **Step 3: Synchronize every modified user skill**

  For each edited skill, run `skill-bidirectional-sync` status and dry-run, select the just-edited Codex copy as authoritative, then sync only that skill. Do not enable the watcher service and do not bulk-copy unrelated skill directories.

- [x] **Step 4: Enforce the first budget gate**

  Re-run the audit. Expected: zero user-managed descriptions over 100 characters and no malformed/missing frontmatter. If active descriptions are at or below 13,000 characters and active unique skills are at or below 120, skip Task 4 and proceed to Task 5.

### Task 4: Reduce Global Project Aliases Only If Needed

**Files:**
- Move only selected symlinks from: `/home/root/.codex/skills/`
- Extend: `/home/root/.codex/skills-archive/2026-07-22/MANIFEST.txt`

**Interfaces:**
- Consumes: remaining over-budget catalog rows after Task 3.
- Produces: project-specific skills discoverable from their owning project but absent from unrelated project sessions.

- [x] **Step 1: Classify remaining global symlinks**

  Keep cross-project routing/operations aliases: `baidusyncdisk-remote-qt-build`, `connect-xiaoshuai-chrome-devtools`, `lyystock-project-locator`, `obsidian-wine`, `stalwart-mail-ops`, `sync-server-xiaoshuai`, `sync-极空间`, `terminal-message`, `webclx-artifact-publisher`, `windows-qt-stock-path-map`, and `zcode`.

  Rank all other symlinks by whether their target already lives under an owning project's `.codex/skills` directory. Select the lowest-use project-only aliases until both audit targets pass; never move the target directory.

- [x] **Step 2: Archive selected aliases reversibly**

  Append each selected alias, canonical target, and hash to `MANIFEST.txt`, then move the alias to `/home/root/.codex/skills-archive/2026-07-22/`. Verify that opening Codex from the owning project still discovers the canonical project-local skill.

- [x] **Step 3: Stop at the target**

  Re-run the audit after each alias batch. Stop as soon as active unique skills are at most 120 and descriptions are at most 13,000 characters; do not remove additional convenience aliases merely to minimize the count.

### Task 5: Verify Fresh-Session Behavior And Document The Policy

**Files:**
- Modify: `docs/codex/tasks/codex-skills-budget.md`

**Interfaces:**
- Consumes: final audit output and fresh Codex startup behavior.
- Produces: verified no-warning startup, trigger smoke-test evidence, final counts, and rollback instructions.

- [x] **Step 1: Start a fresh Codex session**

  Skill catalogs are assembled at startup, so do not validate only in the session that made the edits. Open a new session in `/home/codes/webClx` and assert the warning `Skill descriptions were shortened to fit the 2% skills context budget` is absent.

- [x] **Step 2: Smoke-test routing**

  Confirm that representative prompts still select `webclx-compile-and-deploy`, `webclx-artifact-publisher`, `host-aliases`, `browser-qa`, `context-budget`, and `skill-bidirectional-sync`. Open fresh sessions in `/home/codes/stockScreener` and `/home/codes/kidsAI` and confirm their project-local skills remain discoverable even if their global aliases were archived.

- [x] **Step 3: Run final checks**

  Run:

  ```bash
  bash tests/audit-codex-skills-budget.test.sh
  bash scripts/audit-codex-skills-budget.sh \
    --config /home/root/.codex/config.toml \
    --root /home/root/.codex/skills \
    --root /home/codes/webClx/.codex/skills \
    --max-description-chars 13000 \
    --max-active-skills 120
  git diff --check
  ```

  Expected: both commands exit `0`, there are no duplicate/backup catalog entries, and only intended repository files appear in the path-limited diff.

- [x] **Step 4: Update durable documentation**

  Record the final counts, the 100-character description policy, the archive location, the exact rollback method from `MANIFEST.txt`, and the rule that project-specific skills should remain project-local unless they are intentionally cross-project operators.
