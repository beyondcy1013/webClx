import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../static/app-workspace-history.js", import.meta.url), "utf8");
const html = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

assert.match(
  source,
  /function copyWorkspaceHistorySessionId\(sessionId, button\)[\s\S]*?navigator\.clipboard\?\.writeText[\s\S]*?writeText\(value\)\.then\(markCopied, copyWithFallback\)/,
  "workspace history should copy the full Session ID through the Clipboard API",
);

assert.match(
  source,
  /const copyWithFallback = \(\) => \{[\s\S]*?copyTextWithHiddenTextarea\(value\)[\s\S]*?markCopied\(\)/,
  "workspace history should fall back to hidden-textarea copying",
);

assert.match(
  source,
  /if \(item\.sessionId\) \{[\s\S]*?createActionButton\("复制 ID"[\s\S]*?copyWorkspaceHistorySessionId\(item\.sessionId, copySessionIdButton\)[\s\S]*?actionCell\.appendChild\(copySessionIdButton\)/,
  "rows with a Session ID should expose a copy button in the action column",
);

assert.match(
  html,
  /app-workspace-history\.js\?v=20260801b/,
  "the workspace history script should use the updated cache key",
);

console.log("workspace history Session ID copy tests passed");
