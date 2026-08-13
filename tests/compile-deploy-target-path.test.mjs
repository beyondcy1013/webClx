import assert from "node:assert/strict";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const wrapperUrl = new URL(
  "../.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-and-deploy.sh",
  import.meta.url,
);
const wrapper = readFileSync(wrapperUrl, "utf8");

assert.match(
  wrapper,
  /cargo metadata --format-version 1 --no-deps/,
  "two-stage deploy must resolve Cargo's project-owned target directory",
);
assert.match(
  wrapper,
  /\.target_directory/,
  "two-stage deploy must read target_directory from Cargo metadata",
);
assert.doesNotMatch(
  wrapper,
  new RegExp("isolated_binary|\\.webclx-compile-queue/work/cargo-target"),
  "two-stage deploy must not reconstruct the compile worker's retired private target path",
);

const fixtureRoot = mkdtempSync(join(tmpdir(), "compile-deploy-target-"));
const projectDir = join(fixtureRoot, "project");
const targetDir = join(fixtureRoot, "shared-target");
const mockBin = join(fixtureRoot, "bin");
mkdirSync(join(projectDir, "src"), { recursive: true });
mkdirSync(join(projectDir, ".cargo"), { recursive: true });
mkdirSync(mockBin);
writeFileSync(
  join(projectDir, "Cargo.toml"),
  '[package]\nname = "fixture_bin"\nversion = "0.1.0"\nedition = "2024"\n',
);
writeFileSync(join(projectDir, "src/main.rs"), "fn main() {}\n");
writeFileSync(
  join(projectDir, ".cargo/config.toml"),
  "[build]\ntarget-dir = " + JSON.stringify(targetDir) + "\n",
);
writeFileSync(join(mockBin, "curl"), '#!/bin/sh\necho \'{"sessions":[]}\'\n');
chmodSync(join(mockBin, "curl"), 0o755);

try {
  const dryRun = spawnSync(
    "bash",
    [
      wrapperUrl.pathname,
      "--project",
      "fixture",
      "--project-dir",
      projectDir,
      "--service-name",
      "fixture.service",
      "--binary-path",
      "/tmp/fixture_bin",
      "--source-terminal-name",
      "fixture-terminal",
      "--dry-run",
    ],
    {
      encoding: "utf8",
      env: { ...process.env, PATH: mockBin + ":" + process.env.PATH },
    },
  );
  assert.equal(dryRun.status, 0, dryRun.stderr);
  const deployMatch = dryRun.stdout.match(
    /=== STEP 2: DEPLOY PAYLOAD ===\n([^\n]+)\n/,
  );
  assert.ok(deployMatch, dryRun.stdout);
  const deployPayload = JSON.parse(deployMatch[1]);
  assert.ok(deployPayload.script.includes(targetDir), deployPayload.script);
  const syntax = spawnSync("bash", ["-n"], {
    encoding: "utf8",
    input: deployPayload.script,
  });
  assert.equal(syntax.status, 0, syntax.stderr);
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}

console.log("compile/deploy Cargo target path contract tests passed");
