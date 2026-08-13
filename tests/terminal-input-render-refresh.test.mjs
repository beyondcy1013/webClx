import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const navigationLayoutJs = readFileSync(
  new URL("../static/terminal-navigation-layout.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

const functionStart = navigationLayoutJs.indexOf(
  "function refreshTerminalInputVisibilityAfterUserInput()",
);
const functionEnd = navigationLayoutJs.indexOf("// ===== FAB menu toggle =====", functionStart);
assert.ok(functionStart >= 0 && functionEnd > functionStart, "input visibility refresh helper should exist");

const inputVisibilityRefresh = navigationLayoutJs.slice(functionStart, functionEnd);
assert.match(
  inputVisibilityRefresh,
  /const refresh = \(\) => \{[\s\S]*scheduleTerminalRenderRefresh\(\);[\s\S]*scrollTerminalToBottom\(\);/,
  "ordinary typing should force an xterm viewport repaint before restoring bottom visibility",
);

assert.match(
  terminalHtml,
  /terminal-navigation-layout\.js\?v=20260803b/,
  "terminal page should invalidate cached navigation layout code after the typing repaint fix",
);
