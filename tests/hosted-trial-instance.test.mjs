import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const script = new URL("../scripts/hosted-trial-instance.sh", import.meta.url).pathname;

function run(args, env = {}) {
  return spawnSync("bash", [script, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });
}

test("trial instance planner requires an explicit customer id", () => {
  const result = run([]);
  assert.equal(result.status, 2);
  assert.match(result.stderr, /--customer-id/);
});

test("trial instance planner rejects unsafe customer identifiers", () => {
  const result = run(["--customer-id", "../admin"]);
  assert.equal(result.status, 2);
  assert.match(result.stderr, /customer id/i);
});

test("trial instance planner is dry-run by default and emits no secrets", () => {
  const result = run(["--customer-id", "demo-01", "--port", "12101"]);
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.equal(plan.mode, "dry-run");
  assert.equal(plan.customer_id, "demo-01");
  assert.equal(plan.hostname, "trial-demo-01.fpsq.xyz");
  assert.equal(plan.loopback_port, 12101);
  assert.equal(plan.service_name, "webclx-trial-demo-01.service");
  assert.equal(plan.os_user, "webclx_demo_01");
  assert.equal(plan.trial_days, 7);
  assert.equal(plan.export_days, 7);
  assert.equal(plan.app_dir, "/srv/webclx-trials/demo-01/app");
  assert.equal(plan.workspace_root, "/srv/webclx-trials/demo-01/workspace");
  assert.equal(plan.backup_target, "/srv/webclx-trial-backups/demo-01");
  assert.doesNotMatch(result.stdout, /password|token|secret/i);
});

test("trial instance planner does not require jq", () => {
  const result = run(["--customer-id", "demo-01"], { PATH: "/usr/bin:/bin" });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).customer_id, "demo-01");
});

test("trial instance apply fails closed without readiness evidence", () => {
  const result = run([
    "--customer-id", "demo-01",
    "--port", "12101",
    "--apply",
    "--confirm", "demo-01",
  ]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /DNS|TLS|binary|static|readiness/i);
});

test("trial instance planner renders isolated service and proxy templates", () => {
  const renderDir = mkdtempSync(join(tmpdir(), "webclx-trial-render-"));
  try {
    const result = run([
      "--customer-id", "demo-01",
      "--port", "12101",
      "--tls-cert", "/etc/letsencrypt/live/trial-demo-01/fullchain.pem",
      "--tls-key", "/etc/letsencrypt/live/trial-demo-01/privkey.pem",
      "--render-dir", renderDir,
    ]);
    assert.equal(result.status, 0, result.stderr);
    const service = readFileSync(join(renderDir, "webclx-trial-demo-01.service"), "utf8");
    const nginx = readFileSync(join(renderDir, "nginx-demo-01.conf"), "utf8");
    const firewall = readFileSync(join(renderDir, "firewall-demo-01.sh"), "utf8");
    assert.match(service, /User=webclx_demo_01/);
    assert.match(service, /MemoryMax=1G/);
    assert.match(service, /NoNewPrivileges=true/);
    assert.match(nginx, /proxy_pass http:\/\/127\.0\.0\.1:12101/);
    assert.match(nginx, /proxy_set_header Upgrade/);
    assert.match(firewall, /--dport 12101 ! -s 127\.0\.0\.1 -j REJECT/);
  } finally {
    rmSync(renderDir, { recursive: true, force: true });
  }
});
