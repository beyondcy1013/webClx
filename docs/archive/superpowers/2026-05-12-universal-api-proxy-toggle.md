# Universal API Proxy Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add independent Codex_API and Claude Code switches that make applied presets route through local webClx proxy endpoints before reaching their real upstream APIs.

**Architecture:** Persist a small upstream proxy settings file with the two enable flags and active preset IDs. Applying a preset writes either the direct upstream URL or the local proxy URL based on the matching flag, then updates the active preset ID. New local-only proxy routes resolve the active preset and forward OpenAI-compatible or Anthropic-compatible traffic with upstream credentials injected by webClx.

**Tech Stack:** Rust, Axum, reqwest, `auth_core`, static HTML/JavaScript, Cargo unit tests, Node static tests.

---

### File Structure

- Modify `crates/auth_core/src/models.rs`: add upstream proxy settings/request/response structs and list response fields.
- Modify `crates/auth_core/src/storage.rs`: load, persist, and expose `webclx-upstream-proxy.json` through `AuthPresetManager`.
- Modify `crates/auth_core/src/lib.rs`: add local proxy URL helpers, proxy-aware provider helpers, and summary active-state helpers.
- Modify `crates/auth_core/src/config.rs`: add proxy-aware Claude settings writer helper.
- Modify `crates/auth_core/src/tests.rs` and `crates/auth_core/src/tests/claude.rs`: cover config URL selection and Claude local proxy settings.
- Modify `src/auth.rs` and `src/auth/apply.rs`: include settings in list responses, add toggle save endpoint, update apply behavior.
- Create `src/upstream_proxy.rs`: generic local OpenAI/Anthropic forwarding.
- Modify `src/main.rs`: register new routes and module.
- Modify `static/index.html`: add one toggle above each saved preset table.
- Modify `static/app.js`: load/render/save toggle state and reapply active presets after toggle changes.
- Modify `tests/preset-table-shared-renderer.test.mjs`: add static UI wiring assertions.
- Modify `docs/codex/index.md`: add a concise context entry for the upstream proxy feature.

### Task 1: Persist Upstream Proxy Settings

**Files:**
- Modify: `crates/auth_core/src/models.rs`
- Modify: `crates/auth_core/src/storage.rs`
- Test: `crates/auth_core/src/tests.rs`

- [ ] **Step 1: Add failing tests for settings defaults and persistence**

Add tests like:

```rust
#[test]
fn upstream_proxy_settings_default_to_disabled() {
    let settings = UpstreamProxySettings::default();
    assert!(!settings.codex_api_proxy_enabled);
    assert!(!settings.claude_proxy_enabled);
    assert_eq!(settings.active_api_proxy_preset_id, None);
    assert_eq!(settings.active_claude_proxy_preset_id, None);
}

#[test]
fn auth_manager_persists_upstream_proxy_settings() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("webclx-upstream-proxy-{unique}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");

    let manager = super::AuthPresetManager::load(&dir).expect("manager should load");
    let mut settings = manager.upstream_proxy_settings();
    settings.codex_api_proxy_enabled = true;
    settings.active_api_proxy_preset_id = Some("api-1".to_string());
    super::persist_upstream_proxy_settings(&manager, settings.clone())
        .expect("settings should persist");

    let reloaded = super::AuthPresetManager::load(&dir).expect("manager should reload");
    fs::remove_dir_all(&dir).ok();

    assert_eq!(reloaded.upstream_proxy_settings(), settings);
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p auth_core upstream_proxy_settings -- --nocapture`

Expected: FAIL because `UpstreamProxySettings`, `upstream_proxy_settings`, and `persist_upstream_proxy_settings` do not exist.

- [ ] **Step 3: Implement settings model and storage**

Add:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UpstreamProxySettings {
    pub codex_api_proxy_enabled: bool,
    pub claude_proxy_enabled: bool,
    pub active_api_proxy_preset_id: Option<String>,
    pub active_claude_proxy_preset_id: Option<String>,
}
```

Extend `AuthPresetManager` with `upstream_proxy_settings: Arc<RwLock<UpstreamProxySettings>>` and `upstream_proxy_settings_file: Arc<PathBuf>`. Load from `webclx-upstream-proxy.json`, default to disabled when missing, and write pretty JSON in `persist_upstream_proxy_settings`.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p auth_core upstream_proxy_settings -- --nocapture`

Expected: PASS.

### Task 2: Add Proxy-Aware Config Helpers

**Files:**
- Modify: `crates/auth_core/src/lib.rs`
- Modify: `crates/auth_core/src/config.rs`
- Test: `crates/auth_core/src/tests.rs`
- Test: `crates/auth_core/src/tests/claude.rs`

- [ ] **Step 1: Add failing tests for local proxy URLs**

Add tests like:

```rust
#[test]
fn api_provider_base_url_uses_local_proxy_when_enabled() {
    let preset = sample_api_preset("https://api.example.com/v1", Some("gpt-5.4"));
    let base_url = api_provider_base_url_for_mode(&preset, true);
    assert!(base_url.ends_with("/api/upstream/openai/v1"));
    assert_ne!(base_url, preset.base_url);
}

#[test]
fn api_provider_base_url_keeps_existing_special_proxy_when_disabled() {
    let preset = sample_api_preset("https://api.minimaxi.com/v1", Some("codex-MiniMax-M2.7"));
    let base_url = api_provider_base_url_for_mode(&preset, false);
    assert!(base_url.contains("/api/codex-proxy/minimax/v1"));
}
```

Add Claude test:

```rust
#[test]
fn claude_settings_writer_can_use_local_proxy_base_and_placeholder_token() {
    let preset = sample_claude_preset();
    let updated = set_claude_settings_in_value_with_endpoint(
        Value::Object(Map::new()),
        &preset,
        "http://127.0.0.1:11111/api/upstream/anthropic",
        "webclx-local-claude-proxy",
    )
    .expect("settings should update");
    let env = updated.get("env").and_then(Value::as_object).unwrap();
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
        Some("http://127.0.0.1:11111/api/upstream/anthropic")
    );
    assert_eq!(
        env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str),
        Some("webclx-local-claude-proxy")
    );
    assert_eq!(
        env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").and_then(Value::as_str),
        Some("glm-5.1")
    );
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p auth_core proxy_base_url claude_settings_writer_can_use_local_proxy -- --nocapture`

Expected: FAIL because helper names do not exist.

- [ ] **Step 3: Implement helper functions**

Add constants and helpers:

```rust
pub const LOCAL_PROXY_API_KEY: &str = "webclx-local-api-proxy";
pub const LOCAL_PROXY_CLAUDE_TOKEN: &str = "webclx-local-claude-proxy";
pub const OPENAI_UPSTREAM_PROXY_BASE_PATH: &str = "/api/upstream/openai/v1";
pub const ANTHROPIC_UPSTREAM_PROXY_BASE_PATH: &str = "/api/upstream/anthropic";

pub fn api_provider_base_url_for_mode(preset: &StoredApiPreset, proxy_enabled: bool) -> String {
    if proxy_enabled {
        format!("{}{}", local_webclx_origin(), OPENAI_UPSTREAM_PROXY_BASE_PATH)
    } else {
        api_provider_base_url(preset)
    }
}

pub fn claude_provider_base_url_for_mode(proxy_enabled: bool) -> String {
    if proxy_enabled {
        format!("{}{}", local_webclx_origin(), ANTHROPIC_UPSTREAM_PROXY_BASE_PATH)
    } else {
        String::new()
    }
}
```

Refactor `set_claude_settings_in_value` through:

```rust
pub fn set_claude_settings_in_value_with_endpoint(
    settings: Value,
    preset: &StoredClaudePreset,
    base_url: &str,
    auth_token: &str,
) -> Result<Value>
```

so the existing function calls it with `preset.base_url` and `preset.auth_token`.

- [ ] **Step 4: Run tests and verify GREEN**

Run: `cargo test -p auth_core proxy_base_url claude_settings_writer_can_use_local_proxy -- --nocapture`

Expected: PASS.

### Task 3: Apply Presets Through Direct Or Local Proxy Mode

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/auth/apply.rs`
- Modify: `crates/auth_core/src/models.rs`
- Test: `crates/auth_core/src/tests.rs`

- [ ] **Step 1: Add failing auth_core tests for proxy-aware active summaries**

Add tests that call a new helper:

```rust
#[test]
fn api_preset_summary_is_active_from_proxy_state_when_proxy_enabled() {
    let preset = sample_api_preset("https://api.example.com/v1", Some("gpt-5.4"));
    let settings = UpstreamProxySettings {
        codex_api_proxy_enabled: true,
        active_api_proxy_preset_id: Some(preset.id.clone()),
        ..Default::default()
    };
    let summary = api_preset_summary_with_proxy_state(&preset, CurrentAuthMode::None, None, &settings);
    assert!(summary.active);
}
```

- [ ] **Step 2: Run test and verify RED**

Run: `cargo test -p auth_core api_preset_summary_is_active_from_proxy_state -- --nocapture`

Expected: FAIL because summary helper does not exist.

- [ ] **Step 3: Implement proxy-aware list response fields and summaries**

Add fields to list responses:

```rust
pub upstream_proxy: UpstreamProxySettings,
```

Add proxy-aware summary helpers that mark active by active preset ID when the corresponding proxy mode is enabled, otherwise delegate to the existing direct matching behavior.

- [ ] **Step 4: Update apply handlers**

In `apply_api_preset`:

- Read `let upstream_proxy = state.auth_manager.upstream_proxy_settings();`
- Use `LOCAL_PROXY_API_KEY` in `auth.json` when `upstream_proxy.codex_api_proxy_enabled` is true.
- Use `api_provider_base_url_for_mode(&preset, upstream_proxy.codex_api_proxy_enabled)` when writing config.
- Persist `active_api_proxy_preset_id = Some(preset.id.clone())` after successful apply.

In `apply_claude_preset`:

- Read `upstream_proxy`.
- If `claude_proxy_enabled`, write settings with local Anthropic proxy base and `LOCAL_PROXY_CLAUDE_TOKEN`.
- If disabled, keep existing direct writer.
- Persist `active_claude_proxy_preset_id = Some(preset.id.clone())` after successful apply.

- [ ] **Step 5: Run targeted tests**

Run: `cargo test -p auth_core api_preset_summary_is_active_from_proxy_state proxy_base_url claude_settings_writer -- --nocapture`

Expected: PASS.

### Task 4: Add Toggle API Endpoint

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add handlers**

Add:

```rust
pub async fn update_upstream_proxy_settings(
    State(state): State<AppState>,
    Json(payload): Json<UpdateUpstreamProxySettingsRequest>,
) -> ApiResult<Json<UpstreamProxySettingsResponse>>
```

The request has optional bool fields for `codex_api_proxy_enabled` and `claude_proxy_enabled`. The handler merges provided fields into current settings and persists them.

- [ ] **Step 2: Register route**

Add:

```rust
.route(
    "/api/auth/upstream-proxy-settings",
    put(auth::update_upstream_proxy_settings),
)
```

- [ ] **Step 3: Run backend compile check**

Run: `cargo check`

Expected: PASS.

### Task 5: Add Generic Local Proxy Routes

**Files:**
- Create: `src/upstream_proxy.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Implement local-only guards and route skeletons**

Create handlers:

```rust
pub async fn openai_upstream_proxy(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response>

pub async fn anthropic_upstream_proxy(...)
```

Reject non-loopback IPs with `StatusCode::FORBIDDEN`.

- [ ] **Step 2: Implement OpenAI forwarding**

Resolve `state.auth_manager.upstream_proxy_settings()`. If disabled or no active preset ID, return `AppError::bad_request`. Find the active `StoredApiPreset`. Build upstream URL by appending the local request suffix after `/api/upstream/openai/v1` to `preset.base_url`. Forward with `reqwest`, remove hop-by-hop headers, inject `Authorization: Bearer <preset.api_key>`.

For `POST /responses` when `effective_api_responses_proxy(&preset).is_some()`, route through the existing conversion helpers used by `src/codex_proxy.rs`.

- [ ] **Step 3: Implement Anthropic forwarding**

Resolve active Claude preset, append suffix after `/api/upstream/anthropic` to `preset.base_url`, remove hop-by-hop headers, inject both `x-api-key` and `Authorization: Bearer <preset.auth_token>` only when the incoming request does not already require a different compatible header shape.

- [ ] **Step 4: Register routes**

Add module `mod upstream_proxy;` and routes:

```rust
.route("/api/upstream/openai/v1/{*proxy_path}", any(upstream_proxy::openai_upstream_proxy))
.route("/api/upstream/anthropic/{*proxy_path}", any(upstream_proxy::anthropic_upstream_proxy))
```

Import `any` from `axum::routing`.

- [ ] **Step 5: Run backend checks**

Run: `cargo check`

Expected: PASS.

### Task 6: Add Frontend Toggles

**Files:**
- Modify: `static/index.html`
- Modify: `static/app.js`
- Test: `tests/preset-table-shared-renderer.test.mjs`

- [ ] **Step 1: Add failing static tests**

Add assertions:

```js
assert.match(appJs, /api-upstream-proxy-toggle/, "Codex_API upstream proxy toggle should be wired");
assert.match(appJs, /claude-upstream-proxy-toggle/, "Claude upstream proxy toggle should be wired");
assert.match(appJs, /updateUpstreamProxySettings/, "proxy toggle changes should save through the backend");
assert.match(appJs, /applyActiveApiPresetAfterProxyToggle/, "Codex_API toggle should reapply active preset");
assert.match(appJs, /applyActiveClaudePresetAfterProxyToggle/, "Claude toggle should reapply active preset");
```

- [ ] **Step 2: Run test and verify RED**

Run: `node tests/preset-table-shared-renderer.test.mjs`

Expected: FAIL on the new assertions.

- [ ] **Step 3: Add HTML controls**

Add a compact toggle row above each table:

```html
<label class="toggle-row upstream-proxy-toggle-row">
  <input id="api-upstream-proxy-toggle" type="checkbox" />
  <span class="toggle-note">通过 webClx 本机代理转发</span>
</label>
```

and equivalent `claude-upstream-proxy-toggle`.

- [ ] **Step 4: Add JavaScript state and handlers**

Load `response.upstream_proxy` from both list endpoints. When a toggle changes, call `PUT /api/auth/upstream-proxy-settings` with the changed field, then reapply the currently active preset from `state.apiPresets` or `state.claudePresets`.

- [ ] **Step 5: Run frontend test**

Run: `node tests/preset-table-shared-renderer.test.mjs`

Expected: PASS.

### Task 7: Verification, Deployment Sync, And Context Memory

**Files:**
- Modify: `docs/codex/index.md`
- Sync: `/home/bin/webclx/static/index.html`
- Sync: `/home/bin/webclx/static/app.js`

- [ ] **Step 1: Update context index**

Add a short section noting:

- Toggle state lives in `webclx-upstream-proxy.json`.
- Codex_API local proxy base is `/api/upstream/openai/v1`.
- Claude local proxy base is `/api/upstream/anthropic`.
- Static files must be synced after UI changes.

- [ ] **Step 2: Run full relevant checks**

Run:

```bash
cargo test -p auth_core
cargo check
node tests/preset-table-shared-renderer.test.mjs
```

Expected: all PASS.

- [ ] **Step 3: Sync static files**

Run:

```bash
install -m 0644 static/index.html /home/bin/webclx/static/index.html
install -m 0644 static/app.js /home/bin/webclx/static/app.js
```

- [ ] **Step 4: Build and install backend if checks pass**

Run:

```bash
TARGET_DIR=$(cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')
cargo build --release
install -m 0755 "$TARGET_DIR/release/webclx" /home/bin/webclx/webClx
systemctl restart webclx.service
```

- [ ] **Step 5: Verify service**

Run:

```bash
systemctl is-active webclx.service
curl -fsS http://127.0.0.1:11111/api/auth/api-presets >/tmp/webclx-api-presets.json
curl -fsS http://127.0.0.1:11111/api/auth/claude-presets >/tmp/webclx-claude-presets.json
```

Expected: service is `active`, both curl commands succeed, and responses include `upstream_proxy`.

### Self-Review

- Spec coverage: the plan covers independent toggles, persisted state, apply-time config switching, local proxy routes, frontend controls, tests, and deployment sync.
- Placeholder scan: no `TBD`, `TODO`, or unspecified test steps remain.
- Type consistency: `UpstreamProxySettings`, `UpdateUpstreamProxySettingsRequest`, local proxy path constants, and helper names are used consistently across tasks.
