import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

const settingsModule = await import("../static/terminal-settings.js");
const {
  DEFAULT_TERMINAL_AGENT_COMMANDS,
  cloneDefaultTerminalAgentCommands,
  normalizeTerminalAgentCommands,
} = settingsModule.default;
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const settingsHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const loader = readFileSync(new URL("../static/terminal-settings-loader.js", import.meta.url), "utf8");
const loadSave = readFileSync(new URL("../static/app-settings-load-save.js", import.meta.url), "utf8");
const eventBindings = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);
const specifiedTask = readFileSync(
  new URL("../static/terminal-specified-task.js", import.meta.url),
  "utf8",
);
const styles = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");

test("terminal agent commands use built-in defaults when unconfigured", () => {
  assert.equal(DEFAULT_TERMINAL_AGENT_COMMANDS.length, 9);
  assert.deepEqual(
    normalizeTerminalAgentCommands(undefined).map((item) => item.key),
    ["codex", "cdx", "claude", "codebuddy", "devin", "agy", "cline", "opencode", "other"],
  );
  assert.deepEqual(
    normalizeTerminalAgentCommands([]),
    cloneDefaultTerminalAgentCommands(),
  );
});

test("custom terminal agent commands are normalized and shell-safe", () => {
  const normalized = normalizeTerminalAgentCommands([
    { key: "Qoder", label: "Qoder CLI", new_command: "qoder", resume_command: "qoder --resume {session_id}" },
    { key: "bad key", label: "Bad", new_command: "bad", resume_command: "" },
    { key: "empty", label: "Empty", new_command: " ", resume_command: "" },
  ], null);
  assert.equal(normalized.length, 1);
  assert.equal(normalized[0].key, "qoder");
  assert.equal(normalized[0].label, "Qoder CLI");
  assert.equal(normalized[0].resume_command, "qoder --resume {session_id}");
  assert.equal(normalizeTerminalAgentCommands([
    { key: "x", new_command: "bad {other}", resume_command: "" },
  ], null).length, 1);
});

test("new terminal dialogs render configurable agents with visual separation", () => {
  for (const html of [terminalHtml, indexHtml]) {
    assert.match(html, /id="terminal-specified-task-agent-list"/);
    assert.doesNotMatch(html, /name="terminal-specified-task-agent" value="devin"/);
    assert.match(html, /terminal-specified-task\.js\?v=/);
  }
  assert.match(specifiedTask, /renderTerminalSpecifiedTaskAgents/);
  assert.match(specifiedTask, /terminalSpecifiedTaskAgentLaunchCommand/);
  assert.match(styles, /\.terminal-specified-task-agent-list \{[^}]*gap: 4px;/);
  assert.match(
    styles,
    /\.terminal-specified-task-agent-modes \.terminal-specified-task-group-title \{[^}]*margin-right: 2px;[^}]*border-right/,
  );
});

test("agent lookup does not recurse and supports a custom resume command", () => {
  assert.match(specifiedTask, /function terminalSpecifiedTaskAgentCommand\(agent\) \{/);
  assert.match(specifiedTask, /terminalSpecifiedTaskAgentCommand\(value\)\?\.key \|\| "codex"/);
});

test("settings persist and reload configurable terminal agents", () => {
  assert.match(settingsHtml, /id="terminal-agent-commands-input"/);
  assert.match(loader, /terminalSpecifiedTaskSetAgentCommands\(state\.terminalAgentCommands\)/);
  assert.match(loadSave, /terminal_agent_commands: nextTerminalAgentCommands/);
  assert.match(eventBindings, /readTerminalAgentCommandsFromInput\(\)/);
});
