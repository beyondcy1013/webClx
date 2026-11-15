import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import test from "node:test";

const tick = () => new Promise(resolve => setImmediate(resolve));
function fixture() {
  const nodes = new Map(["codearts-options-bar", "codearts-option-full-access", "codearts-options-note"].map(id =>
    [id, { hidden: true, checked: true, disabled: true, dataset: {}, addEventListener(event, fn) { this[event] = fn; } }]));
  const calls = [];
  const window = { addEventListener(_event, fn) { this.ready = fn; } };
  vm.runInNewContext(fs.readFileSync("static/app-codearts-options.js", "utf8"), {
    window, document: { getElementById: id => nodes.get(id) },
    requestJson: (url, options) => new Promise((resolve, reject) => calls.push({ url, options, resolve, reject })),
  });
  window.ready();
  return { nodes, calls, select: window.codeartsOptions.select, input: nodes.get("codearts-option-full-access"), note: nodes.get("codearts-options-note") };
}

test("CodeArts first selection applies the default native highest permissions", async () => {
  const f = fixture(); const pending = f.select("codearts");
  assert.equal(f.input.disabled, true);
  f.calls[0].resolve({ initialized: false, allowAllPermissions: true, applied: false });
  await tick();
  assert.equal(f.calls[1].url, "/api/auth/codearts-options");
  assert.equal(f.calls[1].options.method, "PUT");
  assert.deepEqual(JSON.parse(f.calls[1].options.body), { allow_all_permissions: true });
  f.calls[1].resolve({ initialized: true, allowAllPermissions: true, applied: true });
  await pending;
  assert.equal(f.input.checked, true); assert.equal(f.input.disabled, false);
  assert.match(f.note.textContent, /重新启动原生 CodeArts/);
});

test("explicitly disabled settings persist across tab selection without auto-enabling", async () => {
  const f = fixture(); const pending = f.select("codearts");
  f.calls[0].resolve({ initialized: true, allowAllPermissions: false, applied: true });
  await pending;
  assert.equal(f.calls.length, 1); assert.equal(f.input.checked, false);
  f.input.checked = true; const save = f.input.change();
  f.calls[1].resolve({ initialized: true, allowAllPermissions: true, applied: true }); await save;
  f.input.checked = false; const disable = f.input.change();
  assert.equal(JSON.parse(f.calls[2].options.body).allow_all_permissions, false);
  f.calls[2].resolve({ initialized: true, allowAllPermissions: false, applied: true }); await disable;
  assert.match(f.note.textContent, /已恢复原权限配置/);
});

test("save errors revert the checkbox and retain the actual error", async () => {
  const f = fixture(); const pending = f.select("codearts");
  f.calls[0].resolve({ initialized: true, allowAllPermissions: true, applied: true }); await pending;
  f.input.checked = false; const disable = f.input.change();
  f.calls[1].reject(new Error("native config invalid")); await disable;
  assert.equal(f.input.checked, true); assert.equal(f.input.disabled, false);
  assert.match(f.note.textContent, /native config invalid/);
});

test("stale read cannot write native settings after switching away", async () => {
  const f = fixture(); const pending = f.select("codearts");
  await f.select("codebuddy");
  f.calls[0].resolve({ initialized: false, allowAllPermissions: true, applied: false }); await pending;
  assert.equal(f.calls.length, 1); assert.equal(f.nodes.get("codearts-options-bar").hidden, true);
});

test("stale save cannot overwrite the state of a newer tab load", async () => {
  const f = fixture(); const first = f.select("codearts");
  f.calls[0].resolve({ initialized: false, allowAllPermissions: true, applied: false }); await tick();
  await f.select("opencode"); const second = f.select("codearts");
  f.calls[2].resolve({ initialized: true, allowAllPermissions: false, applied: true }); await second;
  f.calls[1].resolve({ initialized: true, allowAllPermissions: true, applied: true }); await first;
  assert.equal(f.input.checked, false); assert.equal(f.input.disabled, false);
});

test("enabled but incomplete settings are repaired", async () => {
  const f = fixture(); const pending = f.select("codearts");
  f.calls[0].resolve({ initialized: true, allowAllPermissions: true, applied: false }); await tick();
  assert.equal(f.calls.length, 2);
  f.calls[1].resolve({ initialized: true, allowAllPermissions: true, applied: true }); await pending;
  assert.equal(f.note.dataset.tone, "ok");
});

test("HTML scopes the default checked control and loads its script before tab integration", () => {
  const html = fs.readFileSync("static/index.html", "utf8");
  assert.match(html, /id="codearts-options-bar"[^>]*data-account-kinds="codearts"[^>]*hidden/);
  assert.match(html, /id="codearts-option-full-access"[^>]*checked[^>]*disabled/);
  assert.ok(html.indexOf("app-codearts-options.js?") < html.indexOf("app-native-login-backup.js?"));
});
