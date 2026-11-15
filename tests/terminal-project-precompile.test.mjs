import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const html = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const source = readFileSync(new URL("../static/terminal-project-precompile.js", import.meta.url), "utf8");
const mobile = readFileSync(new URL("../static/terminal-mobile-keys.js", import.meta.url), "utf8");
const identity = mobile.slice(mobile.indexOf("async function resolveCurrentDeploySourceTerminal()"),
  mobile.indexOf("async function triggerProjectDeploy()"));

function harness({ configStatus = 404, config, request, fetchConfig, state } = {}) {
  const calls = [], statuses = [];
  const context = vm.createContext({
    state: state || { activeSessionId: "s1", currentPath: "apps/示例 项目", workspaceDir: "/home/codes/" },
    normalizeRelativePath: (value) => value.replace(/^\/+|\/+$/g, ""),
    updateStatus: (message, level) => statuses.push({ message, level }),
    fetch: fetchConfig || (async (url) => {
      calls.push({ url, method: "GET" });
      return { status: configStatus, ok: configStatus === 200,
        text: async () => "config denied",
        json: async () => ({ content: typeof config === "string" ? config : JSON.stringify(config) }) };
    }),
    requestJson: async (url, options = {}) => {
      calls.push({ url, ...options });
      if (request) return request(url, options);
      if (url.startsWith("/api/terminal/sessions")) {
        return { sessions: [{ id: "s1", name: "重命名后的终端" }] };
      }
      return { queued: true, request_id: "warmup-123", command: ["cargo", "build", "--release"] };
    },
  });
  vm.runInContext(`${identity}\n${source}`, context);
  return { context, calls, statuses, posts: () => calls.filter((call) => call.method === "POST") };
}

test("menu and select put precompile immediately below deploy and dispatch it", () => {
  assert.match(html, /data-project-action="deploy_project"[^>]*>部署项目<[/]button>\s*<button[^>]*data-project-action="precompile_project"/);
  assert.match(html, /<option value="deploy_project"[^>]*>[^<]*<[/]option>\s*<option value="precompile_project">预编译预热<[/]option>/);
  assert.match(html, /terminal-project-precompile\.js\?v=20261114-precompile2/);
  assert.match(html, /terminal-mobile-keys\.js\?v=20261115-slash-draft/);
  const start = mobile.indexOf("function runTerminalProjectCommandAction(action)");
  const end = mobile.indexOf("\n}\n", start) + 2;
  let dispatched = 0;
  const context = vm.createContext({ triggerProjectPrecompile: () => dispatched++ });
  vm.runInContext(mobile.slice(start, end), context);
  context.runTerminalProjectCommandAction("precompile_project");
  assert.equal(dispatched, 1);
});

test("default build uses compile queue with current project and refreshed callback identity", async () => {
  const fixture = harness();
  await fixture.context.triggerProjectPrecompile();
  assert.equal(fixture.posts().length, 1);
  assert.equal(fixture.posts()[0].url, "/api/build/compile");
  const payload = JSON.parse(fixture.posts()[0].body);
  assert.equal(payload.project_dir, "/home/codes/apps/示例 项目");
  assert.equal(payload.project, "示例 项目");
  assert.equal(payload.source_terminal_name, "重命名后的终端");
  assert.equal(payload.source_terminal_id, "s1");
  assert.equal(payload.source_tmux_session, "webclx_s1");
  assert.equal(payload.purpose, "precompile");
  for (const field of ["command", "install_command", "audit_paths", "env"]) assert.equal(field in payload, false);
  assert.match(fixture.statuses.at(-1).message, /已入队.*warmup-123/);
  assert.equal(fixture.calls.filter((call) => call.url.includes("/requests/")).length, 0);
});

test("project configuration preserves custom target argv and artifact requirements only", async () => {
  const command = ["cargo", "build", "--release", "--locked", "--target", "aarch64-linux-android"];
  const fixture = harness({ configStatus: 200, config: {
    web: { port: 8080 }, precompile: { command, required_artifacts: ["build/a b.apk"],
      install_command: ["must-not-run"], env: { CARGO_TARGET_DIR: "/wrong" } },
  } });
  await fixture.context.triggerProjectPrecompile();
  const payload = JSON.parse(fixture.posts()[0].body);
  assert.deepEqual(payload.command, command);
  assert.deepEqual(payload.required_artifacts, ["build/a b.apk"]);
  assert.equal("install_command" in payload, false);
  assert.equal("env" in payload, false);
});

test("existing web-only project config continues to use build inference", async () => {
  const fixture = harness({ configStatus: 200, config: { web: { port: 8080 } } });
  await fixture.context.triggerProjectPrecompile();
  assert.equal("command" in JSON.parse(fixture.posts()[0].body), false);
});

test("terminal switch during lookup cannot change project or callback target", async () => {
  let resolveSessions;
  const fixture = harness({ request: async (url) => {
    if (url.startsWith("/api/terminal/sessions")) return new Promise((resolve) => { resolveSessions = resolve; });
    return { queued: true };
  } });
  const pending = fixture.context.triggerProjectPrecompile();
  fixture.context.state.activeSessionId = "s2";
  fixture.context.state.currentPath = "another-project";
  resolveSessions({ sessions: [{ id: "s1", name: "origin" }, { id: "s2", name: "other" }] });
  await pending;
  const payload = JSON.parse(fixture.posts()[0].body);
  assert.equal(payload.source_terminal_id, "s1");
  assert.equal(payload.project_dir, "/home/codes/apps/示例 项目");
});

test("double click submits once while waiting and allows a later intentional build", async () => {
  let release;
  const fixture = harness({ fetchConfig: () => new Promise((resolve) => { release = resolve; }) });
  const pending = fixture.context.triggerProjectPrecompile();
  await fixture.context.triggerProjectPrecompile();
  await Promise.resolve();
  release({ status: 404 });
  await pending;
  assert.equal(fixture.posts().length, 1);
  fixture.context.fetch = async () => ({ status: 404 });
  await fixture.context.triggerProjectPrecompile();
  assert.equal(fixture.posts().length, 2);
});

test("unreadable or malformed project configuration never silently queues another build", async () => {
  for (const options of [
    { configStatus: 403 }, { configStatus: 500 },
    ...["broken-json", null, [], { precompile: null }, { precompile: {} },
      { precompile: { command: [] } }, { precompile: { command: "cargo build" } },
      { precompile: { command: ["cargo", 12] } },
      { precompile: { command: ["cargo"], required_artifacts: [""] } }]
      .map((config) => ({ configStatus: 200, config })),
  ]) {
    const fixture = harness(options);
    await fixture.context.triggerProjectPrecompile();
    assert.equal(fixture.posts().length, 0, JSON.stringify(options));
    assert.equal(fixture.statuses.at(-1).level, "warn");
  }
});

test("missing workspace, session or authoritative identity prevents builds", async () => {
  for (const state of [{ workspaceDir: "", activeSessionId: "s1" },
    { workspaceDir: "/home/codes", activeSessionId: "" }]) {
    const fixture = harness({ state });
    await fixture.context.triggerProjectPrecompile();
    assert.equal(fixture.calls.length, 0);
  }
  const fixture = harness({ request: async () => ({ sessions: [] }) });
  await fixture.context.triggerProjectPrecompile();
  assert.equal(fixture.posts().length, 0);
});

test("failed and unknown queue responses are visible with no automatic retry", async () => {
  for (const fail of [true, false]) {
    const fixture = harness({ request: async (url) => {
      if (url.startsWith("/api/terminal/sessions")) return { sessions: [{ id: "s1", name: "origin" }] };
      if (fail) throw new Error("queue unavailable");
      return { queued: false };
    } });
    await fixture.context.triggerProjectPrecompile();
    assert.equal(fixture.posts().length, 1);
    assert.equal(fixture.statuses.at(-1).level, "warn");
    if (fail) assert.match(fixture.statuses.at(-1).message, /queue unavailable/);
  }
});

test("workspace root project retains a valid absolute directory", async () => {
  for (const root of ["/home/codes/", "/"]) {
    const fixture = harness({ state: { activeSessionId: "s1", currentPath: "", workspaceDir: root } });
    await fixture.context.triggerProjectPrecompile();
    assert.equal(JSON.parse(fixture.posts()[0].body).project_dir, root === "/" ? "/" : "/home/codes");
  }
});
