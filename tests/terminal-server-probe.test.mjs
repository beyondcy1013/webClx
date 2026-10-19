import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/terminal-server-probe.js", import.meta.url),
  "utf8",
);

const PROBE_TIMEOUT_MS = 3500;
const GRACE_PERIOD_MS = 60000;

function createSandbox({ currentOrigin = "http://HOST:PORT", candidates = [] } = {}) {
  const timers = new Map();
  let nextTimerId = 1;
  const clock = { now: 1_000_000_000 };
  const fetches = [];
  const navigations = [];
  let bridgeProbeCalls = 0;

  const select = {
    querySelectorAll() {
      return candidates.map((value) => ({ value }));
    },
  };

  const sandbox = {
    console,
    URL,
    AbortController,
    performance: { now: () => clock.now },
    Date: { now: () => clock.now },
    document: { getElementById: (id) => (id === "terminal-server-switch-select" ? select : null) },
    window: {
      location: { origin: currentOrigin },
      WebClxAndroid: {
        probeHost() {
          bridgeProbeCalls += 1;
          return JSON.stringify({ ok: true, latency: 1 });
        },
      },
    },
    navigateToTerminalServer(host) {
      navigations.push(host);
    },
    setTimeout(callback, delay) {
      const id = nextTimerId++;
      timers.set(id, { callback, delay });
      return id;
    },
    clearTimeout(id) {
      timers.delete(id);
    },
    fetch(url, options = {}) {
      const entry = {
        url: String(url),
        options,
        resolve: null,
        reject: null,
        promise: null,
      };
      entry.promise = new Promise((resolve, reject) => {
        entry.resolve = resolve;
        entry.reject = reject;
        if (options.signal) {
          options.signal.addEventListener("abort", () =>
            reject(Object.assign(new Error("aborted"), { name: "AbortError" })),
          );
        }
      });
      fetches.push(entry);
      return entry.promise;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  const probe = vm.runInContext("TERMINAL_SERVER_PROBE", sandbox);
  return {
    probe,
    sandbox,
    timers,
    clock,
    fetches,
    navigations,
    bridgeCalls: () => bridgeProbeCalls,
    resolveFetch(predicate) {
      for (const entry of fetches) {
        if (predicate(entry.url)) entry.resolve({});
      }
    },
    rejectFetch(predicate) {
      for (const entry of fetches) {
        if (predicate(entry.url)) entry.reject(new Error("unreachable"));
      }
    },
  };
}

const nextTick = () => new Promise((resolve) => setImmediate(resolve));

test("the probe module never calls the removed synchronous Android bridge", async () => {
  const env = createSandbox();
  const pending = env.probe.reevaluate("test");
  env.resolveFetch(() => true);
  assert.equal(await pending, false);
  assert.equal(env.bridgeCalls(), 0, "bridge.probeHost must never be invoked");
});

test("a healthy current server never triggers a switch", async () => {
  const env = createSandbox({ candidates: ["example.invalid:11112"] });
  const pending = env.probe.reevaluate("test");
  env.resolveFetch(() => true);
  assert.equal(await pending, false);
  assert.deepEqual(env.navigations, []);
});

test("a short outage inside the grace window prevents switching", async () => {
  const env = createSandbox({ candidates: ["alt.example:11111"] });
  let pending = env.probe.reevaluate("first");
  env.rejectFetch(() => true);
  assert.equal(await pending, false);

  env.clock.now += GRACE_PERIOD_MS - 1000;
  const countBefore = env.fetches.length;
  pending = env.probe.reevaluate("second");
  env.rejectFetch(() => true);
  assert.equal(await pending, false);
  // Still inside the grace period: only the current origin is re-probed,
  // alternatives are never evaluated.
  assert.equal(env.fetches.length, countBefore + 1);
  assert.deepEqual(env.navigations, []);
});

test("a sustained outage switches to the fastest reachable candidate", async () => {
  const env = createSandbox({
    candidates: ["slow.example:11111", "fast.example:11111", "down.example:11111"],
  });
  let pending = env.probe.reevaluate("first");
  env.rejectFetch(() => true);
  assert.equal(await pending, false);

  env.clock.now += GRACE_PERIOD_MS + 1000;
  pending = env.probe.reevaluate("second");
  const isCurrent = (url) => url.startsWith("http://HOST:PORT/");
  env.rejectFetch(isCurrent);
  await nextTick();
  // findBestServer re-probes the current origin before racing candidates.
  env.rejectFetch(isCurrent);
  await nextTick();
  // Candidate probes launch concurrently: all three fetches must exist
  // before any of them resolves.
  const candidates = env.fetches.filter((entry) => !isCurrent(entry.url));
  assert.equal(candidates.length, 3, "all alternatives are probed concurrently");
  env.fetches.find((entry) => entry.url.startsWith("http://fast.example")).resolve({});
  await nextTick(); // let the fast probe record its latency before time advances
  env.clock.now += 40;
  env.fetches.find((entry) => entry.url.startsWith("http://slow.example")).resolve({});
  env.fetches.find((entry) => entry.url.startsWith("http://down.example")).reject(new Error("down"));
  assert.equal(await pending, true);
  assert.deepEqual(env.navigations, ["fast.example:11111"]);
});

test("the abort timer is released on success, failure, and abort", async () => {
  const env = createSandbox();
  const ok = env.probe.reevaluate("ok");
  env.resolveFetch(() => true);
  await ok;
  assert.equal(env.timers.size, 0, "success clears its abort timer");

  const fail = env.probe.reevaluate("fail");
  env.rejectFetch(() => true);
  await fail;
  assert.equal(env.timers.size, 0, "failure clears its abort timer");

  env.probe.findBestServer("abort");
  await nextTick();
  // Force the timeout path: fire every pending abort timer.
  for (const [id, timer] of [...env.timers]) {
    env.timers.delete(id);
    timer.callback();
  }
  await nextTick();
  await nextTick();
  assert.equal(env.timers.size, 0, "aborted probes leave no timers behind");
});

test("concurrent reevaluate calls coalesce onto one probe round", async () => {
  const env = createSandbox();
  const first = env.probe.reevaluate("a");
  const second = env.probe.reevaluate("b");
  env.resolveFetch(() => true);
  assert.equal(await first, false);
  assert.equal(await second, false);
  const probeFetches = env.fetches.filter((entry) => entry.url.includes("_probe="));
  assert.equal(probeFetches.length, 1, "coalesced callers share a single fetch round");
});
