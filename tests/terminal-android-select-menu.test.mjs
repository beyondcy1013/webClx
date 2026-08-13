import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const adapter = readFileSync(
  new URL("../static/terminal-android-select-menu.js", import.meta.url),
  "utf8",
);
const styles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);

assert.match(
  terminalHtml,
  /terminal-android-select-menu\.js\?v=20260804b[\s\S]*id="terminal-android-select-menu"[\s\S]*role="menu"/,
  "the terminal page should load one shared Android select command menu",
);

assert.match(
  indexHtml,
  /styles-terminal\.css\?v=20260804b[\s\S]*terminal-android-select-menu\.js\?v=20260804b[\s\S]*id="favorite-path-select"[\s\S]*id="workspace-history-path-select"[\s\S]*id="terminal-android-select-menu"[\s\S]*role="menu"/,
  "workspace and workspace history selects should use the shared Android command menu",
);

assert.match(
  adapter,
  /\bAndroid\b[\s\S]*WebClxAndroid[\s\S]*document\.addEventListener\("pointerdown"[\s\S]*event\.pointerType !== "touch"/,
  "the adapter should intercept touch selection only for Android clients",
);

assert.match(
  adapter,
  /document\.createElement\("button"\)[\s\S]*setAttribute\("role", "menuitem"\)[\s\S]*select\.dispatchEvent\(new Event\("input"[\s\S]*select\.dispatchEvent\(new Event\("change"/,
  "native options should become command buttons while preserving select events",
);

assert.match(
  adapter,
  /select\.closest\("dialog"\) \|\| document\.body[\s\S]*menuHost\.appendChild\(menu\)/,
  "a select inside a modal dialog should keep its command menu in the top layer",
);

assert.doesNotMatch(
  adapter,
  /type\s*=\s*["']radio["']|role["']?,\s*["']radio|menuitemradio/,
  "the Android command menu should not create radio controls",
);

assert.match(
  styles,
  /\.terminal-android-select-menu\s*\{[\s\S]*position:\s*fixed[\s\S]*overflow-y:\s*auto[\s\S]*\.terminal-android-select-menu > button\s*\{[\s\S]*min-height:\s*34px/,
  "the Android select replacement should be a compact scrollable command menu",
);
