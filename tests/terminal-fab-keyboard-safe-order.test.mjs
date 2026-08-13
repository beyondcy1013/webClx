import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);
const terminalNavigationLayout = readFileSync(
  new URL("../static/terminal-navigation-layout.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(
  new URL("../static/terminal.html", import.meta.url),
  "utf8",
);

assert.match(
  terminalHtml,
  /id="terminal-fab-top-menu"[\s\S]*?id="terminal-schedule-button"[\s\S]*?id="scroll-terminal-top"[\s\S]*?id="terminal-fab"[\s\S]*?id="terminal-fab-menu"[\s\S]*?id="terminal-input-history-button"[\s\S]*?id="scroll-terminal-bottom"[\s\S]*?id="terminal-soft-keyboard-toggle"/,
  "FAB actions should split top/schedule from history/bottom/keyboard",
);

assert.match(
  terminalStyles,
  /\.terminal-fab \{[\s\S]*?top: calc\([\s\S]*?--terminal-visible-viewport-top[\s\S]*?--terminal-visible-viewport-height[\s\S]*?--terminal-floating-bottom-offset[\s\S]*?--terminal-scroll-top-offset[\s\S]*?bottom: auto;[\s\S]*?transform: translateY\(-100%\);/,
  "FAB actions should rise from the current visible viewport bottom",
);

assert.match(
  terminalStyles,
  /\.terminal-fab-top \{[\s\S]*?top: calc\([\s\S]*?--terminal-output-visible-top[\s\S]*?--terminal-visible-viewport-top[\s\S]*?--terminal-page-nav-height[\s\S]*?\+ 4px[\s\S]*?bottom: auto;/,
  "schedule and jump-top should stay inside a separate terminal-output-top group",
);

assert.match(
  terminalNavigationLayout,
  /const terminalOutputTop = Math\.max\([\s\S]*?terminalHost\?\.getBoundingClientRect\(\)\.top[\s\S]*?viewportBounds\.top[\s\S]*?--terminal-output-visible-top/,
  "FAB layout should publish the visible terminal output top instead of using the page toolbar",
);

assert.match(
  terminalNavigationLayout,
  /const fabMenus = \[fabTopMenu, fabMenu\][\s\S]*?fabMenus\.forEach\(\(menu\) => \{[\s\S]*?menu\.hidden = !expanded/,
  "the FAB toggle should expand and collapse both groups together",
);

assert.match(
  terminalHtml,
  /styles-terminal\.css\?v=20260804b/,
  "terminal page should refresh the keyboard-safe FAB styles",
);

assert.match(
  terminalHtml,
  /terminal-navigation-layout\.js\?v=20260803b/,
  "terminal page should refresh the output-anchored FAB layout script",
);

console.log("terminal FAB keyboard-safe order checks passed");
