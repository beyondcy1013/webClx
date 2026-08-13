# Terminal Fast Switch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make active-terminal dropdown switching feel fast under concurrent terminal activity without blocking or bypassing other project compilations.

**Architecture:** Keep the existing one-WebSocket/one-xterm frontend model, but remove external tmux/process probes from the terminal manager's global state lock. Bound the initial tmux snapshot to 800 recent lines so xterm does not synchronously render thousands of historical lines on every switch; the full tmux history remains available to backend search and later captures.

**Tech Stack:** Rust, Tokio/Axum WebSocket terminal manager, tmux, xterm.js, Node regression tests, Playwright browser timing probe.

## Global Constraints

- Work in the shared dirty tree without overwriting or staging unrelated changes.
- Do not use subagents or Ultra mode.
- Do not redesign API preset routing, terminal identity, or session persistence semantics.
- Do not run local builds, installs, deploys, or restarts; queue webClx compilation/deployment through the running webClx API.
- Preserve complete tmux history and terminal search; only the initial switch replay is bounded.
- Commit only files and hunks created for this optimization.

---

### Task 1: Lock-boundary regression

**Files:**
- Modify: `src/terminal/manager.rs`
- Modify: `src/terminal/tests.rs`

**Interfaces:**
- Consumes: existing `TerminalState`, `TerminalSession`, `TerminalAgentDetector`, and terminal error/status helpers.
- Produces: `TerminalActivityProbe`, `collect_session_activity_probes`, and `collect_session_infos_from_probes_locked`; all shell/tmux/process inspection completes before the manager write lock is reacquired.

- [ ] **Step 1: Add a regression test for applying precomputed activity probes**

Create a test that constructs a stored/live terminal, supplies a precomputed idle probe, applies it through `collect_session_infos_from_probes_locked`, and verifies title/name/activity merging remains correct without invoking tmux from inside the apply function.

- [ ] **Step 2: Run the narrow test and confirm it fails before implementation**

Run: `cargo test terminal::tests::collect_session_infos_applies_precomputed_activity_probe -- --exact`

Expected: compilation failure because `collect_session_infos_from_probes_locked` and the probe constructor do not yet exist.

- [ ] **Step 3: Split probing from state mutation**

Implement this shape in `src/terminal/manager.rs`:

```rust
struct TerminalActivityProbe {
    session_id: String,
    live_last_output_at: u64,
    snapshot_fingerprint: Option<u64>,
    agent_activity: TerminalAgentActivity,
    working_status: bool,
    error_match: Option<TerminalErrorKeywordMatch>,
    worked_status: bool,
}
```

Build probes from a short read-lock snapshot of live sessions, release the lock, then run `/proc`, tmux snapshot, working/error/worked detection. Reacquire the write lock only to update output observations, names/titles, and response DTOs.

- [ ] **Step 4: Route all three production scans through the unlocked probe path**

Update `list_sessions`, `list_all_sessions`, and `scan_error_auto_continue_sessions` so cleanup/sorting and result application use short lock sections while `collect_session_activity_probes` runs without the manager lock.

- [ ] **Step 5: Move viewed-state fingerprint capture outside the write lock**

Read the live timestamp under a short read lock, capture the recent tmux fingerprint without the lock, then reacquire the write lock to merge the observation.

- [ ] **Step 6: Run manager tests**

Run: `cargo test terminal::tests::collect_session_infos_ -- --nocapture`

Expected: all matching tests pass.

### Task 2: Bounded initial replay

**Files:**
- Modify: `src/terminal/tmux.rs`
- Create: `tests/terminal-switch-performance-contract.test.mjs`

**Interfaces:**
- Consumes: `capture_tmux_pane_snapshot_from` and the initial WebSocket backlog replay path.
- Produces: an 800-line initial snapshot contract; full text snapshots used by search/error detection remain unchanged.

- [ ] **Step 1: Add a failing static contract test**

Assert that `INITIAL_TMUX_SNAPSHOT_LINE_LIMIT` equals `800`, while `capture_tmux_text_pane_snapshot` still uses `-S -` for complete history.

- [ ] **Step 2: Run the contract and confirm it fails**

Run: `node tests/terminal-switch-performance-contract.test.mjs`

Expected: failure because the current initial limit is 2000.

- [ ] **Step 3: Change only the initial snapshot limit**

Set:

```rust
const INITIAL_TMUX_SNAPSHOT_LINE_LIMIT: u32 = 800;
```

Do not change `TMUX_HISTORY_LIMIT`, full-text search capture, error detection capture, or live output backlog recovery.

- [ ] **Step 4: Run the contract again**

Run: `node tests/terminal-switch-performance-contract.test.mjs`

Expected: pass.

### Task 3: Verification, documentation, and safe delivery

**Files:**
- Modify: `docs/codex/tasks/terminal-session-switch-output.md`
- Modify: `docs/superpowers/plans/2026-07-15-terminal-fast-switch.md`

**Interfaces:**
- Consumes: the browser timing probe from diagnosis and the webClx compile/deploy queue.
- Produces: measured before/after timings, regression commands, one scoped commit, and a queued deployment callback.

- [ ] **Step 1: Run focused source tests**

Run:

```bash
node tests/terminal-switch-performance-contract.test.mjs
node tests/terminal-session-switch-output.test.mjs
cargo test terminal::tests::
```

Expected: all pass with zero failures.

- [ ] **Step 2: Record the reusable lock and replay conclusions**

Append a short dated section to `docs/codex/tasks/terminal-session-switch-output.md` describing the two bottlenecks, changed lock boundary, 800-line initial replay, and verification commands.

- [ ] **Step 3: Re-check concurrent-edit boundaries**

Compare target-file hashes and `git diff --name-only`; inspect every task diff and stage only this plan, manager/tmux/test/doc changes.

- [ ] **Step 4: Commit the scoped fix**

Commit message:

```text
fix: 加快活动终端切换

现象：活动终端切换需要等待数秒。
根因：会话状态扫描在全局写锁内执行外部探针，且每次回放 2000 行历史。
修复：锁外完成探针并限制首屏回放为 800 行，保留完整后台历史。
```

- [ ] **Step 5: Queue compile and deploy without competing with other builds**

Run the webClx API deploy wrapper with `scripts/rebuild-and-deploy.sh`. If it returns `queued: true`, stop dependent work and wait for the source-terminal callback; do not poll logs or invoke local Cargo/systemctl commands.

- [ ] **Step 6: Verify the deployed runtime after callback**

Repeat the Playwright switch timing probe against `http://127.0.0.1:11111/terminal`, confirm source/deployed static copies are aligned, and report actual timings rather than an inferred improvement.
