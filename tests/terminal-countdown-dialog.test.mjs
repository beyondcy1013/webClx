import assert from "node:assert/strict";
import fs from "node:fs";

const terminalHtml = fs.readFileSync(
  new URL("../static/terminal.html", import.meta.url),
  "utf8",
);
const stylesTerminalCss = fs.readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);
const countdownJs = fs.readFileSync(
  new URL("../static/terminal-countdown-dialog.js", import.meta.url),
  "utf8",
);

// 1. 验证 HTML 结构
assert.match(
  terminalHtml,
  /id="terminal-countdown-dock"/,
  "terminal.html must contain #terminal-countdown-dock",
);
assert.match(
  terminalHtml,
  /id="terminal-countdown-toggle"/,
  "terminal.html must contain #terminal-countdown-toggle button",
);
assert.match(
  terminalHtml,
  /id="terminal-countdown-card"/,
  "terminal.html must contain #terminal-countdown-card",
);
assert.match(
  terminalHtml,
  /data-delay-seconds="20"[\s\S]*?延迟 20秒/,
  "terminal.html must contain 20 seconds delay button",
);
assert.match(
  terminalHtml,
  /data-delay-seconds="60"[\s\S]*?延迟 1分钟/,
  "terminal.html must contain 1 minute delay button",
);
assert.match(
  terminalHtml,
  /data-delay-seconds="120"[\s\S]*?延迟 2分钟/,
  "terminal.html must contain 2 minutes delay button",
);
assert.match(
  terminalHtml,
  /id="terminal-countdown-flush-btn"/,
  "terminal.html must contain flush button",
);
assert.match(
  terminalHtml,
  /id="terminal-countdown-cancel-btn"/,
  "terminal.html must contain cancel button",
);
assert.match(
  terminalHtml,
  /src="\/assets\/terminal-countdown-dialog\.js/,
  "terminal.html must load terminal-countdown-dialog.js",
);

// 2. 验证 CSS 定位在窗体右边中间（top: 50%; transform: translateY(-50%)）
assert.match(
  stylesTerminalCss,
  /\.terminal-countdown-dock\s*\{[\s\S]*?position:\s*fixed;[\s\S]*?top:\s*50%;[\s\S]*?transform:\s*translateY\(-50%\);/,
  "terminal-countdown-dock must be centered vertically on the right edge",
);

// 3. 验证 formatCountdownTime 函数逻辑
const runner = new Function(
  `${countdownJs}\nreturn globalThis.WebClxTerminalCountdownDialog;`,
);
const dialogHelper = runner();
assert.ok(dialogHelper, "WebClxTerminalCountdownDialog must be exported");
assert.equal(dialogHelper.formatCountdownTime(0), "00:00");
assert.equal(dialogHelper.formatCountdownTime(20), "00:20");
assert.equal(dialogHelper.formatCountdownTime(65), "01:05");
assert.equal(dialogHelper.formatCountdownTime(3665), "01:01:05");

// 4. 验证统一任务列表与定时芯片对 agent_message / queue 的支持
const unifiedJs = fs.readFileSync(
  new URL("../static/app-unified-tasks.js", import.meta.url),
  "utf8",
);
const pasteJs = fs.readFileSync(
  new URL("../static/terminal-paste.js", import.meta.url),
  "utf8",
);

assert.match(
  unifiedJs,
  /agent_message:\s*"排队消息"/,
  "app-unified-tasks.js must support agent_message label",
);
assert.match(
  unifiedJs,
  /state\?\.activeSessionId\s*&&\s*task\?\.sessionId === state\.activeSessionId/,
  "app-unified-tasks.js must match current active terminal regardless of workdir mismatch",
);

assert.match(
  pasteJs,
  /定时 \$\{counts\.current\}\/\$\{counts\.total\} \(\$\{earliestSec\}s\)/,
  "terminal-paste.js must show countdown in chip when within 300s",
);

// 5. 验证排队倒计时与定时任务取消必须要求确认（防误触）
assert.match(
  countdownJs,
  /window\.confirm\(confirmMessage\)/,
  "terminal-countdown-dialog.js must require window.confirm before cancelling countdown task",
);
assert.match(
  unifiedJs,
  /window\.confirm\(confirmMessage\)/,
  "app-unified-tasks.js must require window.confirm before cancelling unified task",
);

const autoContinueJs = fs.readFileSync(
  new URL("../static/app-auto-continue-tasks.js", import.meta.url),
  "utf8",
);
assert.match(
  autoContinueJs,
  /window\.confirm\("确定要取消该服务端排队\/定时输入任务吗？/,
  "app-auto-continue-tasks.js must require window.confirm before cancelling scheduled task",
);
assert.match(
  pasteJs,
  /window\.confirm\(`确定要取消全部 \$\{taskIds\.length\} 个排队\/定时输入任务吗？/,
  "terminal-paste.js must require window.confirm before clearing all scheduled tasks",
);

console.log("terminal-countdown-dialog tests passed!");
