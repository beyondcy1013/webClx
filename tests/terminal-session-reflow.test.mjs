import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalStaticDir = new URL("../static/", import.meta.url);
const terminalJs = readFileSync(new URL("terminal.js", terminalStaticDir), "utf8");
const layoutConnectionJs = readFileSync(
  new URL("terminal-layout-connection.js", terminalStaticDir),
  "utf8",
);

assert.match(
  terminalJs,
  /const TERMINAL_SESSION_REFLOW_STORAGE_KEY = "webclx:terminal-session-reflow-sessions";[\s\S]*function readStoredTerminalSessionReflowIds\(\) \{[\s\S]*return new Set\(\);[\s\S]*\}[\s\S]*function storeTerminalSessionReflowIds\(sessionIds\) \{[\s\S]*window\.localStorage\.setItem\(\s*TERMINAL_SESSION_REFLOW_STORAGE_KEY,/,
  "per-session mobile reflow preference should persist across reloads",
);

assert.match(
  terminalJs,
  /terminalSessionReflowIds: readStoredTerminalSessionReflowIds\(\),/,
  "terminal state should load persisted per-session reflow preferences",
);

assert.match(
  layoutConnectionJs,
  /function terminalSessionReflowEnabled\(context = activeTerminalContext\) \{[\s\S]*state\.terminalSessionReflowIds\.has\(sessionId\)/,
  "reflow should be evaluated for the exact terminal context",
);

assert.match(
  layoutConnectionJs,
  /function applyTerminalSessionReflow\(context = activeTerminalContext\) \{[\s\S]*const isForeground = context === activeTerminalContext;[\s\S]*context\.canonicalTerminalSize = null;[\s\S]*term\.resize\(proposed\.cols, proposed\.rows\);[\s\S]*lastTerminalSize = proposed;[\s\S]*sendMessage\(\{[\s\S]*type: "resize",[\s\S]*cols: proposed\.cols,[\s\S]*rows: proposed\.rows,[\s\S]*\}, context\);[\s\S]*applyTerminalCanonicalViewportLayout\(context\);/,
  "per-session reflow should use the exact context while updating shared active aliases before sending resize",
);

assert.match(
  layoutConnectionJs,
  /function terminalReflowCurrentSession\(\) \{[\s\S]*state\.terminalSessionReflowIds\.add\(sessionId\);[\s\S]*storeTerminalSessionReflowIds\(state\.terminalSessionReflowIds\);[\s\S]*applyTerminalSessionReflow\(context\);[\s\S]*updateStatus\("已按当前手机宽度重排，并记住该终端的当前视口。", "ok"\);/,
  "manual reflow should persist and announce the active session preference",
);

assert.match(
  layoutConnectionJs,
  /function applyTerminalCanonicalViewportSize\(size, context = activeTerminalContext\) \{[\s\S]*if \(terminalSessionReflowEnabled\(context\)\) \{[\s\S]*applyTerminalSessionReflow\(context\);[\s\S]*return;[\s\S]*\}[\s\S]*context\.canonicalTerminalSize = canonical;/,
  "backend canonical size must not override a session-local mobile reflow preference",
);

assert.match(
  layoutConnectionJs,
  /if \(terminalSessionReflowEnabled\(context\)\) \{[\s\S]*applyTerminalSessionReflow\(context\);[\s\S]*\} else if \(context\?\.canonicalTerminalSize\) \{[\s\S]*fitAddon\.fit\(\);[\s\S]*\}/,
  "connection fitting should preserve session-local reflow after switching or reconnecting",
);
