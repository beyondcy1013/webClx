import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
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
import test from "node:test";

const root = new URL("..", import.meta.url).pathname;
const installScript = join(root, "install.sh");

test("Linux installer follows a verified versioned package contract", () => {
  const shell = readFileSync(installScript, "utf8");
  assert.match(shell, /curl -fsSL "\$REPO_API\/latest"/);
  assert.match(shell, /curl -fsSL --retry 3 -o "\$ARCHIVE" "\$ASSET_URL"/);
  assert.match(shell, /curl -fsSL --retry 3 -o "\$CHECKSUM_FILE" "\$CHECKSUM_URL"/);
  assert.match(shell, /PACKAGE="webClx-\$VERSION-\$PLATFORM\.tar\.gz"/);
  assert.match(shell, /SHA-256 mismatch/);
  assert.match(shell, /PACKAGE_DIR="\$NEXT_DIR\/webClx-\$VERSION-\$PLATFORM"/);
  assert.match(shell, /static\/index\.html/);
  assert.match(shell, /VERSION_DIR="\$INSTALL_ROOT\/webclx-\$VERSION"/);
  assert.match(shell, /--no-start/);
  assert.doesNotMatch(shell, /rm -rf/);
});

test("Windows installer follows a verified versioned package contract", () => {
  const powerShell = readFileSync(join(root, "install.ps1"), "utf8");
  assert.match(powerShell, /\$releasePath = .*\/latest/);
  assert.match(powerShell, /curl\.exe -fsSL --retry 3 -o \$archive/);
  assert.match(powerShell, /\$packageName = "webClx-\$Version-\$platform\.zip"/);
  assert.match(powerShell, /Get-FileHash \$archive -Algorithm SHA256/);
  assert.match(powerShell, /SHA-256 mismatch/);
  assert.match(powerShell, /static\\index\.html/);
  assert.match(powerShell, /\$versionDir = .*webclx-\$Version/);
  assert.match(powerShell, /NoStart/);
});

test("release package extraction preserves the expected top-level directory", () => {
  const work = mkdtempSync(join(tmpdir(), "webclx-install-package-"));
  try {
    const packageRoot = join(work, "webClx-1.10.22-linux-x86_64");
    mkdirSync(join(packageRoot, "static"), { recursive: true });
    writeFileSync(join(packageRoot, "webclx"), "fake binary\n");
    chmodSync(join(packageRoot, "webclx"), 0o755);
    writeFileSync(join(packageRoot, "static", "index.html"), "fake page\n");

    const archive = join(work, "package.tar.gz");
    const packed = spawnSync("tar", [
      "-C",
      work,
      "-czf",
      archive,
      "webClx-1.10.22-linux-x86_64",
    ]);
    assert.equal(packed.status, 0, packed.stderr);

    const extractRoot = mkdtempSync(join(tmpdir(), "webclx-install-extract-"));
    try {
      const nextDir = join(extractRoot, ".webclx-1.10.22.next.123");
      mkdirSync(nextDir);
      const extracted = spawnSync("tar", ["-xzf", archive, "-C", nextDir]);
      assert.equal(extracted.status, 0, extracted.stderr);
      const extractedPackage = join(nextDir, "webClx-1.10.22-linux-x86_64");
      assert.equal(readFileSync(join(extractedPackage, "webclx"), "utf8"), "fake binary\n");
      assert.equal(
        readFileSync(join(extractedPackage, "static", "index.html"), "utf8"),
        "fake page\n",
      );
    } finally {
      rmSync(extractRoot, { recursive: true, force: true });
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
