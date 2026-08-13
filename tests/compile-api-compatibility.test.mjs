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
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repoDir = resolve(new URL("..", import.meta.url).pathname);
const compileService = readFileSync(join(repoDir, "src/compile_service.rs"), "utf8");
const scriptsDir = join(
  repoDir,
  ".codex/skills/webclx-compile-and-deploy/scripts",
);
const compileWrapper = join(scriptsDir, "request-webclx-compile-api.sh");
const deployWrapper = join(scriptsDir, "request-webclx-deploy-api.sh");
const twoStageWrapper = join(scriptsDir, "request-webclx-compile-and-deploy.sh");

const fixtureRoot = mkdtempSync(join(tmpdir(), "webclx-compile-wrapper-"));
const mockBin = join(fixtureRoot, "bin");
const environment = { ...process.env, PATH: `${mockBin}:${process.env.PATH}` };
delete environment.WEBCLX_TERMINAL_ID;
delete environment.WEBCLX_TERMINAL_NAME;

mkdirSync(mockBin);
writeFileSync(join(fixtureRoot, "Makefile"), "all:\n\t@true\n");
writeFileSync(
  join(mockBin, "curl"),
  '#!/bin/sh\necho "curl: (7) failed to connect" >&2\nexit 7\n',
);
writeFileSync(join(mockBin, "tmux"), "#!/bin/sh\nexit 0\n");
chmodSync(join(mockBin, "curl"), 0o755);
chmodSync(join(mockBin, "tmux"), 0o755);

test.after(() => {
  rmSync(fixtureRoot, { recursive: true, force: true });
});

test("compile endpoint enqueues a compile request", () => {
  const compileHandler = compileService.slice(
    compileService.indexOf("pub async fn request_compile("),
    compileService.indexOf("pub async fn request_deploy("),
  );
  assert.match(
    compileHandler,
    /queue_build_request\(state, payload, BuildRequestKind::Compile\)/,
    "the compile endpoint must enqueue a compile request",
  );
  assert.doesNotMatch(
    compileHandler,
    /AppError::bad_request/,
    "the compile endpoint must not reject pure compile requests",
  );
  assert.match(
    compileHandler,
    /payload\.install_command\.clear\(\)/,
    "the compile endpoint must never pass an install command to the worker",
  );
});

test("pure compile wrapper uses the compile endpoint without an install command", () => {
  const wrapperSource = readFileSync(compileWrapper, "utf8");
  assert.match(wrapperSource, /\$BASE_URL\/api\/build\/compile["']/);
  assert.doesNotMatch(wrapperSource, /\$BASE_URL\/api\/build\/deploy["']/);

  const result = spawnSync(
    "bash",
    [
      compileWrapper,
      "--base-url",
      "http://127.0.0.1:1",
      "--source-terminal-name",
      "fixture-terminal",
      "--project-dir",
      fixtureRoot,
      "--print-payload",
    ],
    { encoding: "utf8", env: environment },
  );
  assert.equal(result.status, 0, result.stderr);
  const payload = JSON.parse(result.stdout);
  assert.equal(payload.install_command, undefined);
});

test("two-stage wrapper uses a pure compile request for stage one", () => {
  const wrapperSource = readFileSync(twoStageWrapper, "utf8");
  const compileStage = wrapperSource.slice(
    wrapperSource.indexOf("# ---- step 1: compile"),
    wrapperSource.indexOf("# ---- step 2: deploy"),
  );
  assert.match(compileStage, /\$BASE_URL\/api\/build\/compile["']/);
  assert.doesNotMatch(compileStage, /install_command|noop-deploy/);
});

test("pure compile wrapper submits explicitly required artifacts", () => {
  const result = spawnSync(
    "bash",
    [
      compileWrapper,
      "--source-terminal-name",
      "fixture-terminal",
      "--project-dir",
      fixtureRoot,
      "--required-artifact",
      "dist/app.exe",
      "--required-artifact",
      "/tmp/signed-app.exe",
      "--print-payload",
    ],
    { encoding: "utf8", env: environment },
  );
  assert.equal(result.status, 0, result.stderr);
  const payload = JSON.parse(result.stdout);
  assert.deepEqual(payload.required_artifacts, ["dist/app.exe", "/tmp/signed-app.exe"]);
});

test("pure compile wrapper rejects an empty required artifact", () => {
  const result = spawnSync(
    "bash",
    [
      compileWrapper,
      "--source-terminal-name",
      "fixture-terminal",
      "--project-dir",
      fixtureRoot,
      "--required-artifact",
      "",
      "--print-payload",
    ],
    { encoding: "utf8", env: environment },
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /required-artifact must not be empty/i);
});

for (const [label, wrapper, extraArgs] of [
  ["compile", compileWrapper, []],
  [
    "deploy",
    deployWrapper,
    ["--install-cmd", "bash", "--install-arg", "scripts/deploy.sh"],
  ],
  [
    "two-stage",
    twoStageWrapper,
    [
      "--service-name",
      "fixture.service",
      "--binary-path",
      "/tmp/fixture-service",
      "--deploy-script",
      "true",
    ],
  ],
]) {
  test(`${label} wrapper reports API unavailability separately`, () => {
    const result = spawnSync(
      "bash",
      [
        wrapper,
        "--base-url",
        "http://127.0.0.1:1",
        "--project-dir",
        fixtureRoot,
        ...extraArgs,
      ],
      { encoding: "utf8", env: environment },
    );
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /webClx API unavailable/i);
    assert.doesNotMatch(result.stderr, /source terminal name/i);
  });
}

for (const [label, wrapper, extraArgs] of [
  ["compile", compileWrapper, []],
  [
    "deploy",
    deployWrapper,
    ["--install-cmd", "bash", "--install-arg", "scripts/deploy.sh"],
  ],
  [
    "two-stage",
    twoStageWrapper,
    [
      "--service-name",
      "fixture.service",
      "--binary-path",
      "/tmp/fixture-service",
      "--deploy-script",
      "true",
    ],
  ],
]) {
  test(`${label} wrapper rejects a split shell -lc command`, () => {
    const result = spawnSync(
      "bash",
      [
        wrapper,
        "--source-terminal-name",
        "fixture-terminal",
        "--project-dir",
        fixtureRoot,
        "--command-json",
        '["bash","-lc","bash","scripts/build-windows.sh"]',
        "--dry-run",
        ...extraArgs,
      ],
      { encoding: "utf8", env: environment },
    );
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /shell.*-c.*single command string/i);
  });
}

test("deploy wrapper rejects a split shell -lc install command", () => {
  const result = spawnSync(
    "bash",
    [
      deployWrapper,
      "--source-terminal-name",
      "fixture-terminal",
      "--project-dir",
      fixtureRoot,
      "--install-command-json",
      '["bash","-lc","bash","scripts/deploy.sh"]',
      "--dry-run",
    ],
    { encoding: "utf8", env: environment },
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shell.*-c.*single command string/i);
});
