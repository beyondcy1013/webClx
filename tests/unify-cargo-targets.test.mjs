import assert from "node:assert/strict";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
  symlinkSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const script = join(repoDir, "scripts/unify-cargo-targets.sh");
const fixtureRoot = mkdtempSync(join(tmpdir(), "unify-cargo-targets-"));
const workspace = join(fixtureRoot, "sample-workspace");
const configuredWorkspace = join(fixtureRoot, "configured-workspace");
const cacheRoot = join(fixtureRoot, "data/cargo-target");
const configuredTarget = join(fixtureRoot, "data/explicit-target");
const queueDir = join(fixtureRoot, "queue");
const workspaceHash = createHash("sha256").update(workspace).digest("hex").slice(0, 16);
const brokenLegacyTarget = join(cacheRoot, "legacy-" + workspaceHash);
const preferredTarget = join(cacheRoot, "sample-workspace-" + workspaceHash);

mkdirSync(join(workspace, "app/src"), { recursive: true });
mkdirSync(join(workspace, "target/debug"), { recursive: true });
mkdirSync(join(configuredWorkspace, "src"), { recursive: true });
mkdirSync(join(configuredWorkspace, ".cargo"), { recursive: true });
mkdirSync(join(queueDir, "requests"), { recursive: true });
mkdirSync(cacheRoot, { recursive: true });
mkdirSync(preferredTarget, { recursive: true });
symlinkSync(join(cacheRoot, "missing-target"), brokenLegacyTarget);
writeFileSync(
  join(workspace, "Cargo.toml"),
  '[workspace]\nmembers = ["app"]\nresolver = "2"\n',
);
writeFileSync(
  join(workspace, "app/Cargo.toml"),
  '[package]\nname = "sample_app"\nversion = "0.1.0"\nedition = "2024"\n',
);
writeFileSync(join(workspace, "app/src/main.rs"), "fn main() {}\n");
writeFileSync(join(workspace, "target/debug/preexisting-artifact"), "fixture\n");
writeFileSync(
  join(configuredWorkspace, "Cargo.toml"),
  '[package]\nname = "configured_app"\nversion = "0.1.0"\nedition = "2024"\n',
);
writeFileSync(join(configuredWorkspace, "src/main.rs"), "fn main() {}\n");
writeFileSync(
  join(configuredWorkspace, ".cargo/config.toml"),
  "[build]\ntarget-dir = " + JSON.stringify(configuredTarget) + "\n",
);

function run(...extraArgs) {
  return spawnSync(
    "bash",
    [
      script,
      "--root",
      fixtureRoot,
      "--cache-root",
      cacheRoot,
      "--queue-dir",
      queueDir,
      ...extraArgs,
    ],
    { encoding: "utf8" },
  );
}

try {
  const dryRun = run();
  assert.equal(dryRun.status, 0, dryRun.stderr);
  assert.match(dryRun.stdout, /workspace_count=2/);
  assert.match(dryRun.stdout, /action=migrate/);
  assert.ok(
    dryRun.stdout.includes("destination=" + configuredTarget),
    dryRun.stdout,
  );
  assert.equal(realpathSync(join(workspace, "target")), join(workspace, "target"));

  const applied = run("--apply");
  assert.equal(applied.status, 0, applied.stderr);
  const targetRealpath = realpathSync(join(workspace, "target"));
  assert.equal(targetRealpath.startsWith(realpathSync(cacheRoot) + "/"), true);
  assert.equal(
    readFileSync(join(targetRealpath, "debug/preexisting-artifact"), "utf8"),
    "fixture\n",
  );
  assert.equal(realpathSync(join(configuredWorkspace, "target")), configuredTarget);

  const metadata = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps"],
    { cwd: workspace, encoding: "utf8" },
  );
  assert.equal(metadata.status, 0, metadata.stderr);
  assert.equal(JSON.parse(metadata.stdout).target_directory, join(workspace, "target"));
  assert.equal(realpathSync(JSON.parse(metadata.stdout).target_directory), targetRealpath);

  const rerun = run("--apply");
  assert.equal(rerun.status, 0, rerun.stderr);
  assert.match(rerun.stdout, /action=already-unified/);
  assert.equal(basename(targetRealpath).includes("sample-workspace-"), true);
  assert.equal(realpathSync(brokenLegacyTarget), targetRealpath);
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("Cargo target unification tests passed");
