# API Preset Apply Proxy Flag Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-preset Codex_API option that decides whether applying that preset also switches Codex to the webClx local upstream proxy.

**Architecture:** Persist one boolean on each API preset, expose it through save/list/update responses, and make `apply_api_preset()` derive the effective proxy mode from that field plus the existing hard-required proxy heuristics for GLM, DeepSeek, and MiniMax. The frontend editor owns the checkbox and sends it with saves; the preset table shows the saved choice so switching behavior is visible before clicking.

**Tech Stack:** Rust backend, `auth_core` models/tests, static HTML, vanilla JavaScript

---

### Task 1: Lock Model And Compatibility

**Files:**
- Modify: `crates/auth_core/src/models.rs`
- Modify: `crates/auth_core/src/tests.rs`

- [ ] **Step 1: Add a saved boolean to API preset structs and request/summary payloads**

```rust
#[serde(default)]
pub apply_upstream_proxy_on_switch: bool,
```

- [ ] **Step 2: Add backward-compatibility tests**

```rust
assert!(!preset.apply_upstream_proxy_on_switch);
```

- [ ] **Step 3: Add a focused test for the effective apply-time proxy rule**

```rust
assert!(api_preset_enables_local_upstream_proxy_on_apply(&preset));
```

### Task 2: Make Backend Apply Logic Use The Preset Flag

**Files:**
- Modify: `crates/auth_core/src/lib.rs`
- Modify: `src/auth.rs`
- Modify: `src/auth/apply.rs`

- [ ] **Step 1: Add one helper for the effective apply-time proxy decision**

```rust
pub fn api_preset_enables_local_upstream_proxy_on_apply(preset: &StoredApiPreset) -> bool {
    preset.apply_upstream_proxy_on_switch || api_preset_prefers_local_upstream_proxy(preset)
}
```

- [ ] **Step 2: Save and update the new field in API preset create/update handlers**

- [ ] **Step 3: Replace raw `codex_api_proxy_enabled` checks in API apply/update paths with the effective preset decision when writing auth/config and persisting upstream state**

### Task 3: Wire The Codex_API Editor

**Files:**
- Modify: `static/index.html`
- Modify: `static/app.js`

- [ ] **Step 1: Add a checkbox to the Codex_API editor**

```html
<label class="toggle-row inline-toggle-row">
  <input id="api-apply-upstream-proxy-on-switch" type="checkbox" />
  <span class="toggle-note">切换到该预设时同步启用“通过 webClx 本机代理转发”</span>
</label>
```

- [ ] **Step 2: Include the field in form reset, edit snapshot, edit preload, and save payload**

- [ ] **Step 3: Show the saved choice in the API preset table**

### Task 4: Verify, Deploy, And Document

**Files:**
- Modify: `docs/codex/index.md`
- Modify: `/home/bin/webclx/static/index.html`
- Modify: `/home/bin/webclx/static/app.js`

- [ ] **Step 1: Run focused tests and static validation**

Run: `cargo test -p auth_core api_preset_enables_local_upstream_proxy_on_apply old_api_preset_format_is_backward_compatible -- --nocapture`
Expected: exit `0`

Run: `cargo test save_api_preset update_api_preset apply_api_preset -- --nocapture`
Expected: exit `0` or no matching tests if only unit coverage exists outside the binary crate

Run: `node --check static/app.js`
Expected: exit `0`

Run: `git diff --check`
Expected: no output

- [ ] **Step 2: Sync deployed static assets**

Run: `install -m 0644 static/index.html /home/bin/webclx/static/index.html`
Expected: exit `0`

Run: `install -m 0644 static/app.js /home/bin/webclx/static/app.js`
Expected: exit `0`

- [ ] **Step 3: Update durable context**

Document that Codex_API presets now carry their own apply-time local-proxy preference, while GLM/DeepSeek/MiniMax still force proxy mode even if the saved checkbox is off.
