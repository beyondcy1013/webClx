import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const readSource = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const terminalSettingsJs = readSource("../static/terminal-settings.js");
const terminalShellSettingsJs = readSource("../static/terminal-shell-settings.js");
const terminalSettingsLoaderJs = readSource("../static/terminal-settings-loader.js");
const terminalJs = readSource("../static/terminal.js");
const appJs = readSource("../static/app.js");
const appSettingsLoadSaveJs = readSource("../static/app-settings-load-save.js");
const appSettingsEventBindingsJs = readSource("../static/app-settings-event-bindings.js");
const indexHtml = readSource("../static/index.html");
const settingsCore = readSource("../crates/settings_core/src/lib.rs");
const settingsManager = readSource("../crates/settings_core/src/manager.rs");
const settingsStorage = readSource("../crates/settings_core/src/storage.rs");
const settingsApi = readSource("../crates/settings_core/src/api.rs");

const settingsContext = { module: { exports: {} }, exports: {} };
vm.runInNewContext(terminalSettingsJs, settingsContext);
const settings = settingsContext.module.exports;

assert.equal(settings.DEFAULT_TERMINAL_SCROLLBACK_LINES, 5000);
assert.equal(settings.normalizeTerminalScrollbackLines(undefined), 5000);
assert.equal(settings.normalizeTerminalScrollbackLines("25000"), 25000);
assert.equal(settings.normalizeTerminalScrollbackLines(1), 100);
assert.equal(settings.normalizeTerminalScrollbackLines(200000), 100000);

assert.match(
  indexHtml,
  /id="terminal-scrollback-lines-input"[\s\S]*min="100"[\s\S]*max="100000"[\s\S]*step="100"[\s\S]*placeholder="5000"[\s\S]*id="terminal-error-line-limit-input"/,
  "terminal settings should expose a bounded scrollback line input before error matching",
);

assert.match(
  `${appJs}\n${appSettingsLoadSaveJs}`,
  /terminalScrollbackLines: DEFAULT_TERMINAL_SCROLLBACK_LINES[\s\S]*state\.terminalScrollbackLines = normalizeTerminalScrollbackLines\([\s\S]*settings\.terminal_scrollback_lines[\s\S]*terminalScrollbackLinesInputEl\.value = String\(state\.terminalScrollbackLines\)[\s\S]*terminal_scrollback_lines: nextTerminalScrollbackLines/,
  "settings page should initialize, load, render, and persist terminal scrollback lines",
);

assert.match(
  appSettingsEventBindingsJs,
  /readTerminalTouchSelectionLongPressMsFromInput\(\),[\s\S]*normalizeTerminalScrollbackLines\(terminalScrollbackLinesInputEl\?\.value\),[\s\S]*readTerminalErrorMatchLineLimitFromInput\(\)/,
  "ordinary settings save should pass the scrollback value in the declared argument position",
);

assert.match(
  appSettingsEventBindingsJs,
  /DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,[\s\S]*DEFAULT_TERMINAL_SCROLLBACK_LINES,[\s\S]*DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT,[\s\S]*DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,[\s\S]*DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR,[\s\S]*DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT/,
  "restore defaults should preserve scrollback and auto-continue positional arguments",
);

assert.match(
  `${terminalJs}\n${terminalSettingsLoaderJs}\n${terminalShellSettingsJs}`,
  /terminalScrollbackLines: DEFAULT_TERMINAL_SCROLLBACK_LINES[\s\S]*applyTerminalScrollbackLines\(settings\.terminal_scrollback_lines\)[\s\S]*function applyTerminalScrollbackLines\([\s\S]*term\.options\.scrollback = normalized[\s\S]*scrollback: normalizeTerminalScrollbackLines\(state\.terminalScrollbackLines\)/,
  "terminal page should apply configured scrollback to both current and future xterm instances",
);

const runtimeContext = {
  state: { terminalScrollbackLines: 5000 },
  normalizeTerminalScrollbackLines: settings.normalizeTerminalScrollbackLines,
};
vm.runInNewContext(terminalShellSettingsJs, runtimeContext);
vm.runInNewContext(
  "term = { options: { scrollback: 5000 } }; globalThis.applied = applyTerminalScrollbackLines(4321); globalThis.current = term.options.scrollback;",
  runtimeContext,
);
assert.equal(runtimeContext.applied, 4321);
assert.equal(runtimeContext.current, 4321);
assert.equal(runtimeContext.state.terminalScrollbackLines, 4321);

assert.match(
  settingsCore,
  /DEFAULT_TERMINAL_SCROLLBACK_LINES: u32 = 5_000[\s\S]*terminal_scrollback_lines:[\s\S]*fn normalize_terminal_scrollback_lines\(value: u32\)[\s\S]*value\.clamp\(100, 100_000\)/,
  "settings core should define and carry the bounded scrollback setting",
);
assert.match(
  settingsManager,
  /pub fn terminal_scrollback_lines\(&self\) -> u32[\s\S]*terminal_scrollback_lines: u32[\s\S]*= terminal_scrollback_lines/,
  "settings manager should expose and update terminal scrollback lines",
);
assert.match(
  settingsStorage,
  /normalize_terminal_scrollback_lines\(parsed\.terminal_scrollback_lines\)[\s\S]*terminal_scrollback_lines: u32[\s\S]*terminal_scrollback_lines: normalize_terminal_scrollback_lines\(terminal_scrollback_lines\)/,
  "settings storage should normalize loaded and persisted scrollback lines",
);
assert.match(
  settingsApi,
  /terminal_scrollback_lines: manager\.terminal_scrollback_lines\(\)[\s\S]*normalize_terminal_scrollback_lines\([\s\S]*payload[\s\S]*terminal_scrollback_lines[\s\S]*"terminal_scrollback_lines"/,
  "settings API should return, save, and merge terminal scrollback lines",
);
