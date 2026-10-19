import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const manager = fs.readFileSync("static/app-codex-account-manager.js", "utf8");
const navigation = fs.readFileSync("static/app-navigation-tabs.js", "utf8");
const app = fs.readFileSync("static/app.js", "utf8");
const routes = { oauth: "/codex_oauth", api: "/codex_api", claude: "/claude_api", codebuddy: "/codebuddy", devin: "/devin", agy: "/agy" };
for (const [kind, path] of Object.entries(routes)) {
  const calls = [];
  const window = {
    location: { pathname: path, hash: "", search: "" },
    history: Object.fromEntries(["pushState", "replaceState"].map(method => [method, (_a, _b, url) => {
      calls.push([method, url]);
      const parsed = new URL(url, "http://localhost");
      Object.assign(window.location, { pathname: parsed.pathname, search: parsed.search, hash: parsed.hash });
    }])),
  };
  const context = vm.createContext({
    window, URLSearchParams, state: { activeTab: "api", currentPath: "/workspace fixture" },
    document: { body: { classList: { contains: () => false } }, querySelectorAll: () => [] },
    renderAccounts() {}, syncAgySubtabVisibility() {}, syncCodebuddyAutoBarVisibility() {},
    syncDevinOptionsBarVisibility() {}, syncToolbarForKind() {}, syncGlobalOptionsBarVisibility() {},
  });
  vm.runInContext(manager.slice(manager.indexOf("  const ACCOUNT_PATHS"), manager.indexOf("  function textCell")), context);
  vm.runInContext(navigation.slice(0, navigation.indexOf("function setTabPanelActive")), context);
  vm.runInContext(manager.slice(manager.indexOf("  function setAccountKind("), manager.indexOf('  window.addEventListener("DOMContentLoaded"')), context);
  vm.runInContext(app.slice(0, app.indexOf("function getInitialSettingsTab")), context);
  assert.equal(window.accountKindFromUrl(), kind);
  assert.equal(window.currentAccountPathname(), path);
  assert.equal(vm.runInContext("getInitialTab()", context), "api");
  vm.runInContext("syncTabUrl()", context);
  assert.equal(window.location.pathname, path, "startup must preserve subtype");
  for (const [nextKind, nextPath] of Object.entries(routes)) {
    vm.runInContext('setAccountKind("' + nextKind + '", { push: true })', context);
    assert.equal(window.location.pathname, nextPath);
    assert.equal(new URLSearchParams(window.location.search).get("path"), "/workspace fixture");
  }
  const before = calls.length;
  vm.runInContext('setAccountKind("agy", { push: true })', context);
  assert.equal(calls.length, before, "same tab must not duplicate history");
  window.location.pathname = path;
  window.applyAccountKindFromUrl();
  assert.equal(window.currentAccountPathname(), path, "history restores subtype");
  assert.equal(calls.length, before, "history restoration must not write history");
  window.location.pathname = "/workspace";
  window.applyAccountKindFromUrl();
  assert.equal(window.currentAccountPathname(), path, "non-account routes preserve last subtype");
}
assert.match(app, /const settingsTab = getInitialSettingsTab\(\);\s*window\.applyAccountKindFromUrl\?\.\(\);/);
console.log("account tab URLs: all six deep links, switching, history and query preservation passed");
