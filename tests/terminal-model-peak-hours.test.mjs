import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalJs = readFileSync(
  new URL("../static/terminal-session-activity.js", import.meta.url),
  "utf8",
);
const renderJs = readFileSync(
  new URL("../static/terminal-session-render.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const managerRs = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");

assert.match(
  terminalJs,
  /const TERMINAL_MODEL_PEAK_WINDOWS = Object\.freeze\(\{[\s\S]*glm: "14:00-18:00"[\s\S]*deepseek_weekday: \["09:00-12:00", "14:00-18:00"\][\s\S]*\}\)/,
  "model peak windows should default GLM to 14-18 and DeepSeek to weekday 9-12 and 14-18",
);

assert.match(
  terminalJs,
  /function terminalSessionInModelPeakWindow\(session[\s\S]*terminalModelPeakFamily\(session\)[\s\S]*return false/,
  "terminal peak detection should use the backend model family and never mark unknown models",
);

assert.match(
  terminalJs,
  /function terminalModelPeakTransitionKey\(sessions\)[\s\S]*terminalModelPeakFamily\(session\)/,
  "peak-entry toast state should be keyed by terminal and current model family",
);

assert.match(
  renderJs,
  /peak: sharedTerminalSessionInModelPeakWindow\(session\)/,
  "terminal dropdown entries should carry backend-detected peak metadata",
);

assert.match(
  renderJs,
  /option\.classList\.toggle\("session-option-peak", Boolean\(entry\.peak\)\)/,
  "terminal dropdown options should turn red when the terminal model is in its peak window",
);

assert.match(
  renderJs,
  /syncTerminalModelPeakToasts\(sessions\)/,
  "terminal session rendering should evaluate model-peak toast transitions",
);

assert.match(
  terminalHtml,
  /terminal-session-activity\.js\?v=20260904a/,
  "terminal peak helper should get a fresh cache-busting version",
);
assert.match(
  terminalHtml,
  /terminal-session-render\.js\?v=20260913a/,
  "terminal peak renderer should get a fresh cache-busting version",
);

assert.match(
  appHtml,
  /terminal-session-activity\.js\?v=20260904a/,
  "home peak helper should get a fresh cache-busting version",
);
assert.match(
  appHtml,
  /terminal-session-render\.js\?v=20260903b/,
  "home peak renderer should get a fresh cache-busting version",
);

assert.match(
  managerRs,
  /const MODEL_PEAK_GLM_WINDOW: &str = "14:00-18:00"/,
  "backend GLM peak window should be fixed to 14:00-18:00",
);
assert.match(
  managerRs,
  /const MODEL_PEAK_DEEPSEEK_WEEKDAY_WINDOWS: &\[&str\] = &\["09:00-12:00", "14:00-18:00"\]/,
  "backend DeepSeek peak windows should be fixed to weekday Beijing peak hours",
);
assert.match(
  managerRs,
  /if weekday > 5 \{[\s\S]*return false/,
  "DeepSeek weekend should always be off-peak",
);
