import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const nodes = new Map();
for (const id of ["native-login-status", "native-login-read", "native-login-download", "native-login-codearts-auth"]) {
  nodes.set(id, { dataset: {}, removeAttribute(name) { delete this[name]; }, addEventListener(event, fn) { this[event] = fn; } });
}
const pending = [];
const downloads = [];
const inspections = [];
const window = {
  addEventListener(_event, fn) { this.ready = fn; },
  updateNativeLoginAccounts(kind, result) { inspections.push({ kind, result }); },
};
const document = {
  getElementById: id => nodes.get(id),
  body: { appendChild() {} },
  createElement() { return { click() { downloads.push(this.download); }, remove() {} }; },
};
const app = fs.readFileSync("static/app.js", "utf8");
vm.runInNewContext(fs.readFileSync("static/app-native-login-backup.js", "utf8"), {
  window, document, Blob, URL: { createObjectURL: () => "blob:backup", revokeObjectURL() {} },
  buildTerminalRunUrl: () => { throw new Error("CodeArts auth settings must not launch a terminal"); },
  setTimeout: fn => fn(),
  requestJson: (url, options) => new Promise((resolve, reject) => pending.push({ url, options, resolve, reject })),
});
window.ready();
window.nativeLoginBackup.select("oauth");
window.nativeLoginBackup.select("opencode");
assert.equal(nodes.get("native-login-codearts-auth").hidden, true, "CodeArts login must stay scoped to CodeArts");
pending[1].resolve({ available: true });
await new Promise(resolve => setImmediate(resolve));
assert.match(nodes.get("native-login-status").textContent, /OpenCode：已检测/);
pending[0].resolve({ available: false });
await new Promise(resolve => setImmediate(resolve));
assert.match(nodes.get("native-login-status").textContent, /OpenCode：已检测/, "late Codex result must not overwrite OpenCode");
assert.deepEqual(inspections.map(i => i.kind), ["opencode"], "late response must not update table data");

const download = nodes.get("native-login-download").click();
assert.equal(pending[2].url, "/api/auth/native-login/opencode");
assert.equal(pending[2].options.method, "POST", "backup must read native credentials afresh");
pending[2].resolve({ created_at: 123, files: { auth: "test" } });
await download;
assert.deepEqual(downloads, ["webclx-opencode-login-123.json"]);
assert.equal(nodes.get("native-login-download").disabled, false);
const failure = nodes.get("native-login-download").click();
pending[3].reject(new Error("未检测到本机登录凭据"));
await failure;
assert.equal(downloads.length, 1, "failed backups must not produce a file");
assert.match(nodes.get("native-login-status").textContent, /备份 OpenCode 失败/);

// CodeArts uses its own endpoint and filename; a pending download keeps its original kind.
window.nativeLoginBackup.select("codearts");
const login = nodes.get("native-login-codearts-auth");
assert.equal(login.hidden, false);
assert.equal(login.href, "https://codearts.huaweicloud.com/portal/settings/cli-auth");
const html = fs.readFileSync("static/index.html", "utf8");
assert.match(html, /<a\b[^>]*id="native-login-codearts-auth"[^>]*target="_blank"[^>]*rel="noopener noreferrer"[^>]*>CLI 认证设置（AK\/SK）<\/a>/);
assert.match(html, /CODEARTS_CLI_AK/);
assert.match(html, /CODEARTS_CLI_SK/);
assert.match(html, /当前 CLI 登录入口要求 AK\/SK，尚未验证可用的 OAuth 登录流程/);
assert.match(html, /备份不包含仅存在于环境变量中的 AK\/SK/);
assert.equal(pending[4].url, "/api/auth/native-login/codearts");
pending[4].resolve({ available: true, accounts: [{ provider: "inferhub-provider", account_id: "account-test", auth_type: "oauth" }] });
await new Promise(resolve => setImmediate(resolve));
assert.match(nodes.get("native-login-status").textContent, /CodeArts：已检测/);
assert.equal(inspections.at(-1).result.accounts[0].account_id, "account-test", "read must send account summary to the table");
const codeartsDownload = nodes.get("native-login-download").click();
assert.equal(pending[5].url, "/api/auth/native-login/codearts");
assert.equal(pending[5].options.method, "POST");
window.nativeLoginBackup.select("opencode");
pending[6].resolve({ available: false });
await new Promise(resolve => setImmediate(resolve));
pending[5].resolve({ kind: "codearts", created_at: 456, files: { ".codeartsdoer/cli-data/auth.json": "test" } });
await codeartsDownload;
assert.equal(downloads[1], "webclx-codearts-login-456.json");
assert.match(nodes.get("native-login-status").textContent, /OpenCode：未检测/, "CodeArts download cannot overwrite another tab's status");
window.nativeLoginBackup.select("codearts");
pending[7].reject(new Error("broken native credential"));
await new Promise(resolve => setImmediate(resolve));
assert.equal(inspections.at(-1).kind, "codearts");
assert.equal(inspections.at(-1).result, null, "failed read must clear stale native account rows");

window.nativeLoginBackup.select("agy");
assert.equal(pending[8].url, "/api/auth/native-login/agy");
pending[8].resolve({
  available: true,
  account: { id: "agy-1", name: "beyondcy@gmail.com", email: "beyondcy@gmail.com", imported: true },
});
await new Promise(resolve => setImmediate(resolve));
assert.match(nodes.get("native-login-status").textContent, /AGY：已检测到本机登录凭据（beyondcy@gmail\.com）/);
assert.equal(inspections.at(-1).kind, "agy");
assert.equal(inspections.at(-1).result.account.email, "beyondcy@gmail.com");

window.nativeLoginBackup.select("qoder");
assert.equal(pending[9].url, "/api/auth/native-login/qoder");
pending[9].resolve({
  available: true,
  account: { name: "Gilbert Hagenbuch", email: "hagenbuchgilberteo@gmail.com" },
});
await new Promise(resolve => setImmediate(resolve));
assert.match(nodes.get("native-login-status").textContent, /Qoder：已检测到本机登录凭据（hagenbuchgilberteo@gmail\.com）/);
const qoderDownload = nodes.get("native-login-download").click();
assert.equal(pending[10].url, "/api/auth/native-login/qoder");
assert.equal(pending[10].options.method, "POST");
pending[10].resolve({ kind: "qoder", created_at: 789, files: { ".qoder/.auth/user": "test" } });
await qoderDownload;
assert.equal(downloads[2], "webclx-qoder-login-789.json");

// Execute the production selection-controls function with an empty library.
const manager = fs.readFileSync("static/app-codex-account-manager.js", "utf8");
const controls = new Map();
vm.runInNewContext(manager.slice(manager.indexOf("  function updateSelectionControls("), manager.indexOf("  async function writeClipboard(")) + "\nupdateSelectionControls([]);", {
  document: { getElementById(id) { if (!controls.has(id)) controls.set(id, {}); return controls.get(id); } },
  selectedAccountIds: new Set(), bulkUsageQueryRunning: false,
});
assert.equal(controls.get("codex-account-detect-current").disabled, false);
assert.equal(controls.get("codex-account-save-current").disabled, false);
assert.equal(controls.get("codex-account-export").disabled, true);
console.log("native login backup: tab isolation, fresh backup, failure and empty-library controls passed");
