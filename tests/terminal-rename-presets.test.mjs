import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readEntryScriptBundle("index.html");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");

assert.match(
  appJs,
  /DEFAULT_TERMINAL_RENAME_PRESETS[\s\S]*"完结"[\s\S]*"复用对话"/,
  "settings page should define default terminal rename presets",
);

assert.match(
  indexHtml,
  /id="terminal-rename-presets-input"/,
  "settings system tab should expose a terminal rename presets textarea",
);

assert.match(
  appJs,
  /settings\.terminal_rename_presets[\s\S]*renderTerminalRenamePresetsSetting/,
  "settings loader should read terminal_rename_presets and render the textarea",
);

assert.match(
  appJs,
  /terminal_rename_presets:\s*nextTerminalRenamePresets/,
  "settings saver should persist terminal_rename_presets",
);

assert.match(
  terminalHtml,
  /<dialog id="terminal-rename-dialog" class="terminal-rename-dialog" aria-labelledby="terminal-rename-dialog-title">[\s\S]*id="session-rename-form" class="terminal-rename-dialog-form"[\s\S]*id="terminal-rename-dialog-title"[\s\S]*id="session-rename-input"[\s\S]*id="session-rename-presets"[\s\S]*id="terminal-rename-dialog-status"[\s\S]*>保存名称<\/button>[\s\S]*id="session-rename-cancel"[\s\S]*>取消<\/button>[\s\S]*<\/dialog>/,
  "terminal management should use the workspace history rename dialog while retaining presets",
);

assert.doesNotMatch(
  terminalHtml,
  /id="session-rename-inline"|class="[^"]*terminal-rename-inline/,
  "terminal management should no longer render a separate inline rename editor",
);

assert.match(
  terminalJs,
  /DEFAULT_TERMINAL_RENAME_PRESETS[\s\S]*"完结"[\s\S]*"复用对话"/,
  "terminal page should have the same rename preset fallback",
);

assert.match(
  terminalJs,
  /function appendSessionRenamePreset\(preset\)[\s\S]*_\$\{preset\}/,
  "terminal preset click should append underscore plus preset name",
);

assert.match(
  terminalJs,
  /renderSessionRenamePresets\(\)[\s\S]*data-action", "append-session-rename-preset"/,
  "terminal page should render rename preset buttons",
);

assert.match(
  terminalJs,
  /function sessionRenameSavedName\(sessionName\)[\s\S]*replace\(\/_\+\$\/, ""\)/,
  "terminal page should remove trailing underscores when saving a rename",
);

assert.match(
  terminalJs,
  /async function renameSession\(\)[\s\S]*sessionRenameSavedName\(sessionRenameInputEl\.value\)[\s\S]*name: nextName/,
  "terminal page should submit the normalized rename value",
);

assert.match(
  terminalJs,
  /function startSessionRename\(session, trigger\)[\s\S]*state\.renamingSessionId = session\.id[\s\S]*openTerminalRenameDialog\(sessionRenameDraftName\(session\.name\), trigger\)/,
  "terminal management rename should open the shared dialog with its trigger",
);

assert.match(
  terminalJs,
  /function openTerminalRenameDialog\(name, trigger\)[\s\S]*terminalRenameTriggerEl = trigger[\s\S]*sessionRenameDialogEl\.showModal\(\)[\s\S]*focusTextInputToEnd\(sessionRenameInputEl\)[\s\S]*function closeSessionRenameEditor\(\)[\s\S]*sessionRenameDialogEl\.close\(\)[\s\S]*window\.setTimeout\(\(\) => \{[\s\S]*trigger\?\.focus\(\)[\s\S]*\}, 0\)/,
  "terminal management dialog should focus its input and restore focus on close",
);

assert.match(
  terminalJs,
  /renameSessionButton\.addEventListener\("click"[\s\S]*startSessionRename\(current, renameSessionButton\)/,
  "terminal management should pass the rename button as the dialog focus target",
);

assert.match(
  terminalJs,
  /sessionRenameDialogEl\.addEventListener\("cancel"[\s\S]*event\.preventDefault\(\)[\s\S]*closeSessionRenameEditor\(\)[\s\S]*sessionRenameDialogEl\.addEventListener\("click"[\s\S]*event\.target === sessionRenameDialogEl[\s\S]*closeSessionRenameEditor\(\)[\s\S]*sessionRenameCancelButton\.addEventListener\("click"[\s\S]*closeSessionRenameEditor\(\)/,
  "terminal management dialog should close through Escape, backdrop, or cancel",
);

assert.match(
  terminalJs,
  /async function renameSession\(\)[\s\S]*updateTerminalRenameDialogStatus\("请输入新的终端名称。", "warn"\)[\s\S]*closeSessionRenameEditor\(\);[\s\S]*updateSessionStatus\(`正在改名 \$\{session\.name\}…`, "info"\);[\s\S]*await requestJson[\s\S]*updateSessionStatus\(`终端改名失败：\$\{error\.message\}`, "warn"\)/,
  "terminal rename should close the dialog before awaiting and report background failures on the page",
);

assert.match(
  appJs,
  /async function renameSession\(\)[\s\S]*closeTerminalRenameDialog\(\);[\s\S]*updateSessionsStatus\(`正在改名 \$\{session\.name\}…`, "info"\);[\s\S]*await requestJson[\s\S]*updateSessionsStatus\(`终端改名失败：\$\{error\.message\}`, "warn"\)/,
  "workspace terminal rename should close the dialog before awaiting and keep background status outside it",
);
