import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appJs = readEntryScriptBundle("index.html");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

function functionSource(source, name) {
  const functionStart = source.indexOf(`function ${name}(`);
  assert.notEqual(functionStart, -1, `missing function ${name}`);
  const asyncStart = source.lastIndexOf("async ", functionStart);
  const start = asyncStart >= 0 && source.slice(asyncStart, functionStart) === "async "
    ? asyncStart
    : functionStart;
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

assert.match(
  appJs,
  /const WORKSPACE_HISTORY_DETAIL_CONCURRENCY = \d+;/,
  "history detail hydration must declare a bounded concurrency limit",
);

assert.match(
  appJs,
  /async function runWorkspaceHistoryWithConcurrency\(items, worker\)[\s\S]*while \(cursor < items\.length\)[\s\S]*const item = items\[cursor\];[\s\S]*cursor \+= 1;[\s\S]*await worker\(item\);/,
  "history detail hydration must pull items through a bounded worker pool",
);

const concurrency = Number(appJs.match(/const WORKSPACE_HISTORY_DETAIL_CONCURRENCY = (\d+);/)?.[1]);
assert.ok(concurrency >= 1 && concurrency <= 8, "detail concurrency should stay in a sane band");

assert.match(
  appJs,
  /async function hydrateWorkspaceHistoryTerminalSessionIds\([\s\S]*runWorkspaceHistoryWithConcurrency\(terminalSessions, async \(session\) =>[\s\S]*requestJson\(\s*`\/api\/terminal\/sessions\/\$\{encodeURIComponent\(session\.id\)\}\/agent-session`,\s*\{ signal \},/,
  "agent-session hydration must run with bounded concurrency and pass the abort signal",
);

assert.match(
  appJs,
  /async function hydrateWorkspaceHistoryTerminalInputHistory\([\s\S]*runWorkspaceHistoryWithConcurrency\(terminalSessions, async \(session\) =>[\s\S]*requestJson\(\s*`\/api\/terminal\/sessions\/\$\{encodeURIComponent\(session\.id\)\}\/input-history`,\s*\{ signal \},/,
  "input-history hydration must run with bounded concurrency and pass the abort signal",
);

// 过期请求所有权：新刷新必须中止旧的详情请求，且 token 守卫仍在。
assert.match(
  appJs,
  /const requestToken = \+\+state\.codexConversationRequestToken;[\s\S]*state\.workspaceHistoryDetailAbortController\?\.abort\(\);[\s\S]*const detailAbortController = new AbortController\(\);[\s\S]*state\.workspaceHistoryDetailAbortController = detailAbortController;/,
  "a new history refresh must abort in-flight detail requests from the previous refresh",
);

assert.match(
  appJs,
  /return await requestJson\(url, \{ signal: detailAbortController\.signal \}\);/,
  "core history requests must be abortable by the owning refresh",
);

for (const guard of ["agent_session_id", "input_history_text"]) {
  const hydrateName = guard === "agent_session_id"
    ? "hydrateWorkspaceHistoryTerminalSessionIds"
    : "hydrateWorkspaceHistoryTerminalInputHistory";
  assert.match(
    functionSource(appJs, hydrateName),
    /requestToken !== state\.codexConversationRequestToken/,
    `${hydrateName} must keep the stale-token guard`,
  );
}

// 归档补写保持串行队列且不受中止影响（保留写入顺序）。
const persistSource = functionSource(appJs, "persistWorkspaceHistoryTerminalArchiveNow");
assert.match(
  persistSource,
  /requestJson\("\/api\/terminal\/resume-archives", \{[\s\S]*method: "POST"/,
  "archive persistence must remain a serialized POST",
);
assert.doesNotMatch(
  persistSource,
  /signal/,
  "archive persistence POST must not be aborted mid-write",
);

// 静态加载版本必须更新，避免浏览器缓存旧脚本。
assert.match(
  indexHtml,
  /app-workspace-history\.js\?v=20\d{6}[a-z]/,
  "index.html should pin a versioned app-workspace-history.js asset",
);
assert.doesNotMatch(
  indexHtml,
  /app-workspace-history\.js\?v=20260916a/,
  "index.html must not keep the pre-change app-workspace-history.js version",
);

// 行为验证：并发上限 + 全量覆盖。
const runWithConcurrency = vm.runInNewContext(
  `(${functionSource(appJs, "runWorkspaceHistoryWithConcurrency")})`,
  { WORKSPACE_HISTORY_DETAIL_CONCURRENCY: concurrency },
);

{
  const items = Array.from({ length: 25 }, (_, index) => index);
  let inFlight = 0;
  let peakInFlight = 0;
  const processed = [];
  await runWithConcurrency(items, async (item) => {
    inFlight += 1;
    peakInFlight = Math.max(peakInFlight, inFlight);
    await new Promise((resolve) => setTimeout(resolve, 1));
    processed.push(item);
    inFlight -= 1;
  });
  assert.equal(peakInFlight, concurrency, "worker pool must cap concurrent detail requests");
  assert.deepEqual(processed, items, "worker pool must process every item exactly once");
}

{
  const processed = [];
  await runWithConcurrency([], async (item) => processed.push(item));
  assert.deepEqual(processed, [], "empty session list must not spawn workers");
}

console.log("workspace-history-detail-concurrency: all assertions passed");
