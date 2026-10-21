import assert from "node:assert/strict";
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
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const migrationScript = join(repoDir, "scripts/migrate-compile-api-logs.sh");
const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-log-migration-"));
const sourceRoot = join(fixtureRoot, "codes");
const queueDir = join(sourceRoot, "webClx", ".webclx-compile-queue");
const runDir = join(queueDir, "runs", "run-1");
const projectALogs = join(sourceRoot, "project-a", "docs", "logs");
const projectBLogs = join(sourceRoot, "nested", "project-b", "docs", "logs");
const buildA = join(projectALogs, "webclx-build-010101-1.log");
const reportA = join(projectALogs, "webclx-install-report-010101-1.json");
const buildB = join(projectBLogs, "webclx-build-010101-1.log");
const buildAtSourceRoot = join(sourceRoot, "docs", "logs", "webclx-build-root.log");
const unrelated = join(projectALogs, "project-owned.log");
const destinationA = join(
  queueDir,
  "legacy",
  "project-a",
  "docs",
  "logs",
  "webclx-build-010101-1.log",
);
const destinationReportA = join(
  queueDir,
  "legacy",
  "project-a",
  "docs",
  "logs",
  "webclx-install-report-010101-1.json",
);
const destinationB = join(
  queueDir,
  "legacy",
  "nested",
  "project-b",
  "docs",
  "logs",
  "webclx-build-010101-1.log",
);
const destinationAtSourceRoot = join(
  queueDir,
  "legacy",
  "docs",
  "logs",
  "webclx-build-root.log",
);

function runMigration(...args) {
  return spawnSync(
    "bash",
    [
      migrationScript,
      "--source-root",
      sourceRoot,
      "--queue-dir",
      queueDir,
      ...args,
    ],
    { encoding: "utf8" },
  );
}

try {
  mkdirSync(projectALogs, { recursive: true });
  mkdirSync(projectBLogs, { recursive: true });
  mkdirSync(dirname(buildAtSourceRoot), { recursive: true });
  mkdirSync(runDir, { recursive: true });
  mkdirSync(dirname(destinationB), { recursive: true });
  writeFileSync(buildA, "build-a\n");
  writeFileSync(reportA, '{"report":"a"}\n');
  writeFileSync(buildB, "build-b\n");
  writeFileSync(buildAtSourceRoot, "build-root\n");
  writeFileSync(destinationB, "build-b\n");
  writeFileSync(unrelated, "keep\n");
  writeFileSync(join(runDir, "log-a.path"), `${buildA}\n`);
  writeFileSync(join(runDir, "install-report-a.path"), `${reportA}\n`);
  writeFileSync(
    join(runDir, "log-missing.path"),
    `${join(sourceRoot, "missing", "docs", "logs", "webclx-build-missing.log")}\n`,
  );

  const dryRun = runMigration();
  assert.equal(dryRun.status, 0, dryRun.stderr);
  assert.match(dryRun.stdout, /mode=dry-run/);
  assert.match(dryRun.stdout, /files=4/);
  assert.equal(readFileSync(buildA, "utf8"), "build-a\n");
  assert.equal(readFileSync(join(runDir, "log-a.path"), "utf8").trim(), buildA);

  const apply = runMigration("--apply");
  assert.equal(apply.status, 0, apply.stderr);
  assert.match(apply.stdout, /mode=apply/);
  assert.match(apply.stdout, /moved=3/);
  assert.match(apply.stdout, /deduplicated=1/);
  assert.match(apply.stdout, /references_updated=2/);
  assert.equal(existsSync(buildA), false);
  assert.equal(existsSync(reportA), false);
  assert.equal(existsSync(buildB), false);
  assert.equal(existsSync(buildAtSourceRoot), false);
  assert.equal(readFileSync(destinationA, "utf8"), "build-a\n");
  assert.equal(readFileSync(destinationReportA, "utf8"), '{"report":"a"}\n');
  assert.equal(readFileSync(destinationB, "utf8"), "build-b\n");
  assert.equal(readFileSync(destinationAtSourceRoot, "utf8"), "build-root\n");
  assert.equal(readFileSync(join(runDir, "log-a.path"), "utf8").trim(), destinationA);
  assert.equal(
    readFileSync(join(runDir, "install-report-a.path"), "utf8").trim(),
    destinationReportA,
  );
  assert.equal(readFileSync(unrelated, "utf8"), "keep\n");
  assert.equal(existsSync(projectBLogs), false, "empty client log directories should be removed");

  const manifestDir = join(queueDir, "migration-manifests");
  const manifests = readdirSync(manifestDir);
  assert.equal(manifests.length, 1);
  const manifest = readFileSync(join(manifestDir, manifests[0]), "utf8");
  assert.match(manifest, /\tmoved\t/);
  assert.match(manifest, /\tdeduplicated\t/);

  const rerun = runMigration("--apply");
  assert.equal(rerun.status, 0, rerun.stderr);
  assert.match(rerun.stdout, /files=0/);

  mkdirSync(projectBLogs, { recursive: true });
  writeFileSync(buildB, "different-content\n");
  const conflict = runMigration("--apply");
  assert.notEqual(conflict.status, 0);
  assert.match(conflict.stderr, /destination conflict/i);
  assert.equal(readFileSync(buildB, "utf8"), "different-content\n");
  assert.equal(readFileSync(destinationB, "utf8"), "build-b\n");
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile API log migration tests passed");
