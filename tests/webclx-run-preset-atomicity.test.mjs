import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const cli = readFileSync(new URL("../src/cli.rs", import.meta.url), "utf8");
const runStart = cli.indexOf("async fn run_agent(");
const runEnd = cli.indexOf("\n#[cfg(test)]", runStart);
const runAgent = cli.slice(runStart, runEnd);

test("webclx run uses normal real-config apply without a runtime gate", () => {
  assert.match(
    runAgent,
    /find\(\|preset\| preset\.active\)[\s\S]*client\.apply\(kind, &target, Some\(&cwd\), true\)\.await[\s\S]*Command::new\(agent\)/,
  );
  assert.doesNotMatch(runAgent, /lease|heartbeat|preset-run-leases/);
  assert.match(
    runAgent,
    /read_applied_codex_selection[\s\S]*codex_model_provider[\s\S]*prepare_codex_history_args_model_for_current_user[\s\S]*codex_history_args_with_model/,
    "resume must migrate rollout model and provider while resume and fork receive the applied model override",
  );
  assert.match(
    runAgent,
    /\.spawn\(\)[\s\S]*settle_agent_startup[\s\S]*restore_previous_preset[\s\S]*child\s*\.wait\(\)/,
    "the previous preset must be restored after the startup settle window and before waiting for exit",
  );
  assert.match(
    runAgent,
    /claude_startup_model[\s\S]*settle_agent_startup\([\s\S]*claude_startup_model/,
    "Claude temporary runs must watch the fresh startup banner to time the restore",
  );
  assert.match(
    runAgent,
    /is_program\(agent, "claude"\)[\s\S]*claude_snapshot_banner_count/,
    "Claude startup detection must observe a fresh Claude Code banner, not process survival alone",
  );
  assert.match(
    runAgent,
    /AGENT_STARTUP_FALLBACK_DELAY/,
    "a fixed fallback delay must restore the previous preset when no startup signal fires",
  );
  assert.doesNotMatch(
    runAgent,
    /start_kill|kill_agent_process_group|kill\(/,
    "an undetected or failed startup must never terminate the agent",
  );
  assert.doesNotMatch(
    runAgent,
    /child\s*\.wait\(\)[\s\S]*restore_previous_preset\(&client/,
    "temporary restore must not depend on the agent exiting",
  );
  assert.match(runAgent, /client\.apply\(kind, original, Some\(cwd\), true\)/);
  assert.match(
    runAgent,
    /prepare_codex_history_args_model_for_current_user[\s\S]*if let Err\(error\)[\s\S]*restore_previous_preset/,
    "history preparation failures must restore the previous preset before returning",
  );
});

test("webclx run has no runtime gate and never creates alternate homes", () => {
  assert.match(runAgent, /no runtime preset gate is held/);
  assert.doesNotMatch(cli, /preset-run-leases|heartbeat_lease|acquire_lease|release_lease/);
  assert.doesNotMatch(cli, /CODEX_HOME|CLAUDE_CONFIG_DIR|WEBCLX_USER_HOME/);
});

test("preset apply sends the working directory through the serialized apply operation", () => {
  const applyStart = cli.indexOf("async fn apply(");
  const applyEnd = cli.indexOf("\n    async fn", applyStart + 1);
  const apply = cli.slice(applyStart, applyEnd);

  assert.match(apply, /project_path: Option<&Path>/);
  assert.match(apply, /query\(&\[\("project_path", project_path\)\]\)/);
});

test("webclx use also synchronizes a project-local Codex configuration", () => {
  const useStart = cli.indexOf("async fn use_preset(");
  const useEnd = cli.indexOf("\nasync fn run_agent(", useStart);
  const usePreset = cli.slice(useStart, useEnd);

  assert.match(usePreset, /env::current_dir\(\)/);
  assert.match(usePreset, /client\.apply\(kind, &preset, Some\(&cwd\), false\)\.await/);
});
