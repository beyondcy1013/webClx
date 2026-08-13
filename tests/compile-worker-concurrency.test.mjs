import assert from "node:assert/strict";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { workerTestEnvironment } from "./compile-worker-test-env.mjs";

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
    `[package]\nname = "${projectDir.split("/").at(-1)}"\nversion = "0.1.0"\nedition = "2024"\n`,
  );
  writeFileSync(join(projectDir, "src/main.rs"), "fn main() {}\n");
  writeFileSync(
    join(projectDir, ".cargo/config.toml"),
    `[build]\ntarget-dir = ${JSON.stringify(targetDir)}\n`,
  );
}

function writeRequest(queueDir, requestId, projectDir, timelinePath) {
  writeFileSync(
    join(queueDir, "requests", `${requestId}.json`),
    JSON.stringify({
      request_id: requestId,
      request_kind: "compile",
      project: requestId,
      project_dir: projectDir,
      project_path: requestId,
      debounce_secs: 0,
      command: [
        "bash",
        "-lc",
        `printf '${requestId} start %s\\n' "$(date +%s%N)" >> ${JSON.stringify(timelinePath)}; sleep 1; printf '${requestId} end %s\\n' "$(date +%s%N)" >> ${JSON.stringify(timelinePath)}`,
      ],
    }),
  );
}

async function runScenario({ sameTarget, maxConcurrency }) {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-compile-concurrency-"));
  const queueDir = join(fixtureRoot, "queue");
  const timelinePath = join(fixtureRoot, "timeline.txt");
  const targetA = join(fixtureRoot, "target-a");
  const targetB = sameTarget ? targetA : join(fixtureRoot, "target-b");
  const projectA = join(fixtureRoot, "project_a");
  const projectB = join(fixtureRoot, "project_b");

  mkdirSync(join(queueDir, "requests"), { recursive: true });
  writeCargoProject(projectA, targetA);
  writeCargoProject(projectB, targetB);
  writeRequest(queueDir, "request-a", projectA, timelinePath);
  writeRequest(queueDir, "request-b", projectB, timelinePath);

  const args = [
    workerScript,
    "--queue-dir",
    queueDir,
    "--repo-dir",
    repoDir,
    "--work-dir",
    join(fixtureRoot, "worker-cache"),
    "--command-timeout",
    "10",
    "--max-concurrency",
    String(maxConcurrency),
  ];

  const workers = [
    spawn("bash", args, { env: workerTestEnvironment() }),
    spawn("bash", args, { env: workerTestEnvironment() }),
  ];
  const results = await Promise.all(workers.map((worker) => new Promise((resolveExit) => {
    let stderr = "";
    worker.stderr.setEncoding("utf8");
    worker.stderr.on("data", (chunk) => { stderr += chunk; });
    worker.on("exit", (code) => resolveExit({ code, stderr }));
  })));

  try {
    for (const result of results) {
      assert.equal(result.code, 0, result.stderr);
    }
    const intervals = new Map();
    for (const line of readFileSync(timelinePath, "utf8").trim().split("\n")) {
      const [id, event, rawTime] = line.split(" ");
      const interval = intervals.get(id) || {};
      interval[event] = BigInt(rawTime);
      intervals.set(id, interval);
    }
    const a = intervals.get("request-a");
    const b = intervals.get("request-b");
    return a.start < b.end && b.start < a.end;
  } finally {
    rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

assert.equal(
  await runScenario({ sameTarget: false, maxConcurrency: 2 }),
  true,
  "different Cargo target directories should compile concurrently",
);
assert.equal(
  await runScenario({ sameTarget: true, maxConcurrency: 2 }),
  false,
  "the same Cargo target directory must remain serialized",
);
assert.equal(
  await runScenario({ sameTarget: false, maxConcurrency: 1 }),
  false,
  "the global compile concurrency setting must cap otherwise independent builds",
);

console.log("compile worker concurrency tests passed");
