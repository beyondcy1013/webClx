import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const actions = readFileSync(new URL("../static/app-session-actions.js", import.meta.url), "utf8");
const render = readFileSync(new URL("../static/app-home-session-render.js", import.meta.url), "utf8");
const html = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

assert.match(
  actions,
  /async function hydrateHomeSessionInputHistory\(requestToken, sessions\)[\s\S]*?\/input-history`[\s\S]*?workspaceHistoryInputHistoryText\(payload\.entries \|\| \[\]\)[\s\S]*?renderSessions\(\)/,
  "the terminal management list should enrich every session with filtered conversation history",
);

assert.match(
  actions,
  /renderSessions\(\);\s*void hydrateHomeSessionInputHistory\(requestToken, state\.sessions\)/,
  "base terminal rows should render before conversation history is loaded",
);

assert.match(
  render,
  /const conversationTitle = session\.input_history_text \|\| session\.title \|\| "";[\s\S]*?attachWorkspaceHistoryTooltip\(titleCell, \{[\s\S]*?title: conversationTitle/,
  "the title column should prefer conversation history and expose its full content",
);

assert.match(html, /app-home-session-render\.js\?v=20260801a/);
assert.match(html, /app-session-actions\.js\?v=20260801a/);

console.log("home terminal conversation title tests passed");
