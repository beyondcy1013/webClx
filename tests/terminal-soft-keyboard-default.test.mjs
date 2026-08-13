import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readStatic(name) {
  return readFileSync(new URL(`../static/${name}`, import.meta.url), "utf8");
}

const appHtml = readStatic("index.html");
const appSource = readStatic("app.js");
const settingsEventBindingsSource = readStatic("app-settings-event-bindings.js");
const terminalHtml = readStatic("terminal.html");
const terminalSource = readStatic("terminal.js");

assert.match(
  appHtml,
  /id="desktop-terminal-soft-keyboard-input" type="checkbox" checked/,
  "the desktop soft-keyboard setting should render enabled before settings load",
);
assert.match(
  appHtml,
  /默认开启。终端页会在桌面浏览器显示两行特殊按键/,
  "the settings help text should describe the enabled default",
);
assert.match(
  appSource,
  /desktopTerminalSoftKeyboardEnabled:\s*true/,
  "the settings page should use the enabled optimistic default",
);
assert.match(
  settingsEventBindingsSource,
  /desktopTerminalSoftKeyboardInputEl\.checked = true;/,
  "restoring settings defaults should enable the desktop soft keyboard",
);
assert.match(
  terminalSource,
  /desktopTerminalSoftKeyboardEnabled:\s*true/,
  "the terminal should remain enabled when settings are temporarily unavailable",
);
assert.match(appHtml, /app-settings-event-bindings\.js\?v=20260730a/);
assert.match(appHtml, /app\.js\?v=20260806a/);
assert.match(terminalHtml, /terminal\.js\?v=20260810a/);
