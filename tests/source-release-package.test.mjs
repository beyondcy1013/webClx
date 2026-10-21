import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

const source = readFileSync(
  new URL("../scripts/package-source-release.sh", import.meta.url),
  "utf8",
);
const prepareScript = new URL(
  "../scripts/prepare-source-release-dir.sh",
  import.meta.url,
);

test("source release archive and checksum are published atomically", () => {
  assert.match(source, /ARCHIVE_PART=.*\.part/);
  assert.match(source, /CHECKSUM_PART=.*\.part/);
  assert.match(source, /mv\s+"\$ARCHIVE_PART"\s+"\$OUTPUT"/);
  assert.match(source, /mv\s+"\$CHECKSUM_PART"\s+"\$OUTPUT\.sha256"/);
  assert.match(
    source,
    /cd\s+"\$\(dirname\s+"\$OUTPUT"\)"[\s\S]*sha256sum\s+"\$\(basename\s+"\$OUTPUT"\)"/,
  );
  assert.doesNotMatch(source, /tar[^\n]+-czf\s+"\$OUTPUT"/);
});

test("source release excludes internal agent and deployment records", () => {
  for (const path of [
    ".claude",
    ".codex/plans",
    ".codex/skills/webclx-nas-deploy",
    ".codex/skills/webclx-remote-deploy",
    ".codex/skills/webclx-windows-deploy",
    ".codex/skills/webclx-workspace-icon-setting",
    ".qoder",
    ".zcode",
    "docs/cross-model-verification",
  ]) {
    assert.match(source, new RegExp(`"\\$STAGE/${path.replaceAll("/", "\\/")}"`));
  }
  assert.match(source, /"\$STAGE\/AGENTS\.MD"/);
  assert.match(source, /"\$STAGE\/scripts\/deploy-remote-servers\.sh"/);
});

test("tracked source does not embed credentialed fpsq URLs", () => {
  const result = run("git", [
    "grep",
    "-IEn",
    "https?://[^[:space:]\\\"'`/:]+:[^[:space:]\\\"'`@]+@([^[:space:]\\\"'`/]+\\.)?fpsq\\.xyz([:/[:space:]\\\"'`]|$)",
    "--",
  ]);
  assert.ok(result.status === 0 || result.status === 1, result.stderr);
  assert.equal(result.stdout, "", result.stdout);
});

function run(command, args, options = {}) {
  return spawnSync(command, args, { encoding: "utf8", ...options });
}

function runPrepare(archive, options = {}) {
  return run("bash", [prepareScript.pathname, archive], {
    env: { ...process.env, WEBCLX_RELEASE_CACHE_DIR: tmpdir() },
    ...options,
  });
}

function createReleaseFixture({
  linkedEntry = false,
  unlistedStaticEntry = false,
  sourceRelease = "version=1.8.9\ncommit=0123456789ab\ncreated_utc=2026-08-14T00:00:00Z\n",
} = {}) {
  const fixture = mkdtempSync(join(tmpdir(), "webclx-source-release-"));
  const root = join(fixture, "webClx-1.8.9");
  mkdirSync(join(root, "static"), { recursive: true });
  mkdirSync(join(root, "scripts"), { recursive: true });
  writeFileSync(join(root, "Cargo.toml"), '[package]\nversion = "1.8.9"\n');
  writeFileSync(join(root, "Cargo.lock"), "# fixture\n");
  writeFileSync(
    join(root, "SOURCE_RELEASE"),
    sourceRelease,
  );
  writeFileSync(join(root, "static", "index.html"), "fixture index\n");
  writeFileSync(join(root, "static", "i18n.js"), "fixture i18n\n");
  writeFileSync(join(root, "scripts", "rebuild-and-deploy.sh"), "#!/bin/sh\n");
  chmodSync(join(root, "scripts", "rebuild-and-deploy.sh"), 0o755);
  const manifest = run(
    "bash",
    ["-lc", "find static -type f -print0 | sort -z | xargs -0 sha256sum"],
    { cwd: root },
  );
  assert.equal(manifest.status, 0, manifest.stderr);
  writeFileSync(join(root, "STATIC_ASSETS_MANIFEST.sha256"), manifest.stdout);
  if (unlistedStaticEntry) {
    writeFileSync(join(root, "static", "unlisted.js"), "not in manifest\n");
  }
  if (linkedEntry) {
    symlinkSync("Cargo.toml", join(root, "linked-cargo"));
  }
  const archive = join(fixture, "webClx-1.8.9-source.tar.gz");
  const packed = run("tar", ["-C", fixture, "-czf", archive, "webClx-1.8.9"]);
  assert.equal(packed.status, 0, packed.stderr);
  const checksum = run("sha256sum", [archive]);
  assert.equal(checksum.status, 0, checksum.stderr);
  writeFileSync(`${archive}.sha256`, checksum.stdout);
  return { fixture, archive };
}

test("verified source release extracts into a clean ordinary directory", () => {
  const { fixture, archive } = createReleaseFixture();
  const releaseCache = mkdtempSync(join(tmpdir(), "webclx-release-cache-"));
  const result = runPrepare(archive, {
    env: { ...process.env, WEBCLX_RELEASE_CACHE_DIR: releaseCache },
  });
  try {
    assert.equal(result.status, 0, result.stderr);
    const projectDir = result.stdout.trim();
    assert.equal(projectDir.startsWith(`${releaseCache}/source-release.`), true);
    assert.equal(readFileSync(join(projectDir, "SOURCE_RELEASE"), "utf8").includes("1.8.9"), true);
    assert.notEqual(run("git", ["-C", projectDir, "rev-parse", "--is-inside-work-tree"]).status, 0);
  } finally {
    rmSync(releaseCache, { recursive: true, force: true });
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("source release preparation rejects checksum mismatches and links", () => {
  const checksumFixture = createReleaseFixture();
  writeFileSync(`${checksumFixture.archive}.sha256`, `${"0".repeat(64)}  stale.tar.gz\n`);
  const checksumResult = runPrepare(checksumFixture.archive);
  assert.notEqual(checksumResult.status, 0);
  assert.match(checksumResult.stderr, /checksum mismatch/);
  rmSync(checksumFixture.fixture, { recursive: true, force: true });

  const linkFixture = createReleaseFixture({ linkedEntry: true });
  const linkResult = runPrepare(linkFixture.archive);
  assert.notEqual(linkResult.status, 0);
  assert.match(linkResult.stderr, /links or unsupported archive entry types/);
  rmSync(linkFixture.fixture, { recursive: true, force: true });
});

test("source release preparation requires a complete static manifest", () => {
  const fixture = createReleaseFixture({ unlistedStaticEntry: true });
  const result = runPrepare(fixture.archive);
  try {
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /static asset manifest/);
  } finally {
    rmSync(fixture.fixture, { recursive: true, force: true });
  }
});

test("source release preparation rejects malformed provenance", () => {
  for (const sourceRelease of [
    "version=1.8.9\ncommit=0123456789ab\ncreated_utc=not-a-timestamp\n",
    "version=1.8.9\ncommit=0123456789ab\ncreated_utc=2026-08-14T00:00:00Z\nunexpected=accepted\n",
  ]) {
    const fixture = createReleaseFixture({ sourceRelease });
    const result = runPrepare(fixture.archive);
    try {
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /provenance/);
    } finally {
      rmSync(fixture.fixture, { recursive: true, force: true });
    }
  }
});
