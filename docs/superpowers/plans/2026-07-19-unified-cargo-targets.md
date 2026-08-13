# Unified Cargo Target Directories Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Make direct Cargo builds and webClx compile/deploy API builds for every Rust workspace under /home/codes reuse the same physical target directory on /data.

**Architecture:** Each Cargo workspace keeps its normal workspace/target interface, but that path is migrated to and linked to one stable per-workspace directory below /data/cargo-target/webclx-compile/cargo-target. Existing explicit /data target configuration remains authoritative. The webClx worker stops overriding Cargo's resolved target directory, so direct and queued commands follow the same project-owned path.

**Tech Stack:** Cargo metadata/config resolution, Bash, rsync, Node.js contract/integration tests.

## Global Constraints

- Preserve every existing project-specific linker, target triple, profile, feature, and build command.
- Isolate different Cargo workspaces; do not place all binaries in one shared release directory.
- Do not migrate while Cargo/rustc or a webClx compile worker is active.
- Copy and verify each cache before removing its old files; never delete source code or non-Cargo data.
- Keep workspace/target as a compatibility symlink for scripts that still refer to target/release.
- The stable cache identity is based on the canonical Cargo workspace root, not a mutable UI project label.

---

### Task 1: Preserve Cargo-Owned Target Resolution in the Compile Worker

**Files:**
- Create: tests/compile-worker-cargo-target.test.mjs
- Modify: docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh

- [x] Add an integration test with a temporary Cargo project whose .cargo/config.toml points to a temporary /data-style target directory.
- [x] Run the test and confirm it fails because the worker injects its per-request fallback.
- [x] Remove the worker's unconditional CARGO_TARGET_DIR override, retain explicit request environment overrides, and log Cargo's resolved target directory.
- [x] Rerun the same test and existing compile worker tests; expect all to pass.

### Task 2: Add Idempotent Workspace Cache Enrollment

**Files:**
- Create: scripts/unify-cargo-targets.sh
- Create: tests/unify-cargo-targets.test.mjs

- [x] Add a fixture test covering two packages in one workspace, a pre-existing local target, and an already-enrolled rerun.
- [x] Confirm the test fails before the script exists.
- [x] Implement dry-run/apply modes, queue/process safety checks, path-hash cache reuse, rsync migration, and symlink creation.
- [x] Rerun the fixture test and bash syntax validation; expect success.

### Task 3: Enroll All Rust Workspaces Under /home/codes

- [x] Run dry-run inventory and compare workspace/target counts with independent find and cargo metadata discovery.
- [x] Confirm webClx pending count is zero and no Cargo/rustc/compiler worker is active.
- [x] Run apply mode, allowing interrupted runs to resume safely.
- [x] Rerun dry-run and confirm every workspace is already enrolled.

### Task 4: Align Deploy Wrappers and Operational Documentation

**Files:**
- Modify: .codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-and-deploy.sh
- Modify: docs/codex/tasks/compile-coordinator-reliability.md

- [x] Add or extend a contract test that rejects hard-coded obsolete compile-worker target paths.
- [x] Resolve deployment binaries through Cargo metadata while retaining binary-name fallbacks.
- [x] Document the target ownership boundary, enrollment command, and realpath verification.

### Task 5: Verify Path and Result Identity

- [x] Run cargo metadata for every workspace and verify realpath(target_directory) is below /data.
- [x] Verify every workspace/target resolves to the same realpath.
- [x] Run focused Node/shell tests for worker, migration, callback, and deploy path contracts.
- [x] Report before/after disk use, migrated workspace count, and any intentional exception.

Verification on 2026-07-19 enrolled 50 Cargo workspaces. All 50 metadata
target directories resolve below `/data`, all 50 compatibility `target` paths
resolve to the same physical directory, and no physical `target` directory
remains below `/home/codes`. `/home` filesystem use fell from approximately
373 GB to 213 GB (about 160 GB released); the general unified cache uses
184 GB and stockScreener's intentional explicit `/data/cargo-target/stockScreener`
cache uses 19 GB. Hidden webClx deploy/worktree checkouts are intentionally
excluded because they are disposable source worktrees and contain no Cargo
target directory.
