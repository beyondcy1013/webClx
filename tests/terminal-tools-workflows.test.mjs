import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalToolActionsJs = readFileSync(
  new URL("../static/terminal-tool-actions.js", import.meta.url),
  "utf8",
);
const appJs = readEntryScriptBundle("index.html");
const terminalJs = readEntryScriptBundle("terminal.html");

assert.match(
  appHtml,
  /data-settings-category="tools"[\s\S]*?>\s*工作流\s*<\/button>[\s\S]*id="settings-panel-tools"[\s\S]*id="terminal-tool-entries-body"[\s\S]*id="terminal-tool-editor-dialog"/,
  "settings should expose the workflow tab, entry table, and editor",
);
assert.match(
  appJs,
  /state\.terminalToolEntries = ensureBuiltInTerminalToolEntries[\s\S]*terminal_tool_entries: nextTerminalToolEntries/,
  "settings should load and save weapon entries through the authoritative settings payload",
);
assert.match(
  appJs,
  /if \(action\.kind === "create_terminal" \|\| action\.kind === "fork_session"[\s\S]*action\.kind === "fork_session" \? "自动提取 resume"[\s\S]*"当前目录"/,
  "the settings editor should render fork_session as a parameter-free action",
);
assert.match(
  terminalHtml,
  /id="terminal-tools-button"[\s\S]*aria-controls="terminal-tools-menu"[\s\S]*id="terminal-tools-menu"[\s\S]*id="terminal-tool-menu"[\s\S]*role="group"[\s\S]*id="terminal-tool-menu-body"/,
  "terminal tools should own the embedded hierarchical weapon entries",
);
assert.match(
  terminalHtml,
  /id="navigate-back"[\s\S]*id="terminal-workflows-button"[\s\S]*>工作流<\/button>[\s\S]*id="navigate-forward"/,
  "the workflow button should sit between Back and Forward without a dedicated proxy button",
);
assert.doesNotMatch(
  terminalHtml,
  /terminal-proxy-settings-button/,
  "the dedicated proxy settings button should be removed",
);
assert.doesNotMatch(
    terminalToolActionsJs,
  /async function launchProxySettingsWorkflow\(\)[\s\S]*?executeSpecifiedPreset\(\{[\s\S]*?action: "launch"/,
  "proxy settings must not apply MiniMax globally through the fixed-terminal launch path",
);
assert.doesNotMatch(
  terminalHtml,
  /id="terminal-tool-root-tools"|data-terminal-tool-root="tools"/,
  "the weapon root should not remain as a standalone soft-key button",
);
assert.doesNotMatch(
  terminalHtml,
  /<dialog[^>]+id="terminal-tool-dialog"/,
  "the weapon menu must not use a modal dialog",
);
assert.match(
  terminalToolActionsJs,
  /function positionTerminalToolMenu\(\)[\s\S]*positionTerminalToolsMenu\(\)/,
  "the embedded weapon entries should reuse terminal-tools positioning",
);
assert.match(
  terminalJs,
  /terminalToolsMenuEl[\s\S]*document\.addEventListener\("pointerdown"[\s\S]*closeTerminalToolsMenu\(\)[\s\S]*event\.key !== "Escape"[\s\S]*closeTerminalToolsMenu\(\{ restoreFocus: true \}\)/,
  "the combined terminal-tools menu should close on outside click and Escape",
);
assert.match(
  terminalJs,
  /async function executeTerminalToolAction\(action, executionContext = \{\}\)[\s\S]*case "create_terminal"[\s\S]*case "fork_session"[\s\S]*case "rename_terminal"[\s\S]*case "switch_api_preset"[\s\S]*case "codex_launch"[\s\S]*case "function_command"[\s\S]*case "run_workflow"[\s\S]*case "wait"[\s\S]*case "send_command"/,
  "the executor should whitelist all supported action types",
);
assert.match(
  terminalToolActionsJs,
  /async function forkTerminalSessionForTool\(executionContext\)[\s\S]*extractLatestResumeInfo\([\s\S]*sourceUuid[\s\S]*codex fork \$\{sourceUuid\}[\s\S]*createSession\(\{[\s\S]*path: sourcePath[\s\S]*sendTerminalAutoTypedInput\(forkCommand[\s\S]*renameTerminalForTool\(created\.id, `\$\{sourceSessionName\}_fork`/,
  "fork should read the source UUID, create a new terminal, run codex fork <uuid> there, then rename to <name>_fork",
);
assert.doesNotMatch(
  terminalToolActionsJs,
  /runTerminalSlashCommandByKey\("fork"/,
  "fork must not run in-session /fork on the source terminal (that mutates the source)",
);
assert.match(
  terminalJs,
  /for \(let index = 0; index < entry\.actions\.length; index \+= 1\)[\s\S]*await executeTerminalToolAction\(action, executionContext\)/,
  "workflow actions should execute serially",
);
assert.match(
  terminalJs,
  /async function executeTerminalToolEntry\(entry\)[\s\S]*closeTerminalToolMenu\(\);\s*terminalToolExecutionRunning = true;[\s\S]*await executeTerminalToolAction\(action, executionContext\)/,
  "selecting a workflow should close the menu before its first asynchronous action",
);
assert.doesNotMatch(
  terminalJs,
  /function closeTerminalToolMenu\([^)]*\) \{\s*if \(terminalToolExecutionRunning\)/,
  "the menu close control should remain available while a workflow is running",
);
assert.match(
  terminalJs,
  /async function createSession\([\s\S]*throwOnError = false[\s\S]*return session;[\s\S]*if \(throwOnError\) \{[\s\S]*throw error/,
  "workflow terminal creation should propagate failures when requested",
);

// The built-in proxy_settings_workflow should appear in default entries
const terminalSettingsJs2 = readFileSync(
  new URL("../static/terminal-settings.js", import.meta.url),
  "utf8",
);
assert.match(
  terminalSettingsJs2,
  /id: "proxy_settings_workflow"[\s\S]*kind: "codex_launch"[\s\S]*\$mihomo-proxy-ops[\s\S]*preset_selector: "miniMax"[\s\S]*cwd: "\/home\/system"[\s\S]*project_path: "\/home\/system"/,
  "proxy_settings_workflow should be a built-in codex_launch entry with the correct configuration",
);

// Import/export controls should exist
assert.match(
  appHtml,
  /id="terminal-tool-export"[\s\S]*id="terminal-tool-import"/,
  "workflow settings should expose import and export controls",
);
