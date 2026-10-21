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

const activationCountBeforeBackgroundCreate = activated.length;
const backgroundD = cache.getOrCreate(" session-d ");
assert.equal(backgroundD.sessionId, "session-d");
assert.equal(cache.get("session-d"), backgroundD);
assert.equal(
  cache.activeContext,
  firstA,
  "creating a background context must not replace the active terminal context",
);
assert.equal(
  activated.length,
  activationCountBeforeBackgroundCreate,
  "creating a background context must not fire the foreground activation callback",
);

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

// Bounded cache (Android DOM renderer): LRU eviction protects the active
// context, the just-returned context, and anything canEvictContext pins.
{
  const disposedIds = [];
  const createdIds = [];
  const pinned = new Set();
  const bounded = createTerminalSessionCache({
    createContext(sessionId) {
      const context = { sessionId };
      createdIds.push(sessionId);
      return context;
    },
    activateContext() {},
    disposeContext(context) {
      disposedIds.push(context.sessionId);
    },
    maxContexts: 3,
    canEvictContext: (context) => !pinned.has(context.sessionId),
  });

  bounded.activate("s1");
  bounded.activate("s2");
  bounded.activate("s3");
  assert.equal(bounded.size, 3);
  assert.equal(disposedIds.length, 0, "at-capacity cache must not evict");

  pinned.add("s1");
  bounded.activate("s4");
  assert.deepEqual(disposedIds, ["s2"], "the oldest unpinned context is evicted first");
  assert.notEqual(bounded.get("s1"), null, "a context with pending input stays pinned");
  assert.notEqual(bounded.get("s4"), null, "the newly activated context is protected");

  bounded.activate("s5");
  assert.deepEqual(disposedIds, ["s2", "s3"], "s1 is pinned so s3 evicts next");
  assert.equal(bounded.size, 3);

  // Re-opening an evicted session recreates its context from scratch.
  const recreated = bounded.activate("s2");
  assert.ok(recreated);
  assert.equal(createdIds.filter((id) => id === "s2").length, 2);
  assert.deepEqual(disposedIds, ["s2", "s3", "s4"]);

  // A freshly created background context is protected from its own trim.
  const fresh = bounded.getOrCreate("s6");
  assert.equal(bounded.get("s6"), fresh);
  assert.equal(bounded.size <= 3, true, "overflow trims an older evictable context");
  assert.deepEqual(disposedIds.at(-1), "s5");

  // Everything pinned => soft overflow is allowed until a later trim.
  ["s1", "s2", "s6"].forEach((id) => pinned.add(id));
  bounded.activate("s7");
  assert.equal(bounded.size, 4, "a fully pinned cache may exceed the budget");
  pinned.delete("s1");
  bounded.getOrCreate("s8");
  assert.ok(
    disposedIds.includes("s1"),
    "unpinning lets the next creation trim the stale entry",
  );
}

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
  /function getOrCreateBackgroundTerminalContext\(sessionId\)[\s\S]*cache\.getOrCreate\(normalizedSessionId\)/,
  "background preconnect should create a cached context without activating it",
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

// Exercise the real production eviction predicate (not a fake Set) so the
// "pending input / in-flight write / replay / retained terminal" pin list
// stays honest.
{
  const settingsSource = readFileSync(
    new URL("../static/terminal-shell-settings.js", import.meta.url),
    "utf8",
  );
  const fnStart = settingsSource.indexOf("function terminalContextSafeToEvict(");
  assert.ok(fnStart >= 0, "terminalContextSafeToEvict must exist in production");
  let fnEnd = settingsSource.indexOf("function ", fnStart + 10);
  assert.ok(fnEnd > fnStart);
  const sandbox = { activeTerminalContext: null };
  vm.createContext(sandbox);
  vm.runInContext(settingsSource.slice(fnStart, fnEnd), sandbox);
  const safe = sandbox.terminalContextSafeToEvict;

  const clean = {
    disposed: false,
    pendingInputChars: 0,
    pendingInputQueue: [],
    inputFlushTimer: null,
    outputWriteInFlight: false,
    outputDrainTimer: null,
    outputQueue: [],
    initialReplayPending: false,
    backlogReplayActive: false,
    retainedTerm: null,
  };
  assert.equal(safe(clean), true, "an idle background context is evictable");
  assert.equal(
    safe({ ...clean, pendingInputChars: 1 }),
    false,
    "unflushed keystrokes pin the context",
  );
  assert.equal(
    safe({ ...clean, pendingInputQueue: ["x"] }),
    false,
    "a queued input payload pins the context",
  );
  assert.equal(
    safe({ ...clean, inputFlushTimer: 12 }),
    false,
    "a pending input flush timer pins the context",
  );
  assert.equal(
    safe({ ...clean, outputWriteInFlight: true }),
    false,
    "an in-flight xterm write pins the context",
  );
  assert.equal(
    safe({ ...clean, outputQueue: [{ bytes: new Uint8Array([65]) }] }),
    false,
    "queued output pins the context",
  );
  assert.equal(
    safe({ ...clean, backlogReplayActive: true }),
    false,
    "an active replay pins the context",
  );
  assert.equal(
    safe({ ...clean, retainedTerm: {} }),
    false,
    "a retained terminal pins the context",
  );
}

console.log("terminal session cache tests passed");
