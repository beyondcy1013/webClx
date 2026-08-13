import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const baseStyles = readFileSync(
  new URL("../static/styles-base.css", import.meta.url),
  "utf8",
);
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);
const terminalNavigationLayoutJs = readFileSync(
  new URL("../static/terminal-navigation-layout.js", import.meta.url),
  "utf8",
);
const terminalJs = readFileSync(
  new URL("../static/terminal.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(
  new URL("../static/terminal.html", import.meta.url),
  "utf8",
);

assert.match(
  baseStyles,
  /--terminal-floating-right-offset:\s*0px;/,
  "terminal floating actions should use a zero default right inset",
);

assert.match(
  terminalNavigationLayoutJs,
  /function syncTerminalFloatingButtonRight\(\) \{[\s\S]*?--terminal-floating-right-offset[\s\S]*?"0px"/,
  "runtime terminal floating-action alignment should keep the zero right inset",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-group \{[\s\S]*?right: var\(--terminal-floating-right-offset\);/,
  "both terminal FAB groups should use the shared right inset",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-menu:not\(\[hidden\]\) \{[\s\S]*?position: static;/,
  "jump actions should inherit the shared right inset from their FAB container",
);

assert.match(
  terminalHtml,
  /id="terminal-fab-top-menu"[\s\S]*?id="terminal-schedule-button"[\s\S]*?id="scroll-terminal-top"[\s\S]*?id="terminal-fab"/,
  "schedule and jump-top should share the viewport-top FAB group in the requested order",
);

assert.match(
  terminalHtml,
  /id="terminal-fab-menu"[\s\S]*?id="terminal-input-history-button"[\s\S]*?id="scroll-terminal-bottom"[\s\S]*?id="terminal-soft-keyboard-toggle"/,
  "history, jump-bottom, and keyboard should share the keyboard-safe FAB group in the requested order",
);

assert.match(
  terminalHtml,
  /id="terminal-schedule-button"[\s\S]*?data-action="open_schedule_paste"[\s\S]*?id="terminal-paste-schedule-chip"[\s\S]*?id="terminal-paste-schedule-chip-text"[\s\S]*?定时 0\/0/,
  "the merged action group should include the existing scheduled-message workflow and task counts",
);

assert.doesNotMatch(
  terminalHtml,
  /class="terminal-scroll-shell"[\s\S]*?id="terminal-paste-schedule-chip"[\s\S]*?id="terminal-host"/,
  "the schedule status should no longer float separately inside the terminal viewport",
);

assert.match(
  terminalStyles,
  /\.terminal-paste-schedule-chip \{[\s\S]*?display: inline-flex;[\s\S]*?max-width: 100%;/,
  "the schedule status should participate in the merged button layout",
);

const scheduleChipStyleBlock = terminalStyles.match(
  /\.terminal-paste-schedule-chip \{([^}]*)\}/,
)?.[1] || "";
assert.doesNotMatch(
  scheduleChipStyleBlock,
  /position:\s*absolute/,
  "the merged schedule status should not keep its former viewport overlay position",
);

assert.match(
  terminalStyles,
  /\.terminal-fab \{[\s\S]*?top: calc\([\s\S]*?--terminal-visible-viewport-top[\s\S]*?--terminal-visible-viewport-height[\s\S]*?--terminal-floating-bottom-offset[\s\S]*?--terminal-scroll-top-offset[\s\S]*?bottom: auto;[\s\S]*?transform: translateY\(-100%\);/,
  "the merged action group should rise from the visible viewport bottom while respecting saved and keyboard offsets",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-menu:not\(\[hidden\]\) \{[\s\S]*?position: static;[\s\S]*?overflow-y: auto;/,
  "the open menu should stay inside the shared FAB container and scroll within the visible viewport when needed",
);

assert.doesNotMatch(
  terminalStyles,
  /\.terminal-fab-menu:not\(\[hidden\]\) \{[\s\S]*?bottom: calc\(/,
  "the open menu should not estimate its position from a fixed item height",
);

assert.match(
  terminalNavigationLayoutJs,
  /--terminal-visible-viewport-top[\s\S]*?--terminal-visible-viewport-height/,
  "viewport resize handling should publish the current visual viewport geometry for the FAB rail",
);

assert.match(
  terminalJs,
  /terminalScheduleButton\.addEventListener\("click", \(\) => \{[\s\S]*?openScheduledTerminalPasteDialog\(\)/,
  "the merged schedule button should open the existing scheduling dialog",
);

assert.match(
  terminalHtml,
  /styles-base\.css\?v=20260727b/,
  "terminal page should refresh its cached base styles",
);

assert.match(
  terminalHtml,
  /styles-terminal\.css\?v=20260804b/,
  "terminal page should refresh its cached floating-action styles",
);

console.log("terminal floating action position checks passed");
