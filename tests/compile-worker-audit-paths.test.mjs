import assert from "node:assert/strict";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

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

function runFunction(source, invocation, args = []) {
  return spawnSync("bash", ["-c", `${source}\n${invocation}`, "audit-test", ...args], {
    encoding: "utf8",
  });
}

const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-audit-paths-"));
const projectDir = join(fixtureRoot, "project");
const deployScript = join(projectDir, "deploy.sh");
const rootAlias = join(fixtureRoot, "root-alias");

try {
  mkdirSync(projectDir);
  symlinkSync("/", rootAlias);
  writeFileSync(
    deployScript,
    `#!/usr/bin/env bash
ROOT_DIR="${projectDir}"
STATUS_URL="http://127.0.0.1:16888/api/status"
version="$(jq -r '.dataset.version // ""' <<<"$payload")"
install -m 0755 "$BUILT" /home/bin/demo/demo
install -m 0755 "$BUILT" /home/bin/demo/future-output
test -e /../../ || true
test -e /c || true
`,
    { flag: "wx" },
  );

  const candidateFunction =
    shellFunction("is_plausible_audit_candidate", "collect_command_path_candidates") +
    shellFunction("collect_command_path_candidates", "collect_cargo_binary_candidates");
  const candidatesResult = runFunction(
    candidateFunction,
    `collect_command_path_candidates "$1" '["bash","deploy.sh"]' '["/home/bin/demo/static"]'`,
    [projectDir],
  );
  assert.equal(candidatesResult.status, 0, candidatesResult.stderr);
  assert.deepEqual(candidatesResult.stdout.trim().split("\n"), [
    "/home/bin/demo/demo",
    "/home/bin/demo/future-output",
    "/home/bin/demo/static",
    deployScript,
  ]);

  const safetyFunction = shellFunction("is_safe_snapshot_path", "timeout_label");
  for (const unsafePath of ["//", rootAlias]) {
    const result = runFunction(
      safetyFunction,
      'is_safe_snapshot_path "$1"',
      [unsafePath],
    );
    assert.notEqual(result.status, 0, `${unsafePath} must not resolve to an auditable path`);
  }

  const deployCollector = shellFunction(
    "collect_deploy_audit_candidates",
    "path_snapshot_json",
  );
  assert.doesNotMatch(
    deployCollector,
    /collect_command_path_candidates "\$project_dir" "\$command_json"/,
    "compile and verification scripts must not be parsed as deploy outputs",
  );
  assert.match(
    deployCollector,
    /if jq -e 'length > 0' <<<"\$audit_json"/,
    "explicit audit paths must disable install-script path discovery",
  );
  assert.match(
    deployCollector,
    /collect_explicit_audit_candidates "\$project_dir" "\$audit_json"/,
    "explicit audit paths must be collected through their own bounded path",
  );
  const explicitCollectorSource =
    shellFunction("is_plausible_audit_candidate", "collect_command_path_candidates") +
    shellFunction("collect_explicit_audit_candidates", "collect_deploy_audit_candidates") +
    deployCollector;
  const explicitCandidate = join(fixtureRoot, "installed", "demo");
  const explicitCandidatesResult = runFunction(
    explicitCollectorSource,
    `collect_deploy_audit_candidates demo "$1" '["bash","-lc","test -x target/release/demo"]' '["bash","deploy.sh"]' "$(jq -nc --arg path \"$2\" '[\$path]')" "$1/target"`,
    [projectDir, explicitCandidate],
  );
  assert.equal(explicitCandidatesResult.status, 0, explicitCandidatesResult.stderr);
  assert.equal(
    explicitCandidatesResult.stdout.trim(),
    explicitCandidate,
    "explicit audit paths must be the complete audit candidate set",
  );

  const snapshotFunction =
    "SNAPSHOT_PRUNE_NAMES=('.git' 'target')\nSNAPSHOT_MAX_FILES=2000\n" +
    shellFunction("path_snapshot_json", "snapshot_install_audit");
  const recursiveFinds = [...snapshotFunction.matchAll(/find "\$path"[^\n]*/g)];
  assert.ok(recursiveFinds.length >= 3, "directory snapshots should have explicit find calls");
  for (const [findCommand] of recursiveFinds) {
    assert.match(
      findCommand,
      /find "\$path" -xdev /,
      `directory audit must not cross mounted filesystems: ${findCommand}`,
    );
  }

  const mountSnapshot = runFunction(
    snapshotFunction,
    'path_snapshot_json /proc',
  );
  assert.equal(mountSnapshot.status, 0, mountSnapshot.stderr);
  assert.equal(
    JSON.parse(mountSnapshot.stdout).sha256,
    null,
    "a mount point must use metadata-only audit instead of recursively hashing its contents",
  );
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile worker audit path tests passed");
