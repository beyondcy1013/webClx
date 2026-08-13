import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

test("preset execution uses the shared apply path instead of temporary runtimes", () => {
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
  assert.match(authRoutes, /preset-run-leases/);
  assert.doesNotMatch(cli, /preset-run-leases/);
  assert.match(cli, /client\.apply\(kind, &target, Some\(&cwd\)\)\.await/);
  assert.doesNotMatch(cli, /TemporaryPresetRuntime|runner_path|cleanup_runtime/);
});

test("a manual preset switch is queued behind an isolated task config lease instead of rejected", () => {
  const apply = read("src/auth/apply.rs");
  const lease = read("src/auth/preset_run_lease.rs");
  const apiManager = read("static/app-api-manager.js");

  assert.doesNotMatch(apply, /try_lock_active_config_write\(\)/);
  assert.match(lease, /queue_preset_switch_if_running/);
  assert.match(lease, /pending_switch/);
  assert.match(lease, /restore_snapshot[\s\S]*apply_selected_preset_locked/);
  assert.match(apiManager, /response\.deferred/);
});

test("specified launches and in-terminal switches delegate to webclx run", () => {
  const specifiedActions = read("static/specified-preset-actions.js");
  const inPlaceSwitch = read("static/terminal-in-place-preset-switch.js");

  assert.match(specifiedActions, /webclx run/);
  assert.doesNotMatch(specifiedActions, /temporary-preset-runtime|runner_path|runtimeRunner/);
  assert.match(inPlaceSwitch, /specifiedPresetRunCommand/);
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

test("webclx run documents that the selected shared preset remains active", () => {
  const cli = read("src/cli.rs");

  assert.match(cli, /selected preset remains active for subsequent processes/);
  assert.doesNotMatch(cli, /configuration is restored after the agent has read/);
  assert.doesNotMatch(cli, /hold the global preset gate while the agent runs/);
  assert.doesNotMatch(cli, /configuration is restored after the agent exits/);
});

test("reserved config-home variables are rejected by the preset environment gate", () => {
  const authCore = read("crates/auth_core/src/lib.rs");

  assert.match(authCore, /CODEX_HOME/);
  assert.match(authCore, /CLAUDE_CONFIG_DIR/);
  assert.match(authCore, /WEBCLX_USER_HOME/);
  assert.match(authCore, /is_forbidden_preset_env_key/);
});
