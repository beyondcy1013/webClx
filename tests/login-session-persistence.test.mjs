import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const loginJs = readFileSync(new URL("../static/login.js", import.meta.url), "utf8");

async function runLoginPage(sessionResponse) {
  const listeners = new Map();
  const assigned = [];
  const fetchCalls = [];
  const elements = {
    "login-form": {
      addEventListener(type, listener) {
        listeners.set(type, listener);
      },
    },
    "login-username": {
      value: "",
      focused: false,
      focus() {
        this.focused = true;
      },
    },
    "login-password": { value: "" },
    "login-error": { hidden: true, textContent: "" },
    "login-submit": { disabled: false, textContent: "登录" },
  };
  const context = vm.createContext({
    URLSearchParams,
    document: {
      getElementById(id) {
        return elements[id] ?? null;
      },
    },
    fetch: async (...args) => {
      fetchCalls.push(args);
      return sessionResponse;
    },
    window: {
      location: {
        search: "?next=%2Fterminal%3Fpath%3D%252Fhome%252Fcodes%252FwebClx",
        assign(target) {
          assigned.push(target);
        },
      },
    },
  });

  vm.runInContext(loginJs, context);
  await new Promise((resolve) => setImmediate(resolve));

  return { assigned, elements, fetchCalls, listeners };
}

test("an unexpired session leaves the login page without asking for credentials again", async () => {
  const result = await runLoginPage({
    ok: true,
    json: async () => ({ authenticated: true, user: "beyondcy" }),
  });

  assert.equal(result.fetchCalls.length, 1);
  assert.equal(result.fetchCalls[0][0], "/api/auth/session");
  assert.deepEqual(result.assigned, ["/terminal?path=%2Fhome%2Fcodes%2FwebClx"]);
});

test("an expired session keeps the login form available", async () => {
  const result = await runLoginPage({
    ok: true,
    json: async () => ({ authenticated: false, user: null }),
  });

  assert.equal(result.fetchCalls.length, 1);
  assert.deepEqual(result.assigned, []);
  assert.equal(result.elements["login-username"].focused, true);
  assert.equal(result.listeners.has("submit"), true);
});
