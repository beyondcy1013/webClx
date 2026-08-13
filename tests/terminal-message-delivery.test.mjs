import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const scripts = [
  "../.codex/skills/terminal-message/scripts/send_terminal_message.py",
  "../.codex/skills/webclx-terminal-message/scripts/send_terminal_message.py",
].map((path) => fileURLToPath(new URL(path, import.meta.url)));
const builtinWebclxMessageScript = fileURLToPath(new URL(
  "../builtin-skills/webclx-terminal-message/scripts/send_terminal_message.py",
  import.meta.url,
));

const terminalRs = readFileSync(fileURLToPath(new URL("../src/terminal.rs", import.meta.url)), "utf8");
const terminalManagerRs = readFileSync(
  fileURLToPath(new URL("../src/terminal/manager.rs", import.meta.url)),
  "utf8",
);
const codexTaskScript = readFileSync(
  "/home/root/.codex/skills/webclx-codex-task/scripts/webclx_codex_task.py",
  "utf8",
);

const verifyPollMs = Number(
  terminalRs.match(/TERMINAL_MESSAGE_VERIFY_POLL_MS:\s*u64\s*=\s*(\d+)/)?.[1],
);
const verifyPollCount = Number(
  terminalRs.match(/TERMINAL_MESSAGE_VERIFY_POLLS:\s*usize\s*=\s*(\d+)/)?.[1],
);
assert.ok(verifyPollMs * verifyPollCount >= 1500, "rollout verification must allow at least 1.5s");
assert.match(
  terminalManagerRs,
  /thread::sleep\(terminal_message_paste_settle_delay\(&data,\s*bracketed_paste\)\)/,
  "verified bracketed paste must settle before the first submit key",
);
assert.match(codexTaskScript, /"bracketed_paste": True/);
assert.match(codexTaskScript, /"verify_submission": True/);
assert.match(codexTaskScript, /"delivery_id":/);
assert.match(codexTaskScript, /submitted[\s\S]*RuntimeError/);

function dryRun(script, ...extraArgs) {
  const result = spawnSync(
    "python3",
    [
      script,
      "--target",
      "target-terminal",
      "--from",
      "sender-terminal",
      "--message",
      "reliable terminal message",
      "--dry-run",
      ...extraArgs,
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

async function sendAgainstMock(script, responseBody, options = {}) {
  let receivedPayload = null;
  let receivedHeaders = null;
  const server = createServer((request, response) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      receivedPayload = JSON.parse(body);
      receivedHeaders = request.headers;
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify(responseBody));
    });
  });
  await new Promise((resolve) => server.listen(0, options.listenHost ?? "127.0.0.1", resolve));
  const { port } = server.address();

  const result = await new Promise((resolve) => {
    const child = spawn("python3", [
      script,
      "--target",
      "target-terminal",
      "--from",
      "sender-terminal",
      "--message",
      "reliable terminal message",
      "--base-url",
      `http://${options.baseHost ?? "127.0.0.1"}:${port}`,
    ], { env: { ...process.env, ...options.env } });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("close", (status) => resolve({ status, stdout, stderr }));
  });
  await new Promise((resolve) => server.close(resolve));
  return { ...result, receivedPayload, receivedHeaders };
}

async function dryRunWithReplyPreflight(script) {
  const server = createServer((request, response) => {
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({
      sessions: [{ id: "sender-session", name: "sender-terminal", path: "project" }],
    }));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  const baseUrl = `http://127.0.0.1:${port}`;
  const result = await new Promise((resolve) => {
    const child = spawn("python3", [
      script,
      "--target",
      "target-terminal",
      "--from",
      "sender-terminal",
      "--message",
      "reply requested",
      "--request-reply",
      "--base-url",
      baseUrl,
      "--reply-base-url",
      `${baseUrl}/`,
      "--dry-run",
    ]);
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("close", (status) => resolve({ status, stdout, stderr }));
  });
  await new Promise((resolve) => server.close(resolve));
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

async function sendWithAgentDiscovery(script, { agent, target = "", startIfNeeded = false }) {
  const requests = [];
  let started = false;
  const server = createServer((request, response) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const payload = body ? JSON.parse(body) : null;
      requests.push({ method: request.method, url: request.url, payload });
      response.writeHead(200, { "Content-Type": "application/json" });
      if (request.method === "GET" && request.url === "/api/terminal/sessions?all=true") {
        response.end(JSON.stringify({
          sessions: [{
            id: "session-1",
            name: "discovered-terminal",
            path: "project",
            display_path: "/workspace/project",
            connected: true,
            activity_agent: startIfNeeded && !started ? null : agent[0].toUpperCase() + agent.slice(1),
          }],
        }));
        return;
      }
      if (request.url === "/api/terminal/auto-typed-input") {
        started = true;
        response.end(JSON.stringify({ data: `${agent}\n` }));
        return;
      }
      response.end(JSON.stringify({ ok: true, submitted: true, submit_attempts: 1 }));
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  const args = [
    script,
    "--agent",
    agent,
    "--from",
    "sender-terminal",
    "--message",
    "agent discovery message",
    "--base-url",
    `http://127.0.0.1:${port}`,
  ];
  if (target) args.push("--target", target);
  if (startIfNeeded) args.push("--start-if-needed", "--agent-start-timeout", "3");
  const result = await new Promise((resolve) => {
    const child = spawn("python3", args);
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("close", (status) => resolve({ status, stdout, stderr }));
  });
  await new Promise((resolve) => server.close(resolve));
  return { ...result, requests };
}

async function observeRedirectedLocalToken(script, tokenPath) {
  let redirectedHeaders = null;
  const destination = createServer((request, response) => {
    redirectedHeaders = request.headers;
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ sessions: [] }));
  });
  await new Promise((resolve) => destination.listen(0, "127.0.0.1", resolve));
  const destinationUrl = `http://127.0.0.1:${destination.address().port}/redirected`;

  const redirector = createServer((_request, response) => {
    response.writeHead(302, { Location: destinationUrl });
    response.end();
  });
  await new Promise((resolve) => redirector.listen(0, "127.0.0.1", resolve));

  const result = await new Promise((resolve) => {
    const child = spawn("python3", [
      script,
      "--target",
      "target-terminal",
      "--from",
      "sender-terminal",
      "--message",
      "redirect safety check",
      "--base-url",
      `http://127.0.0.1:${redirector.address().port}`,
    ], {
      env: { ...process.env, WEBCLX_LOCAL_TOKEN_FILE: tokenPath },
    });
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("close", (status) => resolve({ status, stderr }));
  });

  await new Promise((resolve) => redirector.close(resolve));
  await new Promise((resolve) => destination.close(resolve));
  return { ...result, redirectedHeaders };
}

for (const script of scripts) {
  const payload = dryRun(script);
  assert.equal(payload.submit_enters, 1, `${script} should start with one Enter`);
  assert.equal(payload.bracketed_paste, true, `${script} should frame prompt text`);
  assert.equal(payload.verify_submission, true, `${script} should verify rollout delivery`);
  assert.equal(payload.delivery_id, payload.data, `${script} should verify the exact prompt`);

  const insertOnlyPayload = dryRun(script, "--no-enter");
  assert.equal(insertOnlyPayload.submit, false);
  assert.equal(insertOnlyPayload.submit_enters, 0);
  assert.equal(insertOnlyPayload.verify_submission, false);

  const multilinePayload = dryRun(
    script,
    "--message",
    "first line\r\nsecond line\nthird line",
  );
  assert.match(multilinePayload.data, /first line second line third line$/);
  assert.doesNotMatch(multilinePayload.data, /[\r\n]/);

  const multilineInsertPayload = dryRun(
    script,
    "--no-enter",
    "--message",
    "first line\nsecond line",
  );
  assert.match(multilineInsertPayload.data, /first line\nsecond line$/);

  const replyPayload = await dryRunWithReplyPreflight(script);
  assert.match(replyPayload.data, /回复端点为 http:\/\/127\.0\.0\.1:\d+/);
  assert.match(replyPayload.data, /目标终端为 sender-terminal/);

  const missingReplyUrl = spawnSync(
    "python3",
    [
      script,
      "--target",
      "target-terminal",
      "--from",
      "sender-terminal",
      "--message",
      "reply requested",
      "--request-reply",
      "--base-url",
      "http://remote-webclx:11111",
      "--dry-run",
    ],
    { encoding: "utf8", env: { ...process.env, WEBCLX_REPLY_URL: "" } },
  );
  assert.equal(missingReplyUrl.status, 1);
  assert.match(missingReplyUrl.stderr, /requires --reply-base-url/i);

  const remoteLoopbackReply = spawnSync(
    "python3",
    [
      script,
      "--target",
      "target-terminal",
      "--from",
      "sender-terminal",
      "--message",
      "reply requested",
      "--request-reply",
      "--base-url",
      "http://remote-webclx:11111",
      "--reply-base-url",
      "http://127.0.0.1:11111",
      "--dry-run",
    ],
    { encoding: "utf8" },
  );
  assert.equal(remoteLoopbackReply.status, 1);
  assert.match(remoteLoopbackReply.stderr, /cannot reply to a loopback/i);

  const failed = await sendAgainstMock(script, {
    ok: true,
    submitted: false,
    submit_attempts: 4,
  });
  assert.equal(failed.status, 1, `${script} must fail when rollout delivery is unconfirmed`);
  assert.match(failed.stderr, /not confirmed/i);
  assert.equal(failed.receivedPayload.verify_submission, true);

  const delivered = await sendAgainstMock(script, {
    ok: true,
    submitted: true,
    submit_attempts: 1,
  });
  assert.equal(delivered.status, 0, delivered.stderr);

  const discovered = await sendWithAgentDiscovery(script, { agent: "codex" });
  assert.equal(discovered.status, 0, discovered.stderr);
  const discoveredMessage = discovered.requests.find(
    (request) => request.url === "/api/terminal/sessions/message",
  );
  assert.equal(discoveredMessage.payload.target, "session-1");
  assert.equal(
    discovered.requests.some((request) => request.url === "/api/terminal/auto-typed-input"),
    false,
  );

  const started = await sendWithAgentDiscovery(script, {
    agent: "claude",
    target: "discovered-terminal",
    startIfNeeded: true,
  });
  assert.equal(started.status, 0, started.stderr);
  const startRequest = started.requests.find(
    (request) => request.url === "/api/terminal/auto-typed-input",
  );
  assert.deepEqual(startRequest.payload, {
    session_id: "session-1",
    command_line: "claude",
  });
  const startedMessage = started.requests.find(
    (request) => request.url === "/api/terminal/sessions/message",
  );
  assert.equal(startedMessage.payload.target, "session-1");
}

const webclxMessageScript = scripts[1];
const tokenDirectory = mkdtempSync(join(tmpdir(), "webclx-terminal-message-token-"));
const tokenPath = join(tokenDirectory, "local-token");
const token = "0123456789abcdef".repeat(4);
writeFileSync(tokenPath, `${token}\n`, { mode: 0o600 });
try {
  const localDelivery = await sendAgainstMock(
    webclxMessageScript,
    { ok: true, submitted: true },
    { env: { WEBCLX_LOCAL_TOKEN_FILE: tokenPath } },
  );
  assert.equal(localDelivery.status, 0, localDelivery.stderr);
  assert.equal(localDelivery.receivedHeaders["x-webclx-local-token"], token);

  const remoteDelivery = await sendAgainstMock(
    webclxMessageScript,
    { ok: true, submitted: true },
    {
      listenHost: "0.0.0.0",
      baseHost: "127.0.0.2",
      env: {
        WEBCLX_LOCAL_TOKEN_FILE: tokenPath,
        NO_PROXY: "*",
        no_proxy: "*",
      },
    },
  );
  assert.equal(remoteDelivery.status, 0, remoteDelivery.stderr);
  assert.equal(remoteDelivery.receivedHeaders["x-webclx-local-token"], undefined);

  for (const script of [webclxMessageScript, builtinWebclxMessageScript]) {
    const redirected = await observeRedirectedLocalToken(script, tokenPath);
    assert.notEqual(redirected.status, 0, `${script} must reject HTTP redirects`);
    assert.equal(redirected.redirectedHeaders, null, `${script} must not follow HTTP redirects`);
  }
} finally {
  rmSync(tokenDirectory, { recursive: true, force: true });
}

console.log("terminal message delivery tests passed");
