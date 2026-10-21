# Windows Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add native Windows compatibility while keeping Linux behavior unchanged.

**Architecture:** Introduce small target-aware platform helpers in existing modules rather than broad rewrites. Linux keeps the `tmux` and `systemd` path; Windows uses env-derived user information, user-home workspace defaults, PowerShell PTY sessions, and graceful system API degradation.

**Tech Stack:** Rust 2024, `portable-pty`, existing Axum/Tokio workspace.

---

### Task 1: Platform Defaults And User Fallback

**Files:**
- Modify: `crates/runtime_paths_core/src/lib.rs`
- Modify: `crates/settings_core/src/lib.rs`
- Test: `crates/runtime_paths_core/src/lib.rs`
- Test: `crates/settings_core/src/tests.rs`

- [ ] Add tests for env-derived non-Unix profile data and platform workspace root helpers.
- [ ] Implement non-Unix current-user fallback from environment variables.
- [ ] Replace hardcoded `/home` workspace limit with platform-specific helper functions.
- [ ] Run `cargo test -p runtime_paths_core -p settings_core`.

### Task 2: Root Binary Platform Guards

**Files:**
- Modify: `src/system.rs`
- Modify: `src/host.rs`
- Modify: `src/startup_tools.rs`

- [ ] Guard Unix-only `libc` calls behind `cfg(unix)`.
- [ ] Return `0` for uid/gid on Windows system info.
- [ ] Resolve hostname from env on Windows before falling back to a generic value.
- [ ] Skip startup tool bootstrap on Windows.
- [ ] Run `cargo check --workspace`.

### Task 3: Windows Terminal Backend

**Files:**
- Modify: `src/terminal/session.rs`
- Modify: `src/terminal/manager.rs`
- Modify: `src/terminal.rs`

- [ ] Add a Windows attach path that spawns `powershell.exe` directly with `portable-pty`.
- [ ] Skip tmux creation/status/kill operations on Windows.
- [ ] Keep Linux tmux behavior unchanged.
- [ ] Run `cargo test --workspace`.

### Task 4: Documentation And Verification

**Files:**
- Modify: `docs/codex/tasks/windows-compatibility.md`

- [ ] Document the implemented first-pass behavior and limitations.
- [ ] Run host checks and, if the Windows target is not installed, record that limitation in the final response.
