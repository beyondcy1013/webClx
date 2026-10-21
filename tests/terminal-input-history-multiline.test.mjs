import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalDialogsJs = readFileSync(
  new URL("../static/terminal-dialogs.js", import.meta.url),
  "utf8",
);
const terminalFocusSelectionJs = readFileSync(
  new URL("../static/terminal-focus-selection.js", import.meta.url),
  "utf8",
);
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);

const historyForm = terminalHtml.match(/<form id="terminal-input-history-form"[\s\S]*?<\/form>/)?.[0] || "";
assert.ok(historyForm, "terminal input history dialog form should exist");
assert.doesNotMatch(
  historyForm,
  /method="dialog"/,
  "drag selection ending outside must not become implicit dialog submission and close the history dialog",
);

const renderBlock = terminalDialogsJs.match(
  /function renderTerminalInputHistory\([\s\S]*?\nfunction terminalInputHistoryTerminalBaseName/,
)?.[0] || "";
assert.ok(renderBlock, "terminal input history renderer should exist");
assert.match(
  renderBlock,
  /createElement\("textarea"\)/,
  "each input-history entry should render as a multiline text box for safe text selection",
);
assert.match(renderBlock, /\.readOnly = true/);
assert.match(renderBlock, /text\.value = String\(entry\?\.text \|\| ""\)/);
assert.doesNotMatch(
  renderBlock,
  /createElement\("pre"\)/,
  "input-history text must not be a single pre element that participates in the drag-close bug",
);
assert.match(
  terminalStyles,
  /\.terminal-input-history-text\s*\{[\s\S]*?resize:\s*vertical/,
  "the multiline history text box should remain resizable",
);

assert.match(
  renderBlock,
  /text\.addEventListener\("contextmenu"/,
  "right-clicking a history text box should open the terminal copy menu",
);
assert.match(
  renderBlock,
  /text\.selectionStart, text\.selectionEnd/,
  "the history context menu should copy only the current textarea selection when one exists",
);
assert.match(
  renderBlock,
  /event\.currentTarget\.closest\("dialog"\)/,
  "the history context menu must be moved into the dialog top layer so it can be clicked",
);
assert.match(
  terminalFocusSelectionJs,
  /async function copyTerminalContextMenuSelection\(\)/,
  "the terminal context menu should support delegating copy to dialogs",
);
assert.match(
  terminalJs,
  /await copyTerminalContextMenuSelection\(\)/,
  "the context copy button should prefer the active dialog selection action",
);
