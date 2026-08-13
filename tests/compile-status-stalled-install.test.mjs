import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const worker = readFileSync(
  new URL("../docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh", import.meta.url),
  "utf8",
);
const service = readFileSync(new URL("../src/compile_service.rs", import.meta.url), "utf8");
const manager = readFileSync(
  new URL("../static/app-compile-status-manager.js", import.meta.url),
  "utf8",
);
const index = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

assert.match(
  worker,
  /write_run_progress "\$project" "install"[^\n]*"\$install_command_json"/,
  "install progress must expose the install command instead of the completed compile command",
);
assert.match(
  worker,
  /monitor_install_progress[\s\S]*write_run_progress[^\n]*"install"[\s\S]*running install command/,
  "install output must refresh a dedicated progress heartbeat",
);
assert.match(
  service,
  /const COMPILE_INSTALL_STALLED_SECS:\s*u64\s*=\s*120;/,
  "an installation without progress for two minutes must be treated as stalled",
);
assert.match(
  service,
  /"timed_out"/,
  "completed commands with a timeout marker must retain an explicit timed-out status",
);
assert.match(
  service,
  /matches!\(run\.status\.as_str\(\), "running" \| "stalled"\)/,
  "live status polling must retain stalled runs",
);
assert.match(manager, /normalized === "stalled"[\s\S]*安装停滞/);
assert.match(manager, /normalized === "timed_out"[\s\S]*已超时/);
assert.match(
  manager,
  /normalized === "running" \|\| normalized === "stalled"/,
  "stalled work must remain visible in the live run table",
);
assert.match(index, /app-compile-status-manager\.js\?v=20260813b/);

console.log("compile stalled install status contracts passed");
