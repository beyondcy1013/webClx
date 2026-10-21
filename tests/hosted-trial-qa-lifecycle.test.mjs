import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const script = new URL("../scripts/hosted-trial-qa-lifecycle.sh", import.meta.url).pathname;

test("QA lifecycle rejects non-QA identifiers", () => {
  const result = spawnSync("bash", [script, "provision", "--customer-id", "customer-01"], {
    encoding: "utf8",
  });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /qa-/i);
});

test("QA lifecycle rejects root path traversal", () => {
  const result = spawnSync("bash", [
    script, "freeze",
    "--customer-id", "qa-demo-01",
    "--root-dir", "/srv/webclx-trials/../escape",
  ], { encoding: "utf8" });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /path|root|directory/i);
});

test("QA lifecycle dry-run emits ordered, secret-free operations", () => {
  const root = mkdtempSync(join(tmpdir(), "webclx-qa-root-"));
  const binary = join(root, "webclx");
  const staticDir = join(root, "static");
  writeFileSync(binary, "fixture");
  spawnSync("chmod", ["+x", binary]);
  spawnSync("mkdir", ["-p", staticDir]);
  writeFileSync(join(staticDir, "index.html"), "fixture");
  try {
    const result = spawnSync("bash", [
      script, "provision",
      "--customer-id", "qa-demo-01",
      "--port", "12101",
      "--binary", binary,
      "--static-dir", staticDir,
      "--root-dir", root,
      "--dry-run",
    ], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /useradd/);
    assert.match(result.stdout, /iptables/);
    assert.match(result.stdout, /systemctl enable --now/);
    assert.doesNotMatch(result.stdout, /password|token|secret/i);
    assert.match(result.stdout, /install -m 0750 -o root -g webclx_qa_demo_01/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("QA lifecycle unit permits instance settings while keeping assets root-owned", () => {
  const source = readFileSync(script, "utf8");
  assert.match(source, /ReadWritePaths=\$APP_DIR \$WORKSPACE_DIR \$ARTIFACT_DIR/);
  assert.match(source, /chown -R root:"\$OS_USER" "\$APP_DIR\/static"/);
  assert.match(source, /chmod -R go-w "\$APP_DIR\/static"/);
});

test("QA lifecycle delete requires exact destructive confirmation", () => {
  const result = spawnSync("bash", [
    script, "delete",
    "--customer-id", "qa-demo-01",
    "--confirm-delete", "wrong",
  ], { encoding: "utf8" });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /confirm-delete/);
});

test("QA lifecycle orders freeze, export, and delete operations safely", () => {
  const common = ["--customer-id", "qa-demo-01", "--root-dir", "/srv/qa-test"];
  const freeze = spawnSync("bash", [script, "freeze", ...common], { encoding: "utf8" });
  assert.equal(freeze.status, 0, freeze.stderr);
  assert.ok(freeze.stdout.indexOf("systemctl stop") < freeze.stdout.indexOf("chmod"));

  const exportResult = spawnSync("bash", [script, "export", ...common], { encoding: "utf8" });
  assert.equal(exportResult.status, 0, exportResult.stderr);
  assert.ok(exportResult.stdout.indexOf("install -d") < exportResult.stdout.indexOf("tar --create"));
  assert.doesNotMatch(exportResult.stdout, /password|token|secret/i);

  const deletion = spawnSync("bash", [
    script, "delete", ...common, "--confirm-delete", "qa-demo-01",
  ], { encoding: "utf8" });
  assert.equal(deletion.status, 0, deletion.stderr);
  const stop = deletion.stdout.indexOf("systemctl disable --now");
  const firewall = deletion.stdout.indexOf("iptables -D");
  const directory = deletion.stdout.indexOf("rm -rf");
  assert.ok(stop >= 0 && stop < firewall && firewall < directory);
});

test("QA lifecycle export includes workspace files and records exported state", () => {
  const root = mkdtempSync(join(tmpdir(), "webclx-qa-export-"));
  const exportRoot = join(root, "exports");
  const instance = join(root, "qa-demo-01");
  const workspace = join(instance, "workspace");
  mkdirSync(workspace, { recursive: true });
  writeFileSync(join(workspace, "hello.txt"), "hello\n");
  writeFileSync(join(instance, "manifest.env"), [
    "customer_id=qa-demo-01",
    "os_user=webclx_qa_demo_01",
    "service_name=webclx-qa-qa-demo-01.service",
    "port=12101",
    `instance_dir=${instance}`,
    `app_dir=${join(instance, "app")}`,
    `workspace_dir=${workspace}`,
    `artifact_dir=${join(instance, "artifacts")}`,
    `export_dir=${join(exportRoot, "qa-demo-01")}`,
    "state=frozen",
  ].join("\n"));
  try {
    const result = spawnSync("bash", [
      script, "export",
      "--customer-id", "qa-demo-01",
      "--root-dir", root,
      "--export-dir", exportRoot,
      "--apply",
    ], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    const archive = join(exportRoot, "qa-demo-01", "qa-demo-01-workspace.tar.gz");
    assert.equal(existsSync(archive), true);
    const listing = spawnSync("tar", ["-tzf", archive], { encoding: "utf8" });
    assert.equal(listing.status, 0, listing.stderr);
    assert.match(listing.stdout, /^workspace\/hello\.txt$/m);
    assert.match(readFileSync(join(instance, "manifest.env"), "utf8"), /^state=exported$/m);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("QA lifecycle export rejects symbolic links", () => {
  const root = mkdtempSync(join(tmpdir(), "webclx-qa-symlink-"));
  const instance = join(root, "qa-demo-01");
  const workspace = join(instance, "workspace");
  mkdirSync(workspace, { recursive: true });
  symlinkSync("/etc/passwd", join(workspace, "escape"));
  writeFileSync(join(instance, "manifest.env"), [
    "customer_id=qa-demo-01",
    "port=12101",
    `instance_dir=${instance}`,
    `app_dir=${join(instance, "app")}`,
    `workspace_dir=${workspace}`,
    `artifact_dir=${join(instance, "artifacts")}`,
    `export_dir=${join(root, "exports", "qa-demo-01")}`,
    "state=frozen",
  ].join("\n"));
  try {
    const result = spawnSync("bash", [
      script, "export", "--customer-id", "qa-demo-01", "--root-dir", root, "--apply",
    ], { encoding: "utf8" });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /symbolic link/i);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
