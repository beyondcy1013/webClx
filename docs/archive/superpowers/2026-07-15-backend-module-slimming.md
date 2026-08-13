# Backend Module Slimming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Reduce the backend's largest proven responsibility clusters without changing HTTP routes, preset routing semantics, or public handler interfaces.

**Architecture:** Move route registration out of `main.rs` into small domain route modules composed by `routes::app`. Keep the upstream proxy handlers and credential/routing decisions in `upstream_proxy.rs`, while moving only pure Anthropic/OpenAI payload transforms into a private child module. Replace the five production `expect`/`unwrap` candidates with typed error paths or explicit invariant branches.

**Tech Stack:** Rust 2024, Axum 0.8, Node.js source-contract tests, existing Cargo test suite.

## Global Constraints

- Preserve all 144 route registrations and 140 unique route paths.
- Preserve direct, relay, conversion, scoped-token, dynamic-token, and client-credential routing behavior.
- Do not move or commit existing user changes in `Cargo.toml`, `Cargo.lock`, `src/filesystem.rs`, `src/terminal.rs`, or `src/terminal/docs.rs`.
- Do not introduce a new error-handling dependency.
- Queue Rust compilation through the documented webClx compile API.

---

### Task 1: Add Architecture Regression Coverage

**Files:**
- Create: `tests/backend-architecture.test.mjs`

**Interfaces:**
- Consumes: Rust source layout and route string literals.
- Produces: a source-level contract for the route set, route ownership, and upstream transform seam.

- [x] **Step 1: Add a Node test for the current route set**

Read `src/routes/**/*.rs`, extract `.route("...")` registrations, and require 144 registrations with the captured 140-path set.

- [x] **Step 2: Add architecture assertions**

Require `src/main.rs` to contain no `.route(` calls, require the domain route files, and require `upstream_proxy.rs` to declare a private `transform` module instead of defining the pure conversion functions itself.

- [x] **Step 3: Run RED**

Run `node --test tests/backend-architecture.test.mjs` and require failure because `src/routes` and `src/upstream_proxy/transform.rs` do not exist yet.

- [x] **Step 4: Commit RED**

Commit only this plan and the new test with `test: 固定后端模块拆分边界`.

### Task 2: Split Route Registration By Domain

**Files:**
- Create: `src/routes/mod.rs`
- Create: `src/routes/pages.rs`
- Create: `src/routes/artifacts.rs`
- Create: `src/routes/workspace.rs`
- Create: `src/routes/operations.rs`
- Create: `src/routes/gateway.rs`
- Create: `src/routes/frp.rs`
- Create: `src/routes/system.rs`
- Create: `src/routes/auth.rs`
- Create: `src/routes/terminal.rs`
- Create: `src/routes/agent.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: existing handler functions and `AppState`.
- Produces: `routes::app(state: AppState) -> Router`.

- [x] **Step 1: Move route chains without changing paths or methods**

Each domain file returns `Router<AppState>`. `routes::app` merges protected domains, applies the existing fallback and auth middleware, merges public routes, and applies compression.

- [x] **Step 2: Reduce main to initialization and composition**

Declare `mod routes`, remove route-only imports, and replace the monolithic chains with `let app = routes::app(state.clone());`.

- [x] **Step 3: Run the route contract**

Run `node --test tests/backend-architecture.test.mjs`; only the upstream-transform assertion may remain RED.

### Task 3: Extract Pure Upstream Protocol Transforms

**Files:**
- Create: `src/upstream_proxy/transform.rs`
- Create: `src/upstream_proxy/tests.rs`
- Modify: `src/upstream_proxy.rs`

**Interfaces:**
- Consumes: `serde_json::Value` payloads and `AppError` for invalid requests.
- Produces: private transform functions re-imported only by the parent proxy implementation and its tests.

- [x] **Step 1: Move pure conversion functions**

Move Anthropic request-to-OpenAI request conversion, OpenAI response-to-Anthropic response conversion, and their pure helpers. Keep network forwarding, credential selection, preset selection, and route decisions in the parent.

- [x] **Step 2: Separate the test module**

Move the inline `#[cfg(test)]` module to `src/upstream_proxy/tests.rs`, following the existing terminal/auth test layout.

- [x] **Step 3: Preserve private visibility**

Use `pub(super)` only for functions called by the parent; do not re-export them from `upstream_proxy`.

- [x] **Step 4: Run the architecture contract GREEN**

Run `node --test tests/backend-architecture.test.mjs` and require all assertions to pass.

### Task 4: Remove Production Panic Paths

**Files:**
- Modify: `src/proxy.rs`
- Modify: `src/frpc.rs`
- Modify: `src/agent.rs`

**Interfaces:**
- Consumes: existing `ApiResult`/`anyhow::Result` error contracts.
- Produces: error returns for proxy construction and invalid FRP role state; explicit `Option` handling for absent tool calls.

- [x] **Step 1: Replace proxy construction expects**

Map `reqwest::Proxy` and client-builder failures into the existing proxy test error response.

- [x] **Step 2: Replace FRP validated-state expects**

Return a validation error if a role type and nested config disagree instead of panicking.

- [x] **Step 3: Replace the agent tool-call unwrap**

Process tool calls only through an explicit `if let Some(tool_calls)` branch.

### Task 5: Verify, Document, And Commit

**Files:**
- Modify: `docs/codex/tasks/project-module-slimming.md`
- Move: this plan to `docs/archive/superpowers/` after completion.

**Interfaces:**
- Consumes: the new internal module layout.
- Produces: current module map and fresh verification evidence.

- [x] **Step 1: Run local non-build checks**

Run the Node architecture test, relevant existing Node tests, `cargo fmt --check`, `git diff --check`, and a production `unwrap/expect` recount.

- [x] **Step 2: Queue Rust verification**

Use the documented webClx compile API for `cargo check` and relevant tests; wait for its callback before claiming compilation success.

- [x] **Step 3: Update the existing slimming topic**

Record the route composition seam, upstream transform seam, and the rule that panic counts must separate test-only calls from production calls.

- [x] **Step 4: Commit GREEN**

Commit only files owned by this task with a Chinese Conventional Commit message, leaving pre-existing staged and untracked work untouched.
