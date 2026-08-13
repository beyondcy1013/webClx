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

function writeCargoProject(projectDir, targetDir) {
  mkdirSync(join(projectDir, "src"), { recursive: true });
  mkdirSync(join(projectDir, ".cargo"), { recursive: true });
  writeFileSync(
    join(projectDir, "Cargo.toml"),
    '[package]\nname = "coalescing_fixture"\nversion = "0.1.0"\nedition = "2024"\n',
  );
  writeFileSync(join(projectDir, "src/main.rs"), "fn main() {}\n");
  writeFileSync(
    join(projectDir, ".cargo/config.toml"),
    `[build]\ntarget-dir = ${JSON.stringify(targetDir)}\n`,
  );
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

const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-pending-coalescing-"));
const queueDir = join(fixtureRoot, "queue");
const projectDir = join(fixtureRoot, "project");
const targetDir = join(fixtureRoot, "shared-target");
const blockerStarted = join(fixtureRoot, "blocker-started");
const compileExecutions = join(fixtureRoot, "compile-executions");
const sourceVersion = join(projectDir, "source-version.txt");

mkdirSync(join(queueDir, "requests"), { recursive: true });
writeCargoProject(projectDir, targetDir);

const callbackServer = createServer(async (request, response) => {
  for await (const _chunk of request) {
    // Drain the request before acknowledging the callback.
  }
  response.writeHead(200, { "content-type": "application/json" });
  response.end('{"ok":true,"submitted":true}');
});
await new Promise((resolveListen) => callbackServer.listen(0, "127.0.0.1", resolveListen));
callbackServer.unref();
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
  "45",
  "--max-concurrency",
  "4",
];

try {
  writeRequest(queueDir, {
    request_id: "000-blocker",
    request_kind: "compile",
    project: "coalescing-fixture",
    project_dir: projectDir,
    project_path: "coalescing-fixture",
    requested_at: 1,
    debounce_secs: 0,
    command: [
      "bash",
      "-lc",
      `touch ${JSON.stringify(blockerStarted)}; sleep 20`,
    ],
  });
  const workers = [waitForExit(spawn("bash", workerArgs))];
  await waitFor(() => existsSync(blockerStarted), "the blocker did not acquire the Cargo target");

  const repeatedCommand = [
    "bash",
    "-lc",
    `cat ${JSON.stringify(sourceVersion)} >> ${JSON.stringify(compileExecutions)}`,
  ];
  const repeatedBase = {
    request_kind: "compile",
    project: "coalescing-fixture",
    project_dir: projectDir,
    project_path: "coalescing-fixture",
    debounce_secs: 0,
    command: repeatedCommand,
  };

  writeFileSync(sourceVersion, "version-1\n");
  writeRequest(queueDir, { ...repeatedBase, request_id: "101-repeat", requested_at: 101 });
  workers.push(waitForExit(spawn("bash", workerArgs)));
  await waitFor(
    () => requestRunDir(queueDir, "101-repeat") !== null,
    "the first repeated request was not claimed",
  );

  writeFileSync(sourceVersion, "version-2\n");
  writeRequest(queueDir, { ...repeatedBase, request_id: "102-repeat", requested_at: 102 });
  workers.push(waitForExit(spawn("bash", workerArgs)));
  await new Promise((resolveWait) => setTimeout(resolveWait, 6_000));

  writeFileSync(sourceVersion, "version-3\n");
  writeRequest(queueDir, { ...repeatedBase, request_id: "103-repeat", requested_at: 103 });
  workers.push(waitForExit(spawn("bash", workerArgs)));

  const results = await Promise.all(workers);
  for (const result of results) {
    assert.equal(result.code, 0, result.stderr);
  }

  assert.equal(
    readFileSync(compileExecutions, "utf8"),
    "version-3\n",
    "the coalesced run must compile once from the latest queued workspace state",
  );
  const mergedRunDir = requestRunDir(queueDir, "101-repeat");
  assert.ok(mergedRunDir, "the merged run should be retained");
  assert.deepEqual(
    ["101-repeat", "102-repeat", "103-repeat"].filter((requestId) =>
      existsSync(join(mergedRunDir, `${requestId}.json`)),
    ),
    ["101-repeat", "102-repeat", "103-repeat"],
    "later arrivals must be absorbed into the same waiting run",
  );
} finally {
  callbackServer.close();
  callbackServer.closeAllConnections();
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile worker pending coalescing test passed");
