import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalMobileKeysJs = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const terminalSessionRenderJs = readFileSync(
  new URL("../static/terminal-session-render.js", import.meta.url),
  "utf8",
);
const terminalToolsJs = readFileSync(
  new URL("../static/terminal-tools.js", import.meta.url),
  "utf8",
);
const terminalFocusSelectionJs = readFileSync(
  new URL("../static/terminal-focus-selection.js", import.meta.url),
  "utf8",
);
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);
const terminalToolsMenuStart = terminalHtml.indexOf('id="terminal-tools-menu"');
const terminalToolsMenuEnd = terminalHtml.indexOf("</div>", terminalToolsMenuStart);
const terminalToolsMenu = terminalHtml.slice(terminalToolsMenuStart, terminalToolsMenuEnd);
const terminalSoftKeyboardStart = terminalHtml.indexOf('id="terminal-mobile-keys"');
const terminalSoftKeyboardEnd = terminalHtml.indexOf('id="terminal-fab"', terminalSoftKeyboardStart);
const terminalSoftKeyboard = terminalHtml.slice(terminalSoftKeyboardStart, terminalSoftKeyboardEnd);

assert.match(
  terminalHtml,
  /id="terminal-context-menu"[^>]*role="menu"[\s\S]*?id="terminal-context-copy-all"[^>]*role="menuitem"[\s\S]*?>\s*复制全部文本\s*</,
  "terminal page should expose a copy-all item in its context menu",
);

assert.match(
  terminalToolsMenu,
  /id="terminal-codex-full-access-toggle"[\s\S]*?id="terminal-quick-command-buttons"[\s\S]*?id="terminal-copy-all"[\s\S]*?data-action="copy_all_text"[\s\S]*?>\s*复制全部\s*<\/button>[\s\S]*?id="session-detail-toggle"/,
  "terminal tools should place full access first, followed by quick commands and copy-all",
);

assert.match(
  terminalToolsMenu,
  /id="terminal-sort-directory-sessions"[\s\S]*?data-action="sort_directory_sessions_by_path"[\s\S]*?title="依次按工作区、Agent 类型、状态排序"[\s\S]*?>\s*切换终端排序\s*<\/button>/,
  "terminal tools should expose a button that cycles terminal-list sorting",
);

assert.doesNotMatch(
  terminalSoftKeyboard,
  /id="terminal-quick-command-buttons"|id="terminal-copy-all"/,
  "terminal soft keyboard should no longer expose quick commands or copy-all beside the tools button",
);

assert.doesNotMatch(
  terminalHtml.match(/<header class="topbar slim compact terminal-control-bar">[\s\S]*?<\/header>/)?.[0] || "",
  /id="terminal-copy-all"/,
  "terminal toolbar should no longer contain the copy-all button",
);

assert.match(
  terminalFocusSelectionJs,
  /function readTerminalAllText\(\)[\s\S]*?activeBuffer\.length[\s\S]*?activeBuffer\.getLine\(index\)[\s\S]*?line\.isWrapped[\s\S]*?lines\[lines\.length - 1\] \+= text[\s\S]*?return lines\.join\("\\n"\)/,
  "copy-all should read every active buffer line and merge xterm soft-wrapped rows",
);

const readAllStart = terminalFocusSelectionJs.indexOf("function readTerminalAllText()");
const readAllEnd = terminalFocusSelectionJs.indexOf("\nfunction readTerminalVisibleText()", readAllStart);
assert.ok(readAllStart >= 0 && readAllEnd > readAllStart, "copy-all helper should be extractable for execution");
const readTerminalAllText = Function(
  "term",
  `${terminalFocusSelectionJs.slice(readAllStart, readAllEnd)}; return readTerminalAllText;`,
)({
  buffer: {
    active: {
      length: 4,
      getLine(index) {
        return [
          { isWrapped: false, translateToString: () => "first " },
          { isWrapped: true, translateToString: () => "continued " },
          { isWrapped: false, translateToString: () => "second   " },
          { isWrapped: false, translateToString: () => "" },
        ][index];
      },
    },
  },
});
assert.equal(
  readTerminalAllText(),
  "first continued\nsecond",
  "copy-all should preserve logical lines while removing terminal padding and trailing blank rows",
);

assert.match(
  terminalFocusSelectionJs,
  /function handleTerminalContextMenuSelection\(event\) \{[\s\S]*?terminalContextMenuEventIsTouch\(event\)[\s\S]*?startTerminalTouchSelection\(event\)[\s\S]*?openTerminalContextMenu\(event\.clientX, event\.clientY\)/,
  "terminal contextmenu should preserve touch selection while opening the custom menu for desktop input",
);

assert.match(
  terminalJs,
  /async function copyTerminalAllText\(\) \{[\s\S]*?readTerminalAllText\(\)[\s\S]*?copyTextToClipboard\(text\)[\s\S]*?已复制终端全部文本/,
  "the shared copy-all action should copy the full buffer and report success",
);

assert.match(
  terminalJs,
  /terminalContextCopyAllButton\.addEventListener\("click", copyTerminalAllText\);/,
  "the context menu item should call the shared copy-all function",
);

assert.match(
  terminalMobileKeysJs,
  /if \(button\.dataset\.action === "copy_all_text"\) \{[\s\S]*?copyTerminalAllText\(\);[\s\S]*?return;/,
  "the soft-keyboard copy-all button should call the shared copy-all function",
);

assert.match(
  terminalMobileKeysJs,
  /if \(command\.action === "sort_directory_sessions_by_path"\) \{[\s\S]*?cycleTerminalSessionSortMode\(\)[\s\S]*?sharedNextTerminalSessionSortMode\(mode\)[\s\S]*?再次调用/,
  "the terminal-tools sort action should advance to the next terminal-list sort mode",
);

assert.match(
  terminalMobileKeysJs,
  /if \(button\.dataset\.action === "sort_directory_sessions_by_path"\) \{[\s\S]*?runTerminalFunctionCommand\(\{ action: button\.dataset\.action \}\);[\s\S]*?return;/,
  "the terminal-tools directory-sort button should route through the function-command dispatcher",
);

assert.match(
  terminalSessionRenderJs,
  /function cycleTerminalSessionSortMode\(\) \{[\s\S]*?sharedNextTerminalSessionSortMode\(state\.sessionSortMode\)[\s\S]*?function renderSessions\(\)[\s\S]*?syncTerminalSessionSortControl\(\)/,
  "the terminal renderer should cycle modes and keep the sort control synchronized during refreshes",
);

assert.match(
  terminalToolsJs,
  /function handleTerminalToolsMenuAction\(event\)[\s\S]*?triggerMobileKey\(button\);[\s\S]*?closeTerminalToolsMenu\(\);/,
  "terminal-tools menu actions should reuse the existing command dispatcher and close the menu",
);

assert.match(
  terminalHtml,
  /terminal-session-render\.js\?v=20260731a/,
  "terminal html should refresh the terminal-session-render asset after adding sort cycling",
);

assert.match(
  terminalJs,
  /document\.addEventListener\("pointerdown", closeTerminalContextMenuFromOutside, true\);[\s\S]*?document\.addEventListener\("keydown", handleTerminalContextMenuKeydown, true\);[\s\S]*?window\.addEventListener\("blur", closeTerminalContextMenu\)/,
  "the terminal context menu should close on outside interaction, Escape, and window blur",
);

assert.match(
  terminalStyles,
  /\.terminal-context-menu \{[\s\S]*?position: fixed;[\s\S]*?z-index: 100;[\s\S]*?\.terminal-context-menu-item/,
  "terminal context menu should be a fixed, styled overlay",
);
