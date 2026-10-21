import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { workerTestEnvironment } from "./compile-worker-test-env.mjs";

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const workerScript = process.env.WEBCLX_COMPILE_WORKER_SCRIPT || join(
  repoDir,
  "docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh",
);
const queueDir = mkdtempSync(join(tmpdir(), "webclx-compile-progress-"));
mkdirSync(join(queueDir, "requests"));

writeFileSync(
  join(queueDir, "requests", "request.json"),
  JSON.stringify({
    request_id: "progress-test",
    request_kind: "compile",
    project: "fixture",
    project_dir: queueDir,
    project_path: "fixture",
    command: [
      "bash",
      "-lc",
      "printf '\\r    Building [=======>] 37/120: tokio\\r'; sleep 2",
    ],
  }),
);

const worker = spawn(
  "bash",
  [
    workerScript,
    "--queue-dir",
    queueDir,
    "--repo-dir",
    repoDir,
    "--command-timeout",
    "10",
  ],
  { stdio: ["ignore", "pipe", "pipe"], env: workerTestEnvironment() },
);

let stderr = "";
worker.stderr.setEncoding("utf8");
worker.stderr.on("data", (chunk) => {
  stderr += chunk;
});

const deadline = Date.now() + 10_000;
let liveProgress = null;
while (Date.now() < deadline && liveProgress === null) {
  const runsDir = join(queueDir, "runs");
  let runNames = [];
  try {
    runNames = readdirSync(runsDir);
  } catch {}
  for (const runName of runNames) {
    try {
      const progress = JSON.parse(readFileSync(join(runsDir, runName, "progress.json"), "utf8"));
      if (progress.packages_completed === 37) {
        liveProgress = progress;
      }
    } catch {}
  }
  if (liveProgress === null) {
    await new Promise((resolveWait) => setTimeout(resolveWait, 100));
  }
}

assert.ok(liveProgress, `worker did not publish live progress: ${stderr}`);
assert.equal(liveProgress.project, "fixture");
assert.equal(liveProgress.phase, "compile");
assert.equal(liveProgress.spec_index, 1);
assert.equal(liveProgress.spec_count, 1);
assert.equal(liveProgress.packages_completed, 37);
assert.equal(liveProgress.packages_total, 120);
assert.equal(liveProgress.current_package, "tokio");

const exitCode = await new Promise((resolveExit) => worker.on("exit", resolveExit));
assert.equal(exitCode, 0, stderr);
const [runName] = readdirSync(join(queueDir, "runs"));
const runDir = join(queueDir, "runs", runName);
assert.match(readFileSync(join(runDir, "run-finished-at"), "utf8"), /^\d{4}-\d{2}-\d{2} /);
assert.equal(readdirSync(runDir).filter((name) => name.startsWith("status-")).length, 1);
assert.equal(readdirSync(runDir).includes("progress.json"), false);

rmSync(queueDir, { recursive: true, force: true });
console.log("compile worker live progress tests passed");
