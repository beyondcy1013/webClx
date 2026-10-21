import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

const terminalHtml = read("static/terminal.html");
const mobileKeys = read("static/terminal-mobile-keys.js");
const specifiedActions = read("static/specified-preset-actions.js");
const specifiedTask = read("static/terminal-specified-task.js");
const designatePreset = read("static/terminal-designate-preset.js");
const inPlaceSwitch = read("static/terminal-in-place-preset-switch.js");
const permanentSwitch = read("static/terminal-permanent-preset-switch.js");
const cli = read("src/cli.rs");
const settingsApi = read("crates/settings_core/src/api.rs");
const settingsCore = read("crates/settings_core/src/lib.rs");
const settingsLoadSave = read("static/app-settings-load-save.js");

const expectedCommands = [
  ["permanent_switch_preset", "永久"],
  ["open_specified_task", "临时"],
  ["designate_preset_fork", "临时/新终端/fork"],
  ["designate_preset_resume", "临时/新终端/resume"],
  ["switch_preset_in_terminal", "临时/原地/resume"],
  ["switch_preset_in_terminal_new_session", "临时/原地/新对话"],
  ["designate_preset_terminal", "临时/新终端/新对话"],
];

test("project preset command select and visible menu expose the same guarded matrix", () => {
  for (const [action, label] of expectedCommands) {
    assert.match(
      terminalHtml,
      new RegExp(`<option value="${action}">${label.replaceAll("/", "\\/")}<\\/option>`),
    );
    assert.match(
      terminalHtml,
      new RegExp(`data-project-action="${action}">${label.replaceAll("/", "\\/")}<\\/button>`),
    );
  }
});

test("every preset command action dispatches to its dedicated owner", () => {
  const handlers = new Map([
    ["permanent_switch_preset", "openTerminalPermanentPresetSwitchDialog"],
    ["open_specified_task", "openTerminalSpecifiedTaskDialog"],
    ["designate_preset_fork", "openTerminalDesignatePresetForkDialog"],
    ["designate_preset_resume", "openTerminalDesignatePresetResumeDialog"],
    ["switch_preset_in_terminal", "openTerminalInPlacePresetSwitchDialog"],
    ["switch_preset_in_terminal_new_session", "openTerminalInPlacePresetSwitchDialog"],
    ["designate_preset_terminal", "openTerminalDesignatePresetDialog"],
  ]);
  for (const [action, handler] of handlers) {
    assert.match(
      mobileKeys,
      new RegExp(`action === "${action}"[\\s\\S]*?${handler}\\(`),
      `${action} must dispatch to ${handler}`,
    );
  }
});

test("temporary and permanent command families cannot exchange ownership semantics", () => {
  assert.match(permanentSwitch, /action: "apply"/);
  assert.doesNotMatch(permanentSwitch, /specifiedPresetRunCommand|action: "launch"/);

  assert.match(
    specifiedTask,
    /const temporary = terminalSpecifiedTaskTemporary\(\);[\s\S]*temporary,/,
  );
  assert.match(designatePreset, /temporary: true/);
  // 普通“新开终端”必须把所选预设交给统一启动事务；不能仅依赖模型字段。
  assert.match(designatePreset, /resetPreset: true,[\s\S]*temporary: options\.temporary === undefined \? false : options\.temporary/);
  assert.match(specifiedTask, /executeSpecifiedPreset\(\{[\s\S]*action: "launch"[\s\S]*presetId: dom\.preset\.value/);
  assert.match(
    inPlaceSwitch,
    /const normalizedPresetId = String\(selection\.id \|\| ""\)\.trim\(\);[\s\S]*specifiedPresetRunCommand\([\s\S]*normalizedPresetId/,
  );
  assert.match(
    specifiedActions,
    /if \(applied\?\.deferred\)[\s\S]*launchResult: null/,
    "a queued permanent apply must not launch against stale shared config",
  );
  assert.match(
    settingsCore,
    /terminal_new_session_behavior: Option<String>[\s\S]*fn sanitize_terminal_new_session_behavior\([\s\S]*"direct"[\s\S]*"dialog"/,
    "settings core must round-trip and normalize the terminal launch behavior",
  );
  assert.match(
    settingsLoadSave,
    /state\.terminalNewSessionBehavior = WebClxNewTerminal\.normalizeBehavior\([\s\S]*settings\.terminal_new_session_behavior/,
  );
  assert.match(
    settingsLoadSave,
    /terminal_new_session_behavior: nextTerminalNewSessionBehavior/,
  );
  assert.match(
    settingsApi,
    /terminal_new_session_behavior: manager\.terminal_new_session_behavior\(\)[\s\S]*sanitize_terminal_new_session_behavior\([\s\S]*payload\.terminal_new_session_behavior/,
  );
});

test("Codex resume and fork keep model routing without a runtime gate", () => {
  assert.match(
    cli,
    /client\.apply\(kind, &target, Some\(&cwd\), true\)[\s\S]*codex_model_provider[\s\S]*prepare_codex_history_args_model_for_current_user[\s\S]*codex_history_args_with_model/,
  );
  assert.doesNotMatch(cli, /preset-run-leases|acquire_lease|heartbeat_lease|release_lease/);
  assert.doesNotMatch(cli, /CODEX_HOME|CLAUDE_CONFIG_DIR|WEBCLX_USER_HOME/);
});
