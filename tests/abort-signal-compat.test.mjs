import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const compatSource = readFileSync(
  new URL("../static/abort-signal-compat.js", import.meta.url),
  "utf8",
);
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");
const agentProfilesHtml = readFileSync(new URL("../static/agent-profiles.html", import.meta.url), "utf8");

test("AbortSignal compatibility layer restores any and timeout when missing", async () => {
  const sandboxSignal = AbortSignal;
  const originalAny = sandboxSignal.any;
  const originalTimeout = sandboxSignal.timeout;
  const sandbox = {
    AbortController,
    AbortSignal: sandboxSignal,
    DOMException,
    setTimeout,
  };
  sandbox.window = sandbox;
  try {
    sandboxSignal.any = undefined;
    sandboxSignal.timeout = undefined;
    vm.runInNewContext(compatSource, sandbox);

    assert.equal(typeof sandboxSignal.any, "function");
    assert.equal(typeof sandboxSignal.timeout, "function");

    const source = new AbortController();
    const combined = sandboxSignal.any([source.signal]);
    assert.equal(combined.aborted, false);
    source.abort("compat-test");
    assert.equal(combined.aborted, true);
    assert.equal(combined.reason, "compat-test");
  } finally {
    sandboxSignal.any = originalAny;
    sandboxSignal.timeout = originalTimeout;
  }
});

test("compatibility layer loads before page feature scripts", () => {
  assert.match(indexHtml, /<script defer src="\/assets\/abort-signal-compat\.js\?v=[^"]+"><\/script>/);
  assert.match(terminalHtml, /<script defer src="\/assets\/abort-signal-compat\.js\?v=[^"]+"><\/script>/);
  assert.match(agentHtml, /<script defer src="\/assets\/abort-signal-compat\.js\?v=[^"]+"><\/script>/);
  assert.match(agentProfilesHtml, /<script defer src="\/assets\/abort-signal-compat\.js\?v=[^"]+"><\/script>/);
});
