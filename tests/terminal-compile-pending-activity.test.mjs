import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const terminalSessionActivity = require("../static/terminal-session-activity.js");
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalManagerRs = readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8",
);
const compileServiceRs = readFileSync(
  new URL("../src/compile_service.rs", import.meta.url),
  "utf8",
);
const compileWorker = readFileSync(
  new URL(
    "../docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh",
    import.meta.url,
  ),
  "utf8",
);

assert.equal(
  terminalSessionActivity.sessionActivityLabel({ activity_state: "building" }),
  "编译中",
  "a stopped agent with an outstanding compile request must not be shown as pending review",
);

assert.match(
  compileServiceRs,
  /register_pending_build_request\([\s\S]*request_id[\s\S]*source_terminal_id/,
  "queueing a build should explicitly bind the request lifecycle to its source terminal",
);

assert.match(
  terminalManagerRs,
  /pending_build_requests[\s\S]*TerminalActivitySnapshot::building/,
  "terminal activity should prioritize an outstanding build over completed output",
);

assert.match(
  compileWorker,
  /completed_build_request_id:\$delivery_id/,
  "the compile callback should identify which outstanding request finished",
);

assert.match(
  terminalRs,
  /if submitted[\s\S]*complete_pending_build_request\([\s\S]*completed_build_request_id/,
  "the pending build must clear only after its terminal callback is confirmed submitted",
);

assert.match(
  terminalRs,
  /!completed_build_request_id\.is_empty\(\)[\s\S]*!payload\.verify_submission[\s\S]*return Err/,
  "build completion callbacks must not bypass terminal submission verification",
);

console.log("terminal pending compile activity tests passed");
