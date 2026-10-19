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

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const workerScript = join(
  repoDir,
  "docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh",
);

class ChildProcessGuard {
  constructor() {
    this.children = new Set();
  }

  spawn(command, args) {
    const child = spawn(command, args);
    this.children.add(child);
    child.once("exit", () => this.children.delete(child));
    return child;
  }

  [Symbol.dispose]() {
    for (const child of this.children) child.kill("SIGKILL");
  }
}

function writeRequest(queueDir, request) {
  writeFileSync(
    join(queueDir, "requests", `${request.request_id}.json`),
    JSON.stringify(request),
  );
}

function waitForExit(child) {
  return new Promise((resolveExit) => {
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("exit", (code) => resolveExit({ code, stderr }));
  });
}

async function withTimeout(promise, message, timeoutMs = 20_000) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(message)), timeoutMs);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

async function waitFor(predicate, message, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolveWait) => setTimeout(resolveWait, 50));
  }
  assert.fail(message);
}

function requestRunDir(queueDir, requestId) {
  const runsDir = join(queueDir, "runs");
  if (!existsSync(runsDir)) return null;
  for (const name of readdirSync(runsDir)) {
    const runDir = join(runsDir, name);
    if (existsSync(join(runDir, `${requestId}.json`))) return runDir;
  }
  return null;
}

using childProcesses = new ChildProcessGuard();
const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-running-isolation-"));
const queueDir = join(fixtureRoot, "queue");
const projectDir = join(fixtureRoot, "project");
const firstStarted = join(fixtureRoot, "first-started");
const executions = join(fixtureRoot, "executions");
const sourceVersion = join(projectDir, "source-version.txt");
const terminalMessages = [];

mkdirSync(join(queueDir, "requests"), { recursive: true });
mkdirSync(projectDir, { recursive: true });

const callbackServer = createServer(async (request, response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  if (request.url === "/api/terminal/sessions/message") {
    terminalMessages.push(JSON.parse(body));
  }
  response.writeHead(200, { "content-type": "application/json" });
  response.end('{"ok":true,"submitted":true}');
});
await new Promise((resolveListen) => callbackServer.listen(0, "127.0.0.1", resolveListen));
callbackServer.unref();
const address = callbackServer.address();
assert.ok(address && typeof address === "object");

const command = [
  "bash",
  "-lc",
  `cat ${JSON.stringify(sourceVersion)} >> ${JSON.stringify(executions)}; touch ${JSON.stringify(firstStarted)}; sleep 2`,
];
const requestBase = {
  request_kind: "compile",
  project: "running-isolation-fixture",
  project_dir: projectDir,
  project_path: "running-isolation-fixture",
  source_terminal_id: "s-test",
  debounce_secs: 0,
  command,
};
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
  "15",
  "--max-concurrency",
  "2",
];

try {
  writeFileSync(sourceVersion, "version-1\n");
  writeRequest(queueDir, { ...requestBase, request_id: "101-running", requested_at: 101 });
  const workers = [waitForExit(childProcesses.spawn("bash", workerArgs))];
  await waitFor(() => existsSync(firstStarted), "the first compile command did not start");

  writeFileSync(sourceVersion, "version-2\n");
  writeRequest(queueDir, { ...requestBase, request_id: "102-later", requested_at: 102 });
  workers.push(waitForExit(childProcesses.spawn("bash", workerArgs)));

  const results = await withTimeout(
    Promise.all(workers),
    "compile worker isolation test exceeded its process timeout",
  );
  for (const result of results) assert.equal(result.code, 0, result.stderr);

  assert.equal(
    readFileSync(executions, "utf8"),
    "version-1\nversion-2\n",
    "a request submitted after compilation starts must execute in a later run",
  );
  const runningRunDir = requestRunDir(queueDir, "101-running");
  const laterRunDir = requestRunDir(queueDir, "102-later");
  assert.ok(runningRunDir && laterRunDir);
  assert.notEqual(runningRunDir, laterRunDir, "running and later requests must not be merged");
  assert.equal(terminalMessages.length, 2);
  for (const message of terminalMessages) {
    assert.doesNotMatch(
      message.data,
      /本轮合并/,
      "a one-request run must not report a merged request count",
    );
  }
} finally {
  callbackServer.close();
  callbackServer.closeAllConnections();
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile worker running request isolation test passed");
