import assert from "node:assert/strict";
import { createServer } from "node:http";
import {
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

function writeDeployRequest({
  queueDir,
  requestId,
  requestedAt,
  projectDir,
  targetPath,
  additionalAuditPath,
  compileDelay,
  installedValue,
}) {
  writeFileSync(
    join(queueDir, "requests", `${requestId}.json`),
    JSON.stringify({
      request_id: requestId,
      request_kind: "deploy",
      project: "shared-runtime",
      project_dir: projectDir,
      project_path: projectDir.split("/").at(-1),
      source_terminal_id: "fixture-terminal",
      requested_at: requestedAt,
      debounce_secs: 0,
      command: ["bash", "-lc", `sleep ${compileDelay}; touch artifact.bin`],
      install_command: [
        "bash",
        "-lc",
        `printf '%s\\n' ${JSON.stringify(installedValue)} > ${JSON.stringify(targetPath)}`,
      ],
      audit_paths: [targetPath, ...(additionalAuditPath ? [additionalAuditPath] : [])],
      required_artifacts: ["artifact.bin"],
    }),
  );
}

async function waitForExit(child) {
  return await new Promise((resolveExit) => {
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("exit", (code) => resolveExit({ code, stderr }));
  });
}

const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-deploy-supersession-"));
const queueDir = join(fixtureRoot, "queue");
const olderProject = join(fixtureRoot, "older-worktree");
const newerProject = join(fixtureRoot, "newer-worktree");
const targetPath = join(fixtureRoot, "runtime.bin");

mkdirSync(join(queueDir, "requests"), { recursive: true });
mkdirSync(olderProject);
mkdirSync(newerProject);

writeDeployRequest({
  queueDir,
  requestId: "001-older",
  requestedAt: 100,
  projectDir: olderProject,
  targetPath,
  additionalAuditPath: join(fixtureRoot, "runtime.service"),
  compileDelay: 1,
  installedValue: "older",
});
writeDeployRequest({
  queueDir,
  requestId: "002-newer",
  requestedAt: 200,
  projectDir: newerProject,
  targetPath,
  compileDelay: 0.1,
  installedValue: "newer",
});

const callbackPayloads = [];
const callbackServer = createServer(async (request, response) => {
  let rawBody = "";
  for await (const chunk of request) {
    rawBody += chunk;
  }
  callbackPayloads.push({
    path: request.url,
    body: rawBody ? JSON.parse(rawBody) : {},
  });
  response.writeHead(200, { "content-type": "application/json" });
  response.end('{"ok":true,"submitted":true}');
});
await new Promise((resolveListen) => callbackServer.listen(0, "127.0.0.1", resolveListen));
const address = callbackServer.address();
assert.ok(address && typeof address === "object");

try {
  const args = [
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
    "10",
    "--max-concurrency",
    "2",
  ];
  const workers = [spawn("bash", args), spawn("bash", args)];
  const results = await Promise.all(workers.map(waitForExit));
  for (const result of results) {
    assert.equal(result.code, 0, result.stderr);
  }

  assert.equal(
    readFileSync(targetPath, "utf8"),
    "newer\n",
    "an older build that finishes later must not overwrite a newer successful deployment",
  );
  const runDirs = readdirSync(join(queueDir, "runs")).map((name) =>
    join(queueDir, "runs", name),
  );
  assert.equal(
    runDirs.filter((runDir) =>
      readdirSync(runDir).some((name) => name.startsWith("deploy-succeeded-")),
    ).length,
    1,
    "only the newer request should record a successful installation",
  );
  assert.equal(
    runDirs.filter((runDir) =>
      readdirSync(runDir).some((name) => name.startsWith("deploy-superseded-")),
    ).length,
    1,
    "the stale request should retain an observable superseded marker",
  );
  const supersededMessage = callbackPayloads.find(({ path, body }) =>
    path === "/api/terminal/sessions/message" &&
    typeof body.data === "string" &&
    body.data.includes("被较新成功请求取代"),
  );
  assert.ok(supersededMessage, "the stale request callback must explain why install was skipped");
  assert.match(supersededMessage.body.data, /替代请求：002-newer/);
} finally {
  await new Promise((resolveClose) => callbackServer.close(resolveClose));
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile worker deploy supersession test passed");
