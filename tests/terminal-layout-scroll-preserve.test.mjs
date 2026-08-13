import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalJs = readEntryScriptBundle("terminal.html");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

const pageResumeRefreshStart = terminalJs.indexOf(
  "function refreshTerminalInputVisibilityAfterPageResume()",
);
const pageResumeRefreshEnd = terminalJs.indexOf(
  "function terminalShouldStickToBottomForOutput",
  pageResumeRefreshStart,
);
assert.ok(
  pageResumeRefreshStart >= 0 && pageResumeRefreshEnd > pageResumeRefreshStart,
  "page-resume terminal refresh helper should exist",
);
const pageResumeRefresh = terminalJs.slice(pageResumeRefreshStart, pageResumeRefreshEnd);
assert.match(
  pageResumeRefresh,
  /captureTerminalScrollSnapshotForSession\(sessionId\)[\s\S]*restoreTerminalScrollSnapshot\(scrollSnapshot\)/,
  "returning to the terminal or resizing the Android IME should restore the prior terminal scroll position",
);
assert.doesNotMatch(
  pageResumeRefresh,
  /scrollTerminalToBottom\(|focusTerminalForUserInput\(/,
  "page resume must not force the terminal to the bottom or reopen the Android IME",
);
assert.match(
  terminalHtml,
  /terminal-output-scroll\.js\?v=20260807a/,
  "terminal page should invalidate cached resume scroll behavior",
);

assert.match(
  terminalJs,
  /function scrollTerminalToBottom\(\) \{[\s\S]*const metrics = terminalScrollMetrics\(\);[\s\S]*if \(!metrics\?\.atBottom\) \{[\s\S]*term\.scrollToBottom\(\);[\s\S]*scheduleTerminalViewportDomSync\(\);/,
  "bottom restoration should not ask xterm to scroll when its logical viewport is already at the bottom",
);

assert.match(
  terminalJs,
  /function restorePageScrollSnapshotForLayout\(snapshot\) \{[\s\S]*const currentScrollTop =[\s\S]*if \(maxScroll - currentScrollTop <= TERMINAL_PAGE_SCROLL_BOTTOM_TOLERANCE_PX\) \{[\s\S]*updatePageScrollRail\(\);[\s\S]*return;[\s\S]*\}[\s\S]*window\.scrollTo\(/,
  "page-bottom restoration should avoid redundant window scrolling when the page is already at the bottom",
);

assert.match(
  terminalJs,
  /function fitTerminal\(\{ force = false \} = \{\}\) \{[\s\S]*preserveTerminalScrollDuringLayout\(\(\) => \{[\s\S]*fitAddon\.fit\(\);[\s\S]*syncTerminalSize\(\{ force \}\);[\s\S]*\}\);[\s\S]*\}/,
  "terminal layout fitting should preserve the current session scroll position",
);

assert.match(
  terminalJs,
  /const TERMINAL_RESIZE_FLUSH_DELAY_MS = 40;[\s\S]*let pendingTerminalSize = null;[\s\S]*let terminalSizeFlushTimer = null;/,
  "terminal resize events should have a short coalescing window so layout churn does not queue ahead of input",
);

assert.match(
  terminalJs,
  /const TERMINAL_SIZE_SETTLE_FRAMES = 3;[\s\S]*const TERMINAL_SIZE_SETTLE_INTERVAL_MS = 100;/,
  "terminal size syncing should retry briefly while browser layout and mobile viewport geometry settle",
);

const syncTerminalSizeStart = terminalJs.indexOf("function syncTerminalSize({ force = false } = {})");
const flushTerminalSizeStart = terminalJs.indexOf("function flushTerminalSize()");
assert.ok(
  syncTerminalSizeStart >= 0 &&
    flushTerminalSizeStart > syncTerminalSizeStart &&
    terminalJs.indexOf("pendingTerminalSize = { context, size: nextSize };", syncTerminalSizeStart) > syncTerminalSizeStart &&
    terminalJs.indexOf("window.setTimeout(() => {", syncTerminalSizeStart) > syncTerminalSizeStart &&
    terminalJs.indexOf("flushTerminalSize();", syncTerminalSizeStart) > syncTerminalSizeStart &&
    terminalJs.indexOf("sendMessage({", flushTerminalSizeStart) > flushTerminalSizeStart &&
    terminalJs.indexOf('type: "resize"', flushTerminalSizeStart) > flushTerminalSizeStart,
  "terminal resize messages should be flushed from the coalesced pending size, not sent immediately from every layout event",
);

assert.ok(
  terminalJs.indexOf("pending.context !== activeTerminalContext", flushTerminalSizeStart) > flushTerminalSizeStart &&
    terminalJs.indexOf("pending.context.term !== term", flushTerminalSizeStart) > flushTerminalSizeStart,
  "a delayed resize must not cross a terminal session boundary",
);

const scheduleTerminalSizeSettleStart = terminalJs.indexOf("function scheduleTerminalSizeSettle(");
assert.ok(
  scheduleTerminalSizeSettleStart >= 0 &&
    terminalJs.indexOf("fitTerminal();", scheduleTerminalSizeSettleStart) > scheduleTerminalSizeSettleStart &&
    terminalJs.indexOf("TERMINAL_SIZE_SETTLE_INTERVAL_MS", scheduleTerminalSizeSettleStart) > scheduleTerminalSizeSettleStart,
  "terminal size settle should refit without sending redundant forced PTY resizes",
);

const websocketOpenStartForSize = terminalJs.indexOf('contextSocket.addEventListener("open", async () => {');
assert.ok(
  websocketOpenStartForSize >= 0 &&
    terminalJs.indexOf("fitTerminal({ force: true });", websocketOpenStartForSize) > websocketOpenStartForSize &&
    terminalJs.indexOf("scheduleTerminalSizeSettle();", websocketOpenStartForSize) > websocketOpenStartForSize,
  "terminal websocket open should schedule size settling after the first forced fit",
);

const visualViewportLayoutStart = terminalJs.indexOf("function applyTerminalViewportResizeLayout(");
const visualViewportChangeStart = terminalJs.indexOf("function handleTerminalViewportResize() {");
assert.ok(
  visualViewportLayoutStart >= 0 &&
    visualViewportChangeStart > visualViewportLayoutStart &&
    terminalJs.indexOf("fitTerminal();", visualViewportLayoutStart) > visualViewportLayoutStart &&
    terminalJs.indexOf("scheduleTerminalSizeSettle();", visualViewportLayoutStart) > visualViewportLayoutStart &&
    terminalJs.indexOf("applyTerminalViewportResizeLayout(pageSnapshot, { settle: true });", visualViewportChangeStart) >
      visualViewportChangeStart,
  "mobile visualViewport changes should settle terminal size after the browser finishes viewport animation",
);

assert.match(
  terminalJs,
  /function handleTerminalViewportScroll\(\) \{[\s\S]*!terminalBacklogReplayActive && !terminalScrollSaveSuppressed\(\)[\s\S]*saveTerminalScrollPositionForSession\(state\.activeSessionId\);[\s\S]*\}/,
  "layout-induced viewport scroll events should not overwrite the saved session scroll state",
);

assert.match(
  terminalJs,
  /function preserveTerminalScrollDuringLayout\(layoutCallback\) \{[\s\S]*captureTerminalScrollSnapshotForSession\(state\.activeSessionId\)[\s\S]*suppressTerminalScrollSaveUntilNextFrame\(\);[\s\S]*suppressTerminalScrollSaveForLayout\(\);[\s\S]*layoutCallback\(\);[\s\S]*restoreTerminalScrollSnapshot\(snapshot\);[\s\S]*scheduleTerminalScrollSnapshotRestore\(snapshot\);[\s\S]*\}/,
  "terminal layout should restore the captured scroll position immediately, on the next frame, and through the mobile viewport settle window",
);

assert.match(
  terminalJs,
  /function restoreTerminalScrollSnapshot\(snapshot\) \{[\s\S]*if \(snapshot\.atBottom\) \{[\s\S]*scrollTerminalToBottom\(\);[\s\S]*\} else \{[\s\S]*scrollTerminalToDomScrollTop\(snapshot\.scrollTop, metrics\.maxScroll\);[\s\S]*updateTerminalScrollBottomButton\(\);[\s\S]*\}[\s\S]*saveTerminalScrollPositionForSession\(snapshot\.sessionId\);[\s\S]*\}/,
  "terminal layout scroll restoration should keep either the previous position or the bottom state",
);

assert.match(
  terminalJs,
  /function terminalScrollSaveSuppressed\(\) \{[\s\S]*terminalScrollSaveSuppressionDepth > 0 \|\| Date\.now\(\) < terminalScrollSaveSuppressedUntil[\s\S]*\}/,
  "terminal layout should keep suppressing scroll saves during delayed mobile visualViewport and IME scroll corrections",
);

assert.match(
  terminalJs,
  /function terminalShouldStickToBottomForOutput\(context = activeTerminalContext\) \{[\s\S]*context\.backlogReplayActive[\s\S]*terminalScrollSaveSuppressed\(\)[\s\S]*terminalScrollPositions\.get\(context\.sessionId\)[\s\S]*saved\?\.atBottom[\s\S]*\}/,
  "terminal output should use the saved bottom state while layout-induced scroll changes are being suppressed",
);

assert.match(
  terminalJs,
  /function preservePageScrollDuringLayout\(layoutCallback\) \{[\s\S]*capturePageScrollSnapshotForLayout\(\)[\s\S]*layoutCallback\(\);[\s\S]*restorePageScrollSnapshotForLayout\(snapshot\);[\s\S]*schedulePageScrollSnapshotRestore\(snapshot\);[\s\S]*\}/,
  "terminal layout should preserve the page bottom position while mobile visualViewport and IME changes settle",
);

assert.match(
  terminalJs,
  /const TERMINAL_VIEWPORT_RESIZE_DEBOUNCE_MS = 240;[\s\S]*function applyTerminalViewportResizeLayout\(pageSnapshot,[\s\S]*fitTerminal\(\);[\s\S]*restorePageScrollSnapshotForLayout\(pageSnapshot\);[\s\S]*function handleTerminalViewportResize\(\) \{[\s\S]*terminalViewportResizePageSnapshot = capturePageScrollSnapshotForLayout\(\);[\s\S]*const pageSnapshot = terminalViewportResizePageSnapshot;[\s\S]*requestAnimationFrame\(\(\) => \{[\s\S]*applyTerminalViewportResizeLayout\(pageSnapshot\);[\s\S]*window\.setTimeout\(\(\) => \{[\s\S]*applyTerminalViewportResizeLayout\(pageSnapshot, \{ settle: true \}\);[\s\S]*TERMINAL_VIEWPORT_RESIZE_DEBOUNCE_MS[\s\S]*window\.visualViewport\.addEventListener\("resize", handleTerminalViewportResize\)/,
  "mobile visualViewport changes should fit on the first frame, retain the original page-bottom anchor, and settle once geometry stabilizes",
);
