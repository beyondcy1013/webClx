import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { join, resolve } from "node:path";
import { readFileSync } from "node:fs";

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const workerPath = join(
  repoDir,
  "docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh",
);
const worker = readFileSync(workerPath, "utf8");

function shellFunction(name, nextName) {
  const start = worker.indexOf(`${name}() {`);
  const end = worker.indexOf(`${nextName}() {`, start);
  assert.ok(start >= 0 && end > start, `could not extract ${name}`);
  return worker.slice(start, end);
}

const slugFunction = shellFunction("safe_path_slug", "default_cargo_target_dir");
const slugResult = spawnSync(
  "bash",
  ["-c", `${slugFunction}\nset -e\nsafe_path_slug webClx\nsafe_path_slug '  '\n`],
  { encoding: "utf8" },
);
assert.equal(slugResult.status, 0, slugResult.stderr);
assert.deepEqual(slugResult.stdout.trim().split("\n"), ["webClx", "project"]);

const notification = worker.slice(
  worker.indexOf("notify_request_file()"),
  worker.indexOf("is_plausible_audit_candidate()"),
);
assert.ok(notification.includes("notify_request_file()"));
assert.match(notification, /webClx 集中日志：\$\{log_label\}/);
assert.match(notification, /webClx 集中审计：\$\{report_label\}/);
assert.match(notification, /原始备注：\$note/);

const deliveryFunction = shellFunction(
  "compile_completion_delivery_id",
  "notify_request_file",
);
const deliveryResult = spawnSync(
  "bash",
  ["-c", `${deliveryFunction}\ncompile_completion_delivery_id request-42\n`],
  { encoding: "utf8" },
);
assert.equal(deliveryResult.status, 0, deliveryResult.stderr);
assert.equal(deliveryResult.stdout, "[webclx-callback:complete:request-42]\n");
assert.match(notification, /final_message="\$\{message\} \$\{delivery_id\}"/);
assert.match(notification, /fallback_message="\$\{message\} \$\{fallback_delivery_id\}"/);

console.log("compile worker compact log contract tests passed");
