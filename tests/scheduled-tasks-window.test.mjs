import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalNavigationJs = readFileSync(
  new URL("../static/terminal-navigation-layout.js", import.meta.url),
  "utf8",
);
const settingsCss = readFileSync(
  new URL("../static/styles-settings.css", import.meta.url),
  "utf8",
);
const navigationTabsJs = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const unifiedTasksJs = readFileSync(
  new URL("../static/app-unified-tasks.js", import.meta.url),
  "utf8",
);

test("terminal opens the shared task manager as an in-page lightweight dialog", () => {
  assert.match(
    terminalNavigationJs,
    /function openScheduledTaskManagement\(\)[\s\S]*?terminal-scheduled-tasks-dialog[\s\S]*?typeof openScheduledTasksDialog === "function"[\s\S]*?openScheduledTasksDialog\(\)[\s\S]*?dialog\?\.showModal\(\)/,
  );
  assert.match(terminalHtml, /terminal-navigation-layout\.js\?v=/);
  assert.match(terminalHtml, /terminal\.js\?v=/);
  assert.match(terminalHtml, /styles-settings\.css\?v=/);
  assert.match(terminalHtml, /terminal-mobile-keys\.js\?v=/);
  assert.match(terminalHtml, /terminal-specified-task\.js\?v=/);
  assert.match(terminalHtml, /terminal-future-tasks\.js\?v=/);
  assert.match(terminalHtml, /specified-preset-actions\.js\?v=/);
  assert.match(terminalHtml, /dialog-backdrop-guard\.js\?v=/);
  assert.doesNotMatch(terminalNavigationJs, /window\.open\(/);
  assert.match(terminalHtml, /id="terminal-scheduled-tasks-dialog"/);
  assert.match(terminalHtml, /id="scheduled-tasks-dialog-close"/);
  assert.match(
    terminalHtml,
    /id="unified-task-dialog-scope"[\s\S]*?data-unified-task-view="current"[\s\S]*?当前目录[\s\S]*?data-unified-task-view="all"[\s\S]*?全部/,
  );
  assert.match(terminalHtml, /定时任务统一管理器/);
  assert.match(terminalHtml, /app-unified-tasks\.js\?v=/);
});

test("compact task window reuses the settings task form and list", () => {
  assert.match(
    indexHtml,
    /get\("compact"\) === "tasks"[\s\S]*?classList\.add\("scheduled-tasks-window"\)/,
  );
  assert.match(indexHtml, /id="unified-task-form-card"/);
  assert.match(indexHtml, /id="unified-task-list"/);
  assert.match(
    indexHtml,
    /id="scheduled-tasks-window-close"[\s\S]*?aria-label="关闭定时任务窗口"/,
  );
  assert.match(indexHtml, /app-unified-tasks\.js/);
  assert.match(
    settingsCss,
    /body\.scheduled-tasks-window \.browser-topbar,[\s\S]*?\.settings-actions,[\s\S]*?#settings-status,[\s\S]*?\.tab-view:not\(#settings-view\)[\s\S]*?#settings-view > \.settings-subpanel:not\(#settings-panel-auto-continue-tasks\)[\s\S]*?display: none !important/,
  );
  assert.match(
    settingsCss,
    /body\.scheduled-tasks-window #settings-view,[\s\S]*?#settings-panel-auto-continue-tasks[\s\S]*?display: grid !important/,
  );
  assert.match(
    navigationTabsJs,
    /function workspacePathQuery\(path\)[\s\S]*?scheduled-tasks-window[\s\S]*?params\.set\("compact", "tasks"\)/,
  );
  assert.match(
    unifiedTasksJs,
    /function closeScheduledTasksWindow\(\)[\s\S]*?WebClxAndroid[\s\S]*?window\.history\.length > 1[\s\S]*?window\.history\.back\(\)[\s\S]*?window\.location\.replace\(fallbackUrl\)[\s\S]*?window\.close\(\)[\s\S]*?window\.closed[\s\S]*?window\.location\.replace\(fallbackUrl\)[\s\S]*?scheduled-tasks-window-close[\s\S]*?addEventListener\("click"[\s\S]*?closeScheduledTasksWindow\(\)/,
  );
  assert.match(unifiedTasksJs, /data-unified-task-view[\s\S]*?setUnifiedTaskView/);
  assert.match(unifiedTasksJs, /task\.sessionId === unifiedTaskState\.currentSessionId/);
  assert.match(
    settingsCss,
    /\.scheduled-tasks-window-close \{[\s\S]*?display: none[\s\S]*?body\.scheduled-tasks-window \.scheduled-tasks-window-close \{[\s\S]*?display: inline-grid/,
  );
  assert.match(
    settingsCss,
    /body\.scheduled-tasks-window \.unified-task-table th,[\s\S]*?white-space: nowrap;[\s\S]*?text-overflow: ellipsis/,
  );
  assert.match(
    settingsCss,
    /body\.scheduled-tasks-window \.unified-task-table \{[\s\S]*?min-width: 760px;[\s\S]*?body\.scheduled-tasks-window \.unified-task-table th:nth-child\(7\) \{ width: 170px; \}/,
  );
  assert.match(
    settingsCss,
    /body\.scheduled-tasks-window #settings-panel-auto-continue-tasks\s*\n\s*> \.settings-card:has\(#unified-task-list\),[\s\S]*?body\.scheduled-tasks-window #settings-panel-auto-continue-tasks\s*\n\s*> \.settings-card:has\(#auto-continue-history-list\) \{[\s\S]*?display: none !important/,
  );
});

test("server tasks can send immediately and cancel without horizontal scrolling", () => {
  assert.match(
    unifiedTasksJs,
    /task\.source === "server" \? `[\s\S]*?<button class="button secondary unified-task-run-now"[\s\S]*?立即发送[\s\S]*?data-task-id="\$\{escapeHtml\(task\.taskId\)\}">/,
  );
  assert.match(
    unifiedTasksJs,
    /async function runUnifiedTaskNow\(taskId\) \{[\s\S]*?const task = findUnifiedTask\(taskId\);[\s\S]*?data: scheduledInputBody\(task\),[\s\S]*?\/input`[\s\S]*?if \(task\.sendEnter\) \{[\s\S]*?\/enter`[\s\S]*?cancelUnifiedTaskBySource\("server", taskId\);[\s\S]*?定时任务已立即发送并取消。/,
  );
  assert.match(
    unifiedTasksJs,
    /function scheduledInputBody\(task\) \{[\s\S]*?text\.includes\("\\r"\)[\s\S]*?\\u001b\[200~/,
  );
  assert.match(
    unifiedTasksJs,
    /async function openScheduledTasksDialog\(\) \{[\s\S]*?terminal-scheduled-tasks-dialog[\s\S]*?dialog\.showModal\(\)[\s\S]*?state\.activeSessionId[\s\S]*?taskView = "current"[\s\S]*?loadUnifiedTerminals\(\);[\s\S]*?loadUnifiedPresetTargets\(\);[\s\S]*?loadUnifiedTasks\(\);/,
  );
  assert.match(
    settingsCss,
    /\.terminal-scheduled-tasks-dialog \{[\s\S]*?width: min\(680px, calc\(100vw - 28px\)\);[\s\S]*?max-height: min\(77dvh, 700px\);/,
  );
  assert.match(
    settingsCss,
    /\.terminal-scheduled-tasks-form \.unified-task-table \{[\s\S]*?table-layout: fixed;[\s\S]*?min-width: 0;[\s\S]*?width: 100%;/,
  );
  assert.match(
    settingsCss,
    /\.terminal-scheduled-tasks-form \.unified-task-table th:nth-child\(1\),[\s\S]*?\.terminal-scheduled-tasks-form \.unified-task-table td:nth-child\(1\) \{ width: 178px; \}/,
  );
  assert.match(
    terminalHtml,
    /<table class="entry-table unified-task-table">[\s\S]*?<thead>\s*<tr>\s*<th scope="col">操作<\/th>\s*<th scope="col">类型<\/th>/,
  );
  assert.match(
    unifiedTasksJs,
    /dialogMode \? `\s*<td>\$\{actionCell\}<\/td>\s*<td title=/,
  );
  assert.match(
    unifiedTasksJs,
    /const runNowButton = event\.target\.closest\?\.\("\.unified-task-run-now"\);[\s\S]*?runUnifiedTaskNow\(runNowButton\.dataset\.taskId \|\| ""\);/,
  );
});

test("dialog backdrop clicks never dismiss dialogs", () => {
  assert.match(terminalHtml, /dialog-backdrop-guard\.js\?v=20260916c/);
  assert.match(terminalHtml, /id="terminal-scheduled-tasks-dialog"/);
  assert.doesNotMatch(
    unifiedTasksJs,
    /terminal-scheduled-tasks-dialog[\s\S]*?addEventListener\("click"[\s\S]*?event\.target === event\.currentTarget/,
    "backdrop clicks and gesture-synthesized clicks must not close the task dialog",
  );
});

test("task content and result column is rendered last", () => {
  assert.match(
    indexHtml,
    /<th>目标终端<\/th>[\s\S]*?<th>状态<\/th>\s*<th>操作<\/th>\s*<th>内容 \/ 结果<\/th>/,
  );
  assert.match(
    unifiedTasksJs,
    /<td class="meta-text">\$\{escapeHtml\(status\)\}<\/td>\s*<td>\$\{actionCell\}<\/td>\s*<td class="task-preview-cell[^>]*">/,
  );
  assert.match(
    unifiedTasksJs,
    /function truncateUnifiedTaskTarget\(value, maxChars = 20\)[\s\S]*?chars\.slice\(0, Math\.max\(0, maxChars - 1\)\)[\s\S]*?<td title="\$\{escapeHtml\(targetText\)\}">\$\{escapeHtml\(targetDisplay\)\}<\/td>/,
  );
  assert.match(
    unifiedTasksJs,
    /terminalName: webclxName \|\| \(sessionId \? `未找到 \(\$\{sessionId\}\)` : "未找到"\)/,
  );
  assert.match(
    unifiedTasksJs,
    /function deduplicateUnifiedTasks\(tasks\)[\s\S]*?`\$\{task\.source\}:\$\{task\.taskId\}`[\s\S]*?uniqueTasks\.has\(identity\)/,
  );
});

test("remaining time uses a numeric hours-minutes-seconds format", () => {
  assert.match(
    unifiedTasksJs,
    /function formatUnifiedTaskRemaining\(dueAtMs\)[\s\S]*?String\(minutes\)\.padStart\(2, "0"\)[\s\S]*?String\(seconds\)\.padStart\(2, "0"\)/,
  );
  assert.match(
    unifiedTasksJs,
    /return `\$\{hours\}:\$\{String\(minutes\)\.padStart\(2, "0"\)\}:\$\{String\(seconds\)\.padStart\(2, "0"\)\}`;/,
  );
});
