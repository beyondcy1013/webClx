import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const repoRoot = resolve(new URL("..", import.meta.url).pathname);

const service = readFileSync(
  resolve(repoRoot, "config/systemd/webclx-sccache.service.in"),
  "utf8",
);
const dropIn = readFileSync(
  resolve(repoRoot, "config/systemd/webclx.service.d/sccache.conf"),
  "utf8",
);
const installer = readFileSync(
  resolve(repoRoot, "scripts/install-webclx-sccache-service.sh"),
  "utf8",
);

assert.match(service, /^Type=oneshot$/m);
assert.match(service, /^RemainAfterExit=yes$/m);
assert.match(service, /^EnvironmentFile=-\/etc\/default\/webclx$/m);
assert.match(service, /^ExecStart=@SCCACHE_BIN@ --start-server$/m);
assert.match(service, /^ExecStop=@SCCACHE_BIN@ --stop-server$/m);

assert.match(dropIn, /^Wants=webclx-sccache\.service$/m);
assert.match(dropIn, /^After=webclx-sccache\.service$/m);

assert.match(installer, /systemctl daemon-reload/);
assert.match(installer, /systemctl enable --now webclx-sccache\.service/);
assert.match(installer, /systemctl is-active --quiet webclx-sccache\.service/);
assert.match(installer, /--dry-run/);

console.log("webClx sccache service contract tests passed");
