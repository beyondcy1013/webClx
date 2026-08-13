import assert from "node:assert/strict";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appJs = readEntryScriptBundle("index.html");

assert.match(
  appJs,
  /function buildFreshTerminalUrl\(path\) \{[\s\S]*?fresh: true[\s\S]*?quickStart: true/,
  "fresh terminal URLs should request a new session and default quick start",
);

assert.match(
  appJs,
  /async function openFreshTerminalSession\([\s\S]*?runCommand = ""[\s\S]*?const requestedPath =[\s\S]*?requestJson\("\/api\/terminal\/sessions"[\s\S]*?method: "POST"[\s\S]*?path: requestedPath[\s\S]*?window\.location\.assign\([\s\S]*?buildTerminalUrl\(session\.path \|\| requestedPath, session\.id, \{[\s\S]*?quickStart,[\s\S]*?runCommand: command,[\s\S]*?\}\)/,
  "fresh terminal clicks should create a concrete backend session before navigating",
);

assert.match(
  appJs,
  /function openFreshTerminalRunLink\(event, path, command[\s\S]*?event\.preventDefault\(\)[\s\S]*?openFreshTerminalSession\(path, \{[\s\S]*?runCommand: command,[\s\S]*?quickStart: false,[\s\S]*?\}\)/,
  "fresh terminal run links should create a concrete backend session before opening the terminal",
);

assert.match(
  appJs,
  /function openWorkspaceHistoryTerminal\(path\) \{[\s\S]*?resolveWorkspaceHistoryPath\(path\)[\s\S]*?openFreshTerminalSession\(absolutePath\)/,
  "workspace history terminal action should open a fresh terminal for the selected directory",
);

assert.match(
  appJs,
  /const parentTerminalAction = createActionLink\("终端", buildFreshTerminalUrl\(directory\.parent_path\), "mini-button accent"\)/,
  "parent directory terminal action should open a fresh terminal in that directory",
);

assert.match(
  appJs,
  /const terminalAction = createActionLink\("终端", buildFreshTerminalUrl\(entry\.path\), "mini-button accent"\)/,
  "directory row terminal action should open a fresh terminal in that directory",
);
