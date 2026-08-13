import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalSettingsJs = readFileSync(
  new URL("../static/terminal-settings.js", import.meta.url),
  "utf8",
);
const appJs = readEntryScriptBundle("index.html");

const context = {
  module: { exports: {} },
  exports: {},
};
vm.runInNewContext(terminalSettingsJs, context);

const settings = context.module.exports;

assert.equal(settings.DEFAULT_TERMINAL_USER, "root");
assert.equal(settings.normalizeTerminalUser(" beyondcy "), "beyondcy");
assert.equal(settings.normalizeTerminalUser(""), "root");
assert.equal(settings.normalizeTerminalUser(null), "root");

assert.match(
  appJs,
  /DEFAULT_TERMINAL_USER,[\s\S]*normalizeTerminalUser,[\s\S]*= globalThis\.WebClxTerminalSettings/,
  "settings page should import terminal user defaults and normalization from shared settings",
);

assert.match(
  appJs,
  /function normalizeAvailableUsers\(users, selectedUser = DEFAULT_TERMINAL_USER\)[\s\S]*function renderTerminalUserOptions\(users, selectedUser = state\.terminalUser\)/,
  "settings page should define terminal user list normalization and select rendering",
);

assert.match(
  appJs,
  /state\.terminalUser = normalizeTerminalUser\(settings\.terminal_user\);[\s\S]*state\.availableUsers = normalizeAvailableUsers\(settings\.available_users, state\.terminalUser\);[\s\S]*renderTerminalUserOptions\(state\.availableUsers, state\.terminalUser\);/,
  "settings loader should normalize terminal users before rendering the select",
);

[
  "formatTerminalSoftKeyboardScale",
  "readTerminalSoftKeyboardScaleFromInput",
  "formatTerminalFloatingButtonOffsetVh",
  "readTerminalFloatingButtonOffsetVhFromInput",
  "formatTerminalTouchSelectionLongPressMs",
  "readTerminalTouchSelectionLongPressMsFromInput",
  "formatFontSizeTier",
  "readFontSizeTiersFromInputs",
].forEach((name) => {
  assert.match(
    appJs,
    new RegExp(`function ${name}\\(`),
    `settings page should define ${name}`,
  );
});
