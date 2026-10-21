import assert from "node:assert/strict";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  readdirSync,
  rmSync,
  symlinkSync,
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
const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-cargo-target-"));
const projectDir = join(fixtureRoot, "project");
const queueDir = join(fixtureRoot, "queue");
const configuredTarget = join(fixtureRoot, "shared-cargo-target");
const observedTarget = join(fixtureRoot, "observed-target");
const nestedProjectDir = join(fixtureRoot, "nested-project");
const nestedCargoDir = join(nestedProjectDir, "rust");
const nestedTarget = join(fixtureRoot, "nested-shared-cargo-target");
const observedNestedTarget = join(fixtureRoot, "observed-nested-target");
const gitMainDir = join(fixtureRoot, "git-main");
const gitWorktreeDir = join(fixtureRoot, "git-worktree");
const populatedGitWorktreeDir = join(fixtureRoot, "populated-git-worktree");
const gitSharedTarget = join(fixtureRoot, "git-shared-target");
const observedGitWorktreeTarget = join(fixtureRoot, "observed-git-worktree-target");
const observedGitWorktreeWrapper = join(fixtureRoot, "observed-git-worktree-wrapper");
const observedGitWorktreeCacheSize = join(fixtureRoot, "observed-git-worktree-cache-size");
const observedGitWorktreeCacheDir = join(fixtureRoot, "observed-git-worktree-cache-dir");
const observedGitWorktreeIncremental = join(fixtureRoot, "observed-git-worktree-incremental");
const observedGitWorktreeServerUds = join(fixtureRoot, "observed-git-worktree-server-uds");
const observedGitWorktreeIdleTimeout = join(fixtureRoot, "observed-git-worktree-idle-timeout");
const observedPopulatedGitWorktreeTarget = join(
  fixtureRoot,
  "observed-populated-git-worktree-target",
);
const observedPopulatedGitWorktreeServerUds = join(
  fixtureRoot,
  "observed-populated-git-worktree-server-uds",
);
const explicitGitWorktreeTarget = join(fixtureRoot, "explicit-git-worktree-target");
const observedExplicitGitWorktreeTarget = join(
  fixtureRoot,
  "observed-explicit-git-worktree-target",
);
const observedExplicitGitWorktreeWrapper = join(
  fixtureRoot,
  "observed-explicit-git-worktree-wrapper",
);
const observedExplicitGitWorktreeCacheSize = join(
  fixtureRoot,
  "observed-explicit-git-worktree-cache-size",
);
const observedExplicitGitWorktreeServerUds = join(
  fixtureRoot,
  "observed-explicit-git-worktree-server-uds",
);
const observedExplicitGitWorktreeIdleTimeout = join(
  fixtureRoot,
  "observed-explicit-git-worktree-idle-timeout",
);
const fixtureBin = join(fixtureRoot, "bin");
const fixtureSccache = join(fixtureBin, "sccache");
const workerEnvironment = {
  ...workerTestEnvironment(),
  PATH: fixtureBin + ":" + process.env.PATH,
};
delete workerEnvironment.SCCACHE_CACHE_SIZE;
delete workerEnvironment.SCCACHE_DIR;
delete workerEnvironment.SCCACHE_SERVER_UDS;
delete workerEnvironment.SCCACHE_IDLE_TIMEOUT;
delete workerEnvironment.RUSTC_WRAPPER;

mkdirSync(fixtureBin, { recursive: true });
writeFileSync(fixtureSccache, "#!/usr/bin/env bash\nexec \"$@\"\n");
chmodSync(fixtureSccache, 0o755);
mkdirSync(join(projectDir, "src"), { recursive: true });
mkdirSync(join(projectDir, ".cargo"), { recursive: true });
mkdirSync(join(nestedCargoDir, "src"), { recursive: true });
mkdirSync(join(nestedCargoDir, ".cargo"), { recursive: true });
mkdirSync(join(queueDir, "requests"), { recursive: true });
writeFileSync(
  join(projectDir, "Cargo.toml"),
  '[package]\nname = "target_fixture"\nversion = "0.1.0"\nedition = "2024"\n',
);
writeFileSync(join(projectDir, "src/main.rs"), "fn main() {}\n");
writeFileSync(
  join(projectDir, ".cargo/config.toml"),
  "[build]\ntarget-dir = " + JSON.stringify(configuredTarget) + "\n",
);
writeFileSync(
  join(nestedCargoDir, "Cargo.toml"),
  '[package]\nname = "nested_target_fixture"\nversion = "0.1.0"\nedition = "2024"\n',
);
writeFileSync(join(nestedCargoDir, "src/main.rs"), "fn main() {}\n");
writeFileSync(
  join(nestedCargoDir, ".cargo/config.toml"),
  "[build]\ntarget-dir = " + JSON.stringify(nestedTarget) + "\n",
);
writeFileSync(
  join(queueDir, "requests/request.json"),
  JSON.stringify({
    request_id: "cargo-target-test",
    request_kind: "compile",
    project: "target-fixture",
    project_dir: projectDir,
    project_path: "target-fixture",
    command: [
      "bash",
      "-c",
      "cargo metadata --format-version 1 --no-deps | jq -r .target_directory > " +
        JSON.stringify(observedTarget),
    ],
  }),
);
writeFileSync(
  join(queueDir, "requests/nested-request.json"),
  JSON.stringify({
    request_id: "nested-cargo-target-test",
    request_kind: "compile",
    project: "nested-target-fixture",
    project_dir: nestedProjectDir,
    project_path: "nested-target-fixture",
    command: [
      "bash",
      "-c",
      "cd rust && cargo metadata --format-version 1 --no-deps | jq -r .target_directory > " +
        JSON.stringify(observedNestedTarget),
    ],
  }),
);

mkdirSync(join(gitMainDir, "src"), { recursive: true });
mkdirSync(gitSharedTarget, { recursive: true });
writeFileSync(
  join(gitMainDir, "Cargo.toml"),
  '[package]\nname = "git_worktree_target_fixture"\nversion = "0.1.0"\nedition = "2024"\n',
);
writeFileSync(join(gitMainDir, "src/main.rs"), "fn main() {}\n");
symlinkSync(gitSharedTarget, join(gitMainDir, "target"));
for (const args of [
  ["init"],
  ["config", "user.email", "test@example.com"],
  ["config", "user.name", "Test User"],
  ["add", "Cargo.toml", "src/main.rs"],
  ["commit", "-m", "fixture"],
  ["worktree", "add", "--detach", gitWorktreeDir],
  ["worktree", "add", "--detach", populatedGitWorktreeDir],
]) {
  const git = spawnSync("git", args, { cwd: gitMainDir, encoding: "utf8" });
  assert.equal(git.status, 0, git.stderr);
}
mkdirSync(join(populatedGitWorktreeDir, "target"), { recursive: true });
writeFileSync(join(populatedGitWorktreeDir, "target", "existing-artifact"), "keep\n");
writeFileSync(
  join(queueDir, "requests/git-worktree-request.json"),
  JSON.stringify({
    request_id: "git-worktree-cargo-target-test",
    request_kind: "compile",
    project: "git-worktree-target-fixture",
    project_dir: gitWorktreeDir,
    project_path: "git-worktree-target-fixture",
    command: [
      "bash",
      "-c",
      "cargo metadata --format-version 1 --no-deps | jq -r .target_directory > " +
        JSON.stringify(observedGitWorktreeTarget) +
        "; printf '%s' \"${RUSTC_WRAPPER:-}\" > " +
        JSON.stringify(observedGitWorktreeWrapper) +
        "; printf '%s' \"${SCCACHE_CACHE_SIZE:-}\" > " +
        JSON.stringify(observedGitWorktreeCacheSize) +
        "; printf '%s' \"${SCCACHE_DIR:-}\" > " +
        JSON.stringify(observedGitWorktreeCacheDir) +
        "; printf '%s' \"${CARGO_INCREMENTAL:-}\" > " +
        JSON.stringify(observedGitWorktreeIncremental) +
        "; printf '%s' \"${SCCACHE_SERVER_UDS:-}\" > " +
        JSON.stringify(observedGitWorktreeServerUds) +
        "; printf '%s' \"${SCCACHE_IDLE_TIMEOUT:-}\" > " +
        JSON.stringify(observedGitWorktreeIdleTimeout),
    ],
  }),
);
writeFileSync(
  join(queueDir, "requests/populated-git-worktree-request.json"),
  JSON.stringify({
    request_id: "populated-git-worktree-cargo-target-test",
    request_kind: "compile",
    project: "populated-git-worktree-target-fixture",
    project_dir: populatedGitWorktreeDir,
    project_path: "populated-git-worktree-target-fixture",
    command: [
      "bash",
      "-c",
      "cargo metadata --format-version 1 --no-deps | jq -r .target_directory > " +
        JSON.stringify(observedPopulatedGitWorktreeTarget) +
        "; printf '%s' \"${SCCACHE_SERVER_UDS:-}\" > " +
        JSON.stringify(observedPopulatedGitWorktreeServerUds),
    ],
  }),
);
writeFileSync(
  join(queueDir, "requests/explicit-git-worktree-request.json"),
  JSON.stringify({
    request_id: "explicit-git-worktree-cargo-target-test",
    request_kind: "compile",
    project: "explicit-git-worktree-target-fixture",
    project_dir: gitWorktreeDir,
    project_path: "explicit-git-worktree-target-fixture",
    compile_environment: [
      { key: "CARGO_TARGET_DIR", value: explicitGitWorktreeTarget },
      { key: "RUSTC_WRAPPER", value: "/usr/bin/env" },
      { key: "SCCACHE_CACHE_SIZE", value: "40G" },
      { key: "SCCACHE_SERVER_UDS", value: "/explicit/sccache.sock" },
      { key: "SCCACHE_IDLE_TIMEOUT", value: "120" },
    ],
    command: [
      "bash",
      "-c",
      "cargo metadata --format-version 1 --no-deps | jq -r .target_directory > " +
        JSON.stringify(observedExplicitGitWorktreeTarget) +
        "; printf '%s' \"${RUSTC_WRAPPER:-}\" > " +
        JSON.stringify(observedExplicitGitWorktreeWrapper) +
        "; printf '%s' \"${SCCACHE_CACHE_SIZE:-}\" > " +
        JSON.stringify(observedExplicitGitWorktreeCacheSize) +
        "; printf '%s' \"${SCCACHE_SERVER_UDS:-}\" > " +
        JSON.stringify(observedExplicitGitWorktreeServerUds) +
        "; printf '%s' \"${SCCACHE_IDLE_TIMEOUT:-}\" > " +
        JSON.stringify(observedExplicitGitWorktreeIdleTimeout),
    ],
  }),
);

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
  {
    encoding: "utf8",
    env: workerEnvironment,
  },
);

try {
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(observedTarget, "utf8").trim(), configuredTarget);
  assert.equal(readFileSync(observedNestedTarget, "utf8").trim(), nestedTarget);
  const isolatedGitWorktreeTarget = realpathSync(
    readFileSync(observedGitWorktreeTarget, "utf8").trim(),
  );
  assert.notEqual(isolatedGitWorktreeTarget, realpathSync(gitSharedTarget));
  assert.ok(
    isolatedGitWorktreeTarget.startsWith(
      realpathSync(join(fixtureRoot, "worker-cache", "cargo-target")) + "/",
    ),
    isolatedGitWorktreeTarget,
  );
  assert.equal(realpathSync(join(gitWorktreeDir, "target")), isolatedGitWorktreeTarget);
  assert.match(
    readFileSync(observedGitWorktreeWrapper, "utf8").trim(),
    new RegExp("^" + fixtureSccache.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "$"),
  );
  assert.equal(readFileSync(observedGitWorktreeCacheSize, "utf8").trim(), "10G");
  assert.equal(
    readFileSync(observedGitWorktreeCacheDir, "utf8").trim(),
    join(fixtureRoot, "worker-cache", "sccache"),
  );
  assert.equal(readFileSync(observedGitWorktreeIncremental, "utf8").trim(), "0");
  const gitWorktreeServerUds = readFileSync(observedGitWorktreeServerUds, "utf8").trim();
  assert.ok(gitWorktreeServerUds.startsWith(join(fixtureRoot, "worker-cache", "tmp") + "/s-"));
  assert.ok(Buffer.byteLength(gitWorktreeServerUds) < 108, gitWorktreeServerUds);
  assert.equal(
    readFileSync(observedPopulatedGitWorktreeServerUds, "utf8").trim(),
    gitWorktreeServerUds,
  );
  assert.equal(readFileSync(observedGitWorktreeIdleTimeout, "utf8").trim(), "0");
  assert.equal(
    readFileSync(join(populatedGitWorktreeDir, "target", "existing-artifact"), "utf8"),
    "keep\n",
  );
  assert.equal(
    realpathSync(readFileSync(observedPopulatedGitWorktreeTarget, "utf8").trim()),
    realpathSync(join(populatedGitWorktreeDir, "target")),
  );
  assert.equal(
    realpathSync(readFileSync(observedExplicitGitWorktreeTarget, "utf8").trim()),
    realpathSync(explicitGitWorktreeTarget),
  );
  assert.equal(
    readFileSync(observedExplicitGitWorktreeWrapper, "utf8").trim(),
    "/usr/bin/env",
  );
  assert.equal(
    readFileSync(observedExplicitGitWorktreeCacheSize, "utf8").trim(),
    "40G",
  );
  assert.equal(
    readFileSync(observedExplicitGitWorktreeServerUds, "utf8").trim(),
    "/explicit/sccache.sock",
  );
  assert.equal(
    readFileSync(observedExplicitGitWorktreeIdleTimeout, "utf8").trim(),
    "120",
  );
  const [runName] = readdirSync(join(queueDir, "runs"));
  const runDirs = readdirSync(join(queueDir, "runs")).map((runName) =>
    join(queueDir, "runs", runName),
  );
  const log = runDirs
    .flatMap((runDir) =>
      readdirSync(runDir)
        .filter((name) => name.startsWith("build-"))
        .map((name) => readFileSync(join(runDir, name), "utf8")),
    )
    .join("\n");
  assert.ok(log.includes("cargo_target_dir=" + configuredTarget), log);
  assert.ok(log.includes("cargo_target_dir=" + nestedTarget), log);
  const specTmpDirs = [...log.matchAll(/^tmp_dir=(.+)$/gm)].map((match) => match[1]);
  assert.ok(specTmpDirs.length > 0, log);
  for (const specTmpDir of specTmpDirs) {
    assert.ok(
      Buffer.byteLength(specTmpDir) < 108,
      `per-request TMPDIR exceeds Unix socket path capacity: ${specTmpDir}`,
    );
  }
  assert.ok(readFileSync(join(queueDir, "runs", runName, "run-finished-at"), "utf8"));
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile worker Cargo target ownership tests passed");
