import assert from "node:assert/strict";
import { createServer } from "node:http";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { workerTestEnvironment } from "./compile-worker-test-env.mjs";

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const workerScript = join(
  repoDir,
  "docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh",
);

const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-precompile-supersede-"));
const queueDir = join(fixtureRoot, "queue");
const projectDir = join(fixtureRoot, "project");
mkdirSync(join(queueDir, "requests"), { recursive: true });
mkdirSync(projectDir);

// Real wall-clock epochs: the worker compares request times with run
// finished-at mtimes when deciding whether a real build was already ahead.
const baseEpoch = Math.floor(Date.now() / 1000) - 3600;

function requestBody({ id, purpose, requestedAt, command }) {
  return JSON.stringify({
    request_id: id,
    request_kind: "compile",
    project: "warmup-fixture",
    project_dir: projectDir,
    project_path: "warmup-fixture",
    source_terminal_id: "fixture-terminal",
    source_terminal_name: "fixture-terminal",
    purpose,
    requested_at: baseEpoch + requestedAt,
    debounce_secs: 0,
    command,
  });
}

function writeRequest(payload) {
  writeFileSync(
    join(queueDir, "requests", `${JSON.parse(payload).request_id}.json`),
    payload,
  );
}

function writeSupersedeMarker(requestId, requestedAt, purpose = "compile") {
  mkdirSync(join(queueDir, "supersede"), { recursive: true });
  const digest = createHash("sha256").update(projectDir).digest("hex");
  writeFileSync(
    join(queueDir, "supersede", `${digest}.json`),
    JSON.stringify({
      request_id: requestId,
      requested_at: baseEpoch + requestedAt,
      purpose,
      project_dir: projectDir,
    }),
  );
}

function waitFor(predicate, { timeoutMs = 20000, intervalMs = 100 } = {}) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolveWait, rejectWait) => {
    const tick = () => {
      if (predicate()) return resolveWait();
      if (Date.now() > deadline) return rejectWait(new Error("timed out waiting for condition"));
      setTimeout(tick, intervalMs);
    };
    tick();
  });
}

function waitForExit(child) {
  return new Promise((resolveExit) => {
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("exit", (code) => resolveExit({ code, stderr }));
  });
}

function runDirs() {
  const runsRoot = join(queueDir, "runs");
  if (!existsSync(runsRoot)) return [];
  return readdirSync(runsRoot).map((name) => join(runsRoot, name));
}

const callbackPayloads = [];
const callbackServer = createServer(async (request, response) => {
  let rawBody = "";
  for await (const chunk of request) rawBody += chunk;
  callbackPayloads.push({ path: request.url, body: rawBody ? JSON.parse(rawBody) : {} });
  response.writeHead(200, { "content-type": "application/json" });
  response.end('{"ok":true,"submitted":true}');
});
await new Promise((resolveListen) => callbackServer.listen(0, "127.0.0.1", resolveListen));
const address = callbackServer.address();
assert.ok(address && typeof address === "object");

const workerArgs = [
  workerScript,
  "--queue-dir",
  queueDir,
  "--base-url",
  `http://127.0.0.1:${address.port}`,
  "--repo-dir",
  repoDir,
  "--work-dir",
  join(fixtureRoot, "worker-cache"),
  "--command-timeout",
  "60",
];

try {
  // A long warm-up build that has not touched its artifact yet.
  writeRequest(requestBody({
    id: "001-warmup",
    purpose: "precompile",
    requestedAt: 100,
    command: ["bash", "-lc", "sleep 6; touch prewarmed.bin"],
  }));

  const worker = spawn("bash", workerArgs, { env: workerTestEnvironment() });
  const exit = waitForExit(worker);

  await waitFor(() => runDirs().some((runDir) =>
    readdirSync(runDir).some((name) => name.startsWith("build-pid-")),
  ), { timeoutMs: 20000 });

  // The real compile request lands while the warm-up is still building.
  writeRequest(requestBody({
    id: "002-real",
    purpose: "compile",
    requestedAt: 200,
    command: ["bash", "-lc", "touch real.bin"],
  }));
  writeSupersedeMarker("002-real", 200);

  const result = await exit;
  assert.equal(result.code, 0, result.stderr);

  assert.equal(
    existsSync(join(projectDir, "prewarmed.bin")),
    false,
    "the warm-up build must be stopped before it writes its artifact",
  );
  assert.equal(
    readFileSync(join(projectDir, "real.bin"), "utf8"),
    "",
    "the real compile request must still run after the warm-up was cancelled",
  );

  const supersededRunDirs = runDirs().filter((runDir) =>
    readdirSync(runDir).some((name) => name.startsWith("precompile-superseded-")),
  );
  assert.equal(supersededRunDirs.length, 1, "the warm-up run must record why it stopped");
  const markerName = readdirSync(supersededRunDirs[0])
    .find((name) => name.startsWith("precompile-superseded-"));
  assert.equal(
    readFileSync(join(supersededRunDirs[0], markerName), "utf8").trim(),
    "002-real compile",
    "the cancellation record keeps the replacing request and its purpose",
  );

  const cancelledToasts = callbackPayloads.filter(({ path, body }) =>
    path === "/api/build/compile/notify"
    && typeof body.message === "string"
    && body.message.includes("预编译预热已取消"),
  );
  assert.ok(cancelledToasts.length > 0, "a cancelled warm-up must raise a toast");
  assert.match(cancelledToasts[0].body.message, /002-real/);
  assert.equal(
    callbackPayloads.some(({ path, body }) =>
      path === "/api/terminal/sessions/message"
      && typeof body.data === "string"
      && body.data.includes("预编译预热已取消")),
    false,
    "a cancelled warm-up must not inject a terminal message",
  );

  // A queued warm-up that is already superseded must never start at all.
  rmSync(join(queueDir, "runs"), { recursive: true, force: true });
  rmSync(join(projectDir, "real.bin"), { force: true });
  writeSupersedeMarker("004-real", 400);
  writeRequest(requestBody({
    id: "003-warmup",
    purpose: "precompile",
    requestedAt: 300,
    command: ["bash", "-lc", "touch queued-prewarm.bin"],
  }));
  writeRequest(requestBody({
    id: "004-real",
    purpose: "compile",
    requestedAt: 400,
    command: ["bash", "-lc", "touch queued-real.bin"],
  }));

  const second = await waitForExit(spawn("bash", workerArgs, { env: workerTestEnvironment() }));
  assert.equal(second.code, 0, second.stderr);
  assert.equal(
    existsSync(join(projectDir, "queued-prewarm.bin")),
    false,
    "a warm-up superseded while queued must not build",
  );
  assert.equal(existsSync(join(projectDir, "queued-real.bin")), true);
  const queuedCancellation = runDirs()
    .map((runDir) => readdirSync(runDir)
      .filter((name) => name.startsWith("precompile-superseded-"))
      .map((name) => readFileSync(join(runDir, name), "utf8").trim()))
    .flat();
  assert.ok(
    queuedCancellation.includes("004-real compile"),
    "a warm-up skipped while queued must still report the replacing request",
  );

  // Two warm-ups: the newer one supersedes the older one without any real build
  // involved, because a warm-up is expendable and the newest click wins.
  rmSync(join(queueDir, "runs"), { recursive: true, force: true });
  writeRequest(requestBody({
    id: "005-warmup-a",
    purpose: "precompile",
    requestedAt: 500,
    command: ["bash", "-lc", "sleep 6; touch warmup-a.bin"],
  }));

  const third = spawn("bash", workerArgs, { env: workerTestEnvironment() });
  const thirdExit = waitForExit(third);
  await waitFor(() => runDirs().some((runDir) =>
    readdirSync(runDir).some((name) => name.startsWith("build-pid-")),
  ), { timeoutMs: 20000 });

  writeRequest(requestBody({
    id: "006-warmup-b",
    purpose: "precompile",
    requestedAt: 600,
    command: ["bash", "-lc", "touch warmup-b.bin"],
  }));
  writeSupersedeMarker("006-warmup-b", 600, "precompile");

  const thirdResult = await thirdExit;
  assert.equal(thirdResult.code, 0, thirdResult.stderr);
  assert.equal(
    existsSync(join(projectDir, "warmup-a.bin")),
    false,
    "a newer warm-up must stop the older warm-up build",
  );
  assert.equal(
    existsSync(join(projectDir, "warmup-b.bin")),
    true,
    "the newer warm-up must run after the older one was stopped",
  );
  const warmupCancellation = runDirs()
    .map((runDir) => readdirSync(runDir)
      .filter((name) => name.startsWith("precompile-superseded-"))
      .map((name) => readFileSync(join(runDir, name), "utf8").trim()))
    .flat();
  assert.ok(
    warmupCancellation.includes("006-warmup-b precompile"),
    "the warm-up run must record the newer warm-up as the replacement",
  );
  const warmupToast = callbackPayloads.find(({ path, body }) =>
    path === "/api/build/compile/notify"
    && typeof body.message === "string"
    && body.message.includes("同项目出现新的预热请求 006-warmup-b"),
  );
  assert.ok(warmupToast, "the cancelled warm-up toast must name the newer warm-up");

  // A warm-up queued behind a real build of the same project warms nothing once
  // that build has run, so it is cancelled with a message instead of running.
  rmSync(join(queueDir, "runs"), { recursive: true, force: true });
  writeRequest(requestBody({
    id: "007-real-ahead",
    purpose: "compile",
    requestedAt: 700,
    command: ["bash", "-lc", "sleep 5; touch real-ahead.bin"],
  }));
  writeRequest(requestBody({
    id: "008-warmup-behind",
    purpose: "precompile",
    requestedAt: 800,
    command: ["bash", "-lc", "touch warmup-behind.bin"],
  }));

  const fourth = await waitForExit(spawn("bash", workerArgs, { env: workerTestEnvironment() }));
  assert.equal(fourth.code, 0, fourth.stderr);
  assert.equal(existsSync(join(projectDir, "real-ahead.bin")), true);
  assert.equal(
    existsSync(join(projectDir, "warmup-behind.bin")),
    false,
    "a warm-up behind an already queued real build must not build",
  );
  const behindCancellation = runDirs()
    .map((runDir) => readdirSync(runDir)
      .filter((name) => name.startsWith("precompile-superseded-"))
      .map((name) => readFileSync(join(runDir, name), "utf8").trim()))
    .flat();
  assert.ok(
    behindCancellation.includes("007-real-ahead ahead"),
    "the skipped warm-up must record the real build that was already ahead",
  );
  const behindToast = callbackPayloads.find(({ path, body }) =>
    path === "/api/build/compile/notify"
    && typeof body.message === "string"
    && body.message.includes("正式编译 007-real-ahead 已在前"),
  );
  assert.ok(behindToast, "the warm-up toast must say a real build was already ahead");
} finally {
  await new Promise((resolveClose) => callbackServer.close(resolveClose));
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile worker precompile supersession test passed");
