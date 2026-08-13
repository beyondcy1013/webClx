import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const cli = readFileSync(new URL("../src/cli.rs", import.meta.url), "utf8");
const runStart = cli.indexOf("async fn run_agent(");
const runEnd = cli.indexOf("\n#[cfg(test)]", runStart);
const runAgent = cli.slice(runStart, runEnd);

test("webclx run applies one complete persistent preset before starting the agent", () => {
  assert.match(
    runAgent,
    /client\.apply\(kind, &target, Some\(&cwd\)\)\.await[\s\S]*Command::new\(agent\)/,
    "the selected preset and project-local override must be applied before process startup",
  );
  assert.match(
    runAgent,
    /while applied\.deferred[\s\S]*client\.apply\(kind, &target, Some\(&cwd\)\)\.await\?;[\s\S]*Command::new\(agent\)/,
    "a queued apply must wait until the preset is written instead of launching with stale provider settings",
  );
  assert.match(
    runAgent,
    /applied\.codex_model\(\)[\s\S]*codex_history_args_with_model/,
    "resume and fork must use the model read from the configuration written by apply",
  );
});

test("webclx run has no fixed handoff or restoration path", () => {
  assert.doesNotMatch(
    runAgent,
    /STARTUP_PRESET_HANDOFF|acquire_preset_run_lease|release_preset_run_lease|configuration is restored/,
  );
  assert.match(
    cli,
    /selected preset remains active for subsequent processes/,
    "help text must describe the persistent configuration behavior",
  );
  assert.doesNotMatch(cli, /configuration is restored after the agent has read/);
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
  assert.match(usePreset, /client\.apply\(kind, &preset, Some\(&cwd\)\)\.await/);
});
