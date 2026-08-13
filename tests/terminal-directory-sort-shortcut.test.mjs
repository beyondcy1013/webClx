import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const terminalSettings = require("../static/terminal-settings.js");
const terminalSessionActivity = require("../static/terminal-session-activity.js");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

for (const html of [indexHtml, terminalHtml]) {
  assert.match(
    html,
    /terminal-session-activity\.js\?v=20260726a/,
    "pages using terminal session sorting should load the current activity helper version",
  );
}

const defaultSortCommand = terminalSettings.DEFAULT_TERMINAL_PAGE_FUNCTION_COMMANDS.find(
  (command) => command.key === "sort_by_path",
);
assert.deepEqual(
  defaultSortCommand,
  {
    key: "sort_by_path",
    label: "切换终端排序",
    action: "sort_directory_sessions_by_path",
    command: "",
    shortcut: "Ctrl+Alt+O",
  },
  "directory sorting should reserve Ctrl+Alt+O in terminal-page defaults",
);

const migratedCommands = terminalSettings.ensureBuiltInTerminalFunctionCommands([
  {
    key: "sort_by_path",
    label: "旧目录排序",
    action: "sort_directory_sessions_by_path",
    command: "",
    shortcut: "Ctrl+Alt+P",
  },
]);
assert.equal(
  migratedCommands.find((command) => command.key === "sort_by_path")?.shortcut,
  "Ctrl+Alt+O",
  "saved directory-sort commands should migrate stale shortcuts to Ctrl+Alt+O",
);

assert.deepEqual(
  ["", "workspace", "agent", "status"].map((mode) =>
    terminalSessionActivity.nextTerminalSessionSortMode(mode),
  ),
  ["workspace", "agent", "status", "workspace"],
  "repeated sort invocations should cycle workspace, agent, and status modes",
);

const sessions = [
  { id: "claude-b", name: "Terminal 10", path: "/beta", activity_agent: "Claude", activity_state: "idle" },
  { id: "unknown-a", name: "Terminal 1", path: "/alpha", activity_agent: "", activity_state: "completed" },
  { id: "codex-b", name: "Terminal 2", path: "/beta", activity_agent: "Codex", activity_state: "working" },
  { id: "codex-a", name: "Terminal 3", path: "/alpha", activity_agent: "Codex", activity_state: "error" },
];

assert.deepEqual(
  terminalSessionActivity.sortTerminalSessions(sessions, "workspace").map((session) => session.id),
  ["claude-b", "codex-b", "unknown-a", "codex-a"],
  "workspace mode should group matching paths behind their first occurrence without reordering groups or sessions",
);
assert.deepEqual(
  terminalSessionActivity.sortTerminalSessions(sessions, "agent").map((session) => session.id),
  ["claude-b", "codex-a", "codex-b", "unknown-a"],
  "agent mode should group detected agent types and place unknown agents last",
);
assert.deepEqual(
  terminalSessionActivity.sortTerminalSessions(sessions, "status").map((session) => session.id),
  ["codex-a", "codex-b", "unknown-a", "claude-b"],
  "status mode should prioritize errors, working sessions, pending review, then idle sessions",
);
assert.deepEqual(
  sessions.map((session) => session.id),
  ["claude-b", "unknown-a", "codex-b", "codex-a"],
  "terminal-list sorting should not mutate the fetched session array",
);
