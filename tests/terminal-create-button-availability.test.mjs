import assert from "node:assert/strict";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalJs = readEntryScriptBundle("terminal.html");

const loadSessionsStart = terminalJs.indexOf("async function loadSessions(");
const createSessionStart = terminalJs.indexOf("async function createSession(", loadSessionsStart);
const deleteSessionStart = terminalJs.indexOf("async function deleteSession(", createSessionStart);
const viewportLayoutStart = terminalJs.indexOf(
  "function refreshTerminalViewportLayout(",
  deleteSessionStart,
);

assert.notEqual(loadSessionsStart, -1, "terminal page should define loadSessions");
assert.notEqual(createSessionStart, -1, "terminal page should define createSession");
assert.notEqual(deleteSessionStart, -1, "terminal page should define deleteSession");
assert.notEqual(viewportLayoutStart, -1, "terminal page should define refreshTerminalViewportLayout");

const loadSessionsBody = terminalJs.slice(loadSessionsStart, createSessionStart);
const createSessionBody = terminalJs.slice(createSessionStart, deleteSessionStart);
const deleteSessionBody = terminalJs.slice(deleteSessionStart, viewportLayoutStart);
const createButtonHandlerStart = terminalJs.indexOf(
  'createSessionButton.addEventListener("click", () => {',
);
const createButtonHandlerEnd = terminalJs.indexOf("\n});", createButtonHandlerStart);
const createButtonHandlerBody = terminalJs.slice(createButtonHandlerStart, createButtonHandlerEnd);
const syncCreateSessionButtonStart = terminalJs.indexOf("function syncCreateSessionButton()");
const syncCreateSessionButtonEnd = terminalJs.indexOf("function ", syncCreateSessionButtonStart + 1);
const syncCreateSessionButtonBody = terminalJs.slice(
  syncCreateSessionButtonStart,
  syncCreateSessionButtonEnd,
);

assert.notEqual(
  syncCreateSessionButtonStart,
  -1,
  "terminal page should define a shared new-terminal button state sync helper",
);

assert.doesNotMatch(
  loadSessionsBody,
  /createSessionButton\.disabled\s*=/,
  "background session list refreshes should not gray out the new terminal button",
);

assert.match(
  syncCreateSessionButtonBody,
  /createSessionButton\.disabled\s*=\s*state\.creatingSession \|\| state\.initialTerminalIntentPending/,
  "the shared new-terminal button sync helper should disable the button while a create request is in flight",
);

assert.match(
  createSessionBody,
  /state\.creatingSession\s*=\s*true;[\s\S]*syncCreateSessionButton\(\);[\s\S]*state\.creatingSession\s*=\s*false;[\s\S]*syncCreateSessionButton\(\);/,
  "creating a terminal should still drive the shared disabled state while the create request is in flight",
);

assert.match(
  deleteSessionBody,
  /createSessionButton\.disabled\s*=\s*true[\s\S]*createSessionButton\.disabled\s*=\s*false/,
  "deleting a terminal should still disable the new button while the delete request is in flight",
);

assert.notEqual(createButtonHandlerStart, -1, "terminal page should bind the new-terminal button");
assert.match(
  createButtonHandlerBody,
  /createSession\(\{\s*enableQuickStart:\s*true,?\s*\}\)/,
  "the toolbar new-terminal button should retain direct quick-start behavior",
);
assert.match(
  createButtonHandlerBody,
  /openWithBehavior[\s\S]*openDialog:\s*\(\)\s*=>\s*openTerminalNewSessionDialog\(createSessionButton\)/,
  "the toolbar new-terminal button should retain its new-session dialog",
);

const newSessionDialogStart = terminalJs.indexOf("async function openTerminalNewSessionDialog(");
const newSessionDialogEnd = terminalJs.indexOf("\n}\n\n// 指定预设", newSessionDialogStart);
const newSessionDialogBody = terminalJs.slice(newSessionDialogStart, newSessionDialogEnd);
assert.match(
  newSessionDialogBody,
  /temporary:\s*options\.temporary === undefined \? false : options\.temporary/,
  "the new-session dialog should launch the selected agent directly after applying its preset",
);
