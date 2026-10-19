import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/app-settings-formatters.js", import.meta.url),
  "utf8",
);
const context = vm.createContext({});
vm.runInContext(source, context);

function normalizeActiveWindow(value) {
  return vm.runInContext(`normalizeTerminalAutoContinueActiveWindow(${JSON.stringify(value)})`, context);
}

function normalizeAvoidWindow(value) {
  return vm.runInContext(`normalizeTerminalScheduledInputAvoidWindow(${JSON.stringify(value)})`, context);
}

test("accepts 24:00 only as the auto-continue window endpoint", () => {
  assert.equal(normalizeActiveWindow("00:00-24:00"), "00:00-24:00");
  assert.equal(normalizeActiveWindow("00:00-24:01"), "");
  assert.equal(normalizeActiveWindow("24:00-08:00"), "");
  assert.equal(normalizeAvoidWindow("00:00-24:00"), "");
});
