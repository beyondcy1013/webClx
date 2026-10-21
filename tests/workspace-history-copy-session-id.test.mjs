import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../static/app-workspace-history.js", import.meta.url), "utf8");
const html = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const stylesBase = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");
const stylesResponsive = readFileSync(new URL("../static/styles-responsive.css", import.meta.url), "utf8");

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
  /sessionValue\.textContent = sessionId;[\s\S]*?createActionButton\("复制"[\s\S]*?copyWorkspaceHistorySessionId\(sessionId, copySessionIdButton\)[\s\S]*?sessionCell\.append\(sessionValue, copySessionIdButton\)/,
  "rows should display the full Session ID and expose a copy button in the same cell",
);

assert.doesNotMatch(
  source,
  /sessionCell\.textContent = item\.sessionId \? shortResumeId\(item\.sessionId\)/,
  "workspace history should not shorten the visible Session ID",
);

for (const styles of [stylesBase, stylesResponsive]) {
  assert.match(
    styles,
    /\.workspace-history-table\s*\{[\s\S]*?min-width:\s*1360px;/,
    "workspace history should preserve its full-width columns and scroll instead of shrinking",
  );
}

assert.match(
  stylesBase,
  /\.workspace-history-table th:nth-child\(4\),[\s\S]*?\.workspace-history-table td:nth-child\(4\)[\s\S]*?width:\s*360px;[\s\S]*?min-width:\s*360px;/,
  "the Session ID column should be wide enough for a full UUID without shrinking",
);

assert.match(
  html,
  /styles-base\.css\?v=20260818a[\s\S]*styles-responsive\.css\?v=20260818a[\s\S]*app-workspace-history\.js\?v=20260818a/,
  "the workspace history script should use the updated cache key",
);

console.log("workspace history Session ID copy tests passed");
