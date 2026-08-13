import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const unifiedTasksJs = readFileSync(
  new URL("../static/app-unified-tasks.js", import.meta.url),
  "utf8",
);
const navigationJs = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const authRoutesRs = readFileSync(new URL("../src/routes/auth.rs", import.meta.url), "utf8");
const schedulerRs = readFileSync(
  new URL("../src/auth/preset_test_scheduler.rs", import.meta.url),
  "utf8",
);

assert.match(
  indexHtml,
  /<option value="preset-test">预设 API 测试<\/option>/,
  "preset API tests should be a unified task type",
);

assert.doesNotMatch(
  indexHtml,
  /app-preset-test-schedules\.js|id="preset-test-schedule-list"/,
  "preset API tests should not keep a separate script or task table",
);

assert.match(
  unifiedTasksJs,
  /requestJson\("\/api\/auth\/preset-test-schedules"\)/,
  "the unified task loader should include preset test schedules",
);

assert.match(
  unifiedTasksJs,
  /function normalizeUnifiedPresetTestTask\(task\)/,
  "preset test schedules should be normalized into the unified task table",
);

assert.match(
  unifiedTasksJs,
  /taskType === "preset-test"[\s\S]*?createUnifiedPresetTestTask\(createBtn\)/,
  "the unified create action should route preset tests to the preset schedule API",
);

assert.match(
  navigationJs,
  /tab === "auto-continue-tasks"[\s\S]*?loadUnifiedPresetTargets\(\)[\s\S]*?loadUnifiedTasks\(\)/,
  "opening the unified task panel should load preset targets and all task types",
);

assert.match(
  authRoutesRs,
  /"\/api\/auth\/preset-test-schedules"/,
  "the backend should expose the preset test schedule API",
);

for (const field of ["time", "weekday", "interval_minutes"]) {
  assert.match(
    schedulerRs,
    new RegExp(`pub ${field}:`),
    `schedule DTO should expose ${field} for lossless unified editing`,
  );
}
