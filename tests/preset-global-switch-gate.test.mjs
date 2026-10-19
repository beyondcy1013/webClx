import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

test("preset execution has no runtime gate or alternate config home", () => {
  const authModule = read("src/auth.rs");
  const authRoutes = read("src/routes/auth.rs");
  const cli = read("src/cli.rs");

  assert.equal(
    existsSync(new URL("../src/auth/temporary_runtime.rs", import.meta.url)),
    false,
    "the temporary preset runtime module must be removed",
  );
  assert.doesNotMatch(authModule, /temporary_runtime/);
  assert.doesNotMatch(authRoutes, /temporary-preset-runtime/);
  assert.doesNotMatch(authRoutes, /preset-run-leases",\s*post/);
  assert.match(authRoutes, /acknowledge_removed_preset_run_lease/);
  assert.match(
    authModule,
    /acknowledge_removed_preset_run_lease[\s\S]*compatibility_only[\s\S]*true/,
  );
  assert.doesNotMatch(cli, /preset-run-leases|acquire_lease|heartbeat_lease|release_lease/);
  assert.match(cli, /client\.apply\(kind, &target, Some\(&cwd\)\)\.await/);
  assert.doesNotMatch(cli, /TemporaryPresetRuntime|runner_path|cleanup_runtime/);
});

test("manual preset switches are written directly without pending runtime gates", () => {
  const apply = read("src/auth/apply.rs");
  const apiManager = read("static/app-api-manager.js");

  assert.doesNotMatch(apply, /try_lock_active_config_write\(\)/);
  assert.doesNotMatch(apply, /queue_preset_switch_if_running|pending_switch/);
  assert.equal(existsSync(new URL("../src/auth/preset_run_lease.rs", import.meta.url)), false);
  assert.match(apiManager, /response\.deferred/);
});

test("specified launches and in-terminal switches delegate temporary ownership to webclx run", () => {
  const specifiedActions = read("static/specified-preset-actions.js");
  const inPlaceSwitch = read("static/terminal-in-place-preset-switch.js");

  assert.match(specifiedActions, /specifiedPresetRunCommand/);
  assert.doesNotMatch(specifiedActions, /acquireSpecifiedPresetLease|releaseSpecifiedPresetLease/);
  assert.doesNotMatch(specifiedActions, /temporary-preset-runtime|runner_path|runtimeRunner/);
  assert.match(inPlaceSwitch, /specifiedPresetRunCommand/);
  assert.doesNotMatch(inPlaceSwitch, /acquireSpecifiedPresetLease|releaseSpecifiedPresetLease/);
  assert.doesNotMatch(inPlaceSwitch, /action:\s*["']prepare["']|runtimeRunner|runtimeId/);
});

test("preset execution never creates alternate Codex or Claude config homes", () => {
  const cli = read("src/cli.rs");
  const codexTask = read("src/codex_task.rs");
  const startupTools = read("src/startup_tools.rs");

  assert.doesNotMatch(cli, /CODEX_HOME|CLAUDE_CONFIG_DIR|WEBCLX_USER_HOME/);
  assert.doesNotMatch(codexTask, /prepare_codex_temporary_runtime|TemporaryPresetRuntime/);
  assert.doesNotMatch(startupTools, /export CLAUDE_CONFIG_DIR=|CLAUDE_CONFIG_SNAPSHOT_WRAPPER/);
});

test("webclx run documents the absence of a runtime preset gate", () => {
  const cli = read("src/cli.rs");

  assert.match(cli, /no runtime preset gate is held/);
  assert.doesNotMatch(cli, /global preset gate|全局预设门禁/);
});

test("reserved config-home variables are rejected by the preset environment gate", () => {
  const authCore = read("crates/auth_core/src/lib.rs");

  assert.match(authCore, /CODEX_HOME/);
  assert.match(authCore, /CLAUDE_CONFIG_DIR/);
  assert.match(authCore, /WEBCLX_USER_HOME/);
  assert.match(authCore, /is_forbidden_preset_env_key/);
});
