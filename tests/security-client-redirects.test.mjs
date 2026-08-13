import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const publisher = fileURLToPath(new URL(
  "../.codex/skills/webclx-artifact-publisher/scripts/publish-artifact.sh",
  import.meta.url,
));

async function listen(server) {
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${server.address().port}`;
}

async function close(server) {
  await new Promise((resolve) => server.close(resolve));
}

const directory = mkdtempSync(join(tmpdir(), "webclx-security-client-redirect-"));
const artifact = join(directory, "webClx-test-source.tar.gz");
const tokenPath = join(directory, "local-token");
writeFileSync(artifact, "test archive");
writeFileSync(tokenPath, `${"0123456789abcdef".repeat(4)}\n`, { mode: 0o600 });

let destinationRequests = 0;
const destination = createServer((_request, response) => {
  destinationRequests += 1;
  response.writeHead(200, { "Content-Type": "application/json" });
  response.end(JSON.stringify({ ok: true }));
});
const destinationUrl = await listen(destination);

const redirector = createServer((_request, response) => {
  response.writeHead(302, { Location: `${destinationUrl}/captured` });
  response.end();
});
const redirectorUrl = await listen(redirector);

try {
  const result = await new Promise((resolve) => {
    const child = spawn("bash", [
      publisher,
      "--project", "webClx-test",
      "--path", artifact,
      "--name", "webClx-test-source.tar.gz",
      "--base-url", redirectorUrl,
    ], {
      env: { ...process.env, WEBCLX_LOCAL_TOKEN_FILE: tokenPath },
    });
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("close", (status) => resolve({ status, stderr }));
  });

  assert.notEqual(result.status, 0, "artifact publisher must reject HTTP redirects");
  assert.match(result.stderr, /HTTP 302/);
  assert.equal(destinationRequests, 0, "artifact publisher must not follow HTTP redirects");
} finally {
  await close(redirector);
  await close(destination);
  rmSync(directory, { recursive: true, force: true });
}

console.log("security client redirect tests passed");
