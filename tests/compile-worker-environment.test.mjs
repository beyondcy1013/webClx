import assert from "node:assert/strict";
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
import { spawnSync } from "node:child_process";
import { workerTestEnvironment } from "./compile-worker-test-env.mjs";

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const workerScript = join(
  repoDir,
  "docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh",
);
const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-compile-environment-"));
const projectDir = join(fixtureRoot, "project");
const queueDir = join(fixtureRoot, "queue");
const observedEnvironment = join(fixtureRoot, "observed-environment.txt");

mkdirSync(projectDir, { recursive: true });
mkdirSync(join(queueDir, "requests"), { recursive: true });

const command = [
  "bash",
  "-lc",
  `printf '%s\\n' "$TOOLCHAIN_MARKER" >> ${JSON.stringify(observedEnvironment)}`,
];
for (const [requestId, marker] of [
  ["environment-a", "toolchain-a"],
  ["environment-b", "toolchain-b"],
]) {
  writeFileSync(
    join(queueDir, `requests/${requestId}.json`),
    JSON.stringify({
      request_id: requestId,
      request_kind: "compile",
      project: "environment-fixture",
      project_dir: projectDir,
      project_path: "environment-fixture",
      command,
      compile_environment: [{ key: "TOOLCHAIN_MARKER", value: marker }],
    }),
  );
}

const result = spawnSync(
  "bash",
  [
    workerScript,
    "--queue-dir",
    queueDir,
    "--repo-dir",
    repoDir,
    "--work-dir",
    join(fixtureRoot, "worker-cache"),
    "--command-timeout",
    "10",
  ],
  { encoding: "utf8", env: workerTestEnvironment() },
);

try {
  assert.equal(result.status, 0, result.stderr);
  const markers = readFileSync(observedEnvironment, "utf8")
    .trim()
    .split("\n")
    .sort();
  assert.deepEqual(markers, ["toolchain-a", "toolchain-b"]);

  const specs = readdirSync(join(queueDir, "runs")).flatMap((runName) =>
    readFileSync(join(queueDir, "runs", runName, "specs.jsonl"), "utf8")
      .trim()
      .split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line)),
  );
  assert.equal(specs.length, 2, "different compile environments must not dedupe");
  assert.deepEqual(
    specs.map((spec) => spec.compile_environment[0].value).sort(),
    ["toolchain-a", "toolchain-b"],
  );
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile worker environment propagation tests passed");
