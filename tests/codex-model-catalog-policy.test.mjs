import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const source = read("static/app-model-catalog-policy.js");
const sandbox = { document: { getElementById: () => null } };
vm.runInNewContext(source, sandbox);
const create = sandbox.WebClxModelCatalogPolicy.create;
const element = () => ({ disabled: true, checked: false, dataset: {}, addEventListener() {} });

function fixture(request) {
  const controls = { checkbox: element(), saveButton: element(), refreshButton: element(), status: element() };
  return { ...controls, controller: create({ ...controls, request }) };
}

test("global policy loads, saves both values, and does not apply any preset", async () => {
  const calls = [];
  const f = fixture(async (url, options) => {
    calls.push([url, options]);
    return options ? JSON.parse(options.body) : { suppress_comp_hash_changes: false };
  });
  await f.controller.load();
  assert.equal(f.checkbox.disabled, false);
  for (const enabled of [true, false]) {
    f.checkbox.checked = enabled;
    await f.controller.save();
    assert.equal(JSON.parse(calls.at(-1)[1].body).suppress_comp_hash_changes, enabled);
    assert.equal(f.checkbox.checked, enabled);
    assert.match(f.status.textContent, /下次应用.*重新启动 Codex/);
  }
  assert.ok(calls.every(([url]) => url === "/api/auth/model-catalog-policy"));
});

test("failed or invalid loading never presents a writable default", async () => {
  for (const response of [null, {}, { suppress_comp_hash_changes: "false" }]) {
    const f = fixture(async () => response);
    await f.controller.load();
    assert.equal(f.saveButton.disabled, true);
    assert.equal(f.checkbox.disabled, true);
    assert.equal(f.refreshButton.disabled, false);
    assert.equal(f.status.dataset.tone, "error");
  }
  const f = fixture(async () => { throw new Error("offline"); });
  await f.controller.load();
  assert.match(f.status.textContent, /offline/);
  assert.equal(f.saveButton.disabled, true);
});

test("failed save is visible and concurrent saves are suppressed", async () => {
  let release;
  let writes = 0;
  const f = fixture(async (_url, options) => {
    if (!options) return { suppress_comp_hash_changes: false };
    writes++;
    await new Promise((resolve) => { release = resolve; });
    throw new Error("disk full");
  });
  await f.controller.load();
  const save = f.controller.save();
  await f.controller.save();
  assert.equal(writes, 1);
  assert.equal(f.saveButton.disabled, true);
  release();
  await save;
  assert.match(f.status.textContent, /保存失败.*disk full/);
  assert.equal(f.saveButton.disabled, false);
});

test("UI documents scope and startup semantics; every catalog write passes the policy", () => {
  const html = read("static/index.html");
  assert.match(html, /app-model-catalog-policy\.js\?v=/);
  assert.equal((html.match(/id="codex-suppress-comp-hash"/g) || []).length, 1);
  assert.match(html, /全部 Codex_API 预设/);
  assert.match(html, /上下文超限或切换到更小窗口仍可能压缩/);
  assert.match(html, /不会使不同模型的加密压缩内容互相兼容/);
  assert.match(read("src/auth.rs"), /model_catalog_policy::load\(state\)/);
  assert.match(read("src/auth/apply.rs"), /sync_api_model_catalogs\(state,/);
  assert.match(read("src/auth/apply.rs"), /super::model_catalog_policy::load\(state\)/);
  assert.match(read("src/routes/auth.rs"), /get\(auth::get_model_catalog_policy\)\.put\(auth::save_model_catalog_policy\)/);
});
