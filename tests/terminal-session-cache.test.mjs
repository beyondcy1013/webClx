import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/terminal-session-cache.js", import.meta.url),
  "utf8",
);
const sandbox = { globalThis: {} };
vm.runInNewContext(source, sandbox);

const { createTerminalSessionCache } = sandbox.globalThis.WebClxTerminalSessionCache || {};
assert.equal(typeof createTerminalSessionCache, "function");

const created = [];
const activated = [];
const disposed = [];
const cache = createTerminalSessionCache({
  createContext(sessionId) {
    const context = { sessionId, marker: Symbol(sessionId) };
    created.push(context);
    return context;
  },
  activateContext(context, previousContext) {
    activated.push({ context, previousContext });
  },
  disposeContext(context) {
    disposed.push(context);
  },
});

const firstA = cache.activate("session-a");
const firstB = cache.activate("session-b");
const secondA = cache.activate("session-a");

assert.equal(firstA, secondA, "switching back must reuse the cached terminal context");
assert.equal(created.length, 2, "each visited session should create exactly one context");
assert.equal(disposed.length, 0, "switching sessions must not dispose background contexts");
assert.equal(activated[1].previousContext, firstA);
assert.equal(activated[2].previousContext, firstB);

cache.remove("session-a");
assert.deepEqual(disposed, [firstA], "removing a session should release its terminal context");
assert.equal(cache.get("session-a"), null);

cache.activate("session-c");
cache.prune(new Set(["session-c"]));
assert.equal(cache.get("session-b"), null, "pruning should close contexts for vanished sessions");
assert.notEqual(cache.get("session-c"), null);

cache.clear();
assert.equal(cache.size, 0);
assert.equal(disposed.at(-1)?.sessionId, "session-c");

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const shellSource = readFileSync(
  new URL("../static/terminal-shell-settings.js", import.meta.url),
  "utf8",
);
const connectionSource = readFileSync(
  new URL("../static/terminal-layout-connection.js", import.meta.url),
  "utf8",
);
const inputSource = readFileSync(
  new URL("../static/terminal-input-transport.js", import.meta.url),
  "utf8",
);
const outputSource = readFileSync(
  new URL("../static/terminal-output-scroll.js", import.meta.url),
  "utf8",
);
const sessionsSource = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);

assert.ok(
  terminalHtml.indexOf("/assets/terminal-session-cache.js") <
    terminalHtml.indexOf("/assets/terminal-shell-settings.js"),
  "the cache registry must load before terminal instance helpers use it",
);
assert.match(
  shellSource,
  /function activateTerminalSessionContext\(sessionId[\s\S]*cache\.activate\(normalizedSessionId\)[\s\S]*restoreCachedTerminalViewport\(context\)/,
  "activating a session should reuse its cached xterm and restore its viewport immediately",
);
assert.match(
  shellSource,
  /function activateTerminalContext\(context, previousContext\)[\s\S]*sendTerminalContextVisibility\(previousContext, false\)[\s\S]*sendTerminalContextVisibility\(context, true\)/,
  "switching should mark the hidden socket as background and the selected socket as visible",
);
assert.match(
  inputSource,
  /function websocketUrl\(context = activeTerminalContext\)[\s\S]*query\.set\("visible", terminalContextOutputVisible\(context\) \? "true" : "false"\)/,
  "new and reconnecting sockets should declare their effective browser visibility",
);
assert.match(
  connectionSource,
  /function connectTerminal\(targetContext = null\)[\s\S]*targetContext \|\| activateTerminalSessionContext\(state\.activeSessionId\)[\s\S]*terminalContextSocketOpen\(context\)[\s\S]*return;/,
  "switching back to a connected cached session should not reconnect or replay history",
);
const connectStart = connectionSource.indexOf("function connectTerminal(targetContext = null)");
const selectStart = connectionSource.indexOf("function selectSession(", connectStart);
assert.doesNotMatch(
  connectionSource.slice(connectStart, selectStart),
  /closeSocket\(/,
  "ordinary session switching must leave the previous session websocket alive",
);
assert.match(
  connectionSource,
  /queueTerminalOutput\(bytes, token, context\)/,
  "each websocket must route output to the context that owns that connection",
);
assert.match(
  outputSource,
  /function queueTerminalOutput\([\s\S]*context\.outputQueue\.push\([\s\S]*drainTerminalOutputQueue\(context\)/,
  "background output should drain into the owning cached xterm instead of the active global terminal",
);
assert.match(
  outputSource,
  /function drainTerminalOutputQueue\([\s\S]*context\.term\.write\(/,
  "the owning context's xterm should receive queued output",
);
assert.match(
  sessionsSource,
  /disposeTerminalSessionContext\(session\.id\)/,
  "ending a session should release its cached xterm and websocket",
);
assert.match(
  sessionsSource,
  /connect:[\s\S]*!isTerminalConnected\(\) \|\| targetSession\.id !== activeTerminalContext\?\.sessionId/,
  "history navigation should compare the target with the mounted context, not only state.activeSessionId",
);

console.log("terminal session cache tests passed");
