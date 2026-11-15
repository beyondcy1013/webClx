import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const source = fs.readFileSync("static/app-codex-account-manager.js", "utf8");
const start = source.indexOf("  let codeartsNativeLogin = null;");
const end = source.indexOf("  function renderAccounts(accounts)", start);
assert.ok(start >= 0 && end > start, "native inspection must feed the CodeArts account table");
const list = { rows: [], appendChild(row) { this.rows.push(row); } };
const cell = value => ({ textContent: value || "-", appendChild(child) { this.child = child; } });
let renders = 0;
let downloads = 0;
const context = vm.createContext({
  window: { nativeLoginBackup: { download() { downloads++; } } },
  activeKind: "codearts", allAccounts: [], Date, Array,
  document: { createElement: () => ({ cells: [], classList: { add() {} }, append(...cells) { this.cells.push(...cells); } }) },
  textCell: cell,
  actionButton: (label, action) => ({ label, action }),
  renderAccounts() { renders++; },
});
vm.runInContext(source.slice(start, end), context);
const account = { provider: "inferhub-provider", account_id: "account-test", auth_type: "oauth" };
context.window.updateNativeLoginAccounts("codearts", { available: true, accounts: [account] });
assert.equal(renders, 1);
context.list = list;
vm.runInContext("renderCodeArtsNativeAccounts(list)", context);
assert.equal(list.rows.length, 1);
assert.equal(list.rows[0].cells.length, 17);
assert.equal(list.rows[0].cells[7].textContent, "account-test");
assert.equal(list.rows[0].cells[5].textContent, "CodeArts");
assert.equal(list.rows[0].cells[4].textContent, "未在线核验");
list.rows[0].cells[1].child.action();
assert.equal(downloads, 1);
context.activeKind = "api";
context.window.updateNativeLoginAccounts("codearts", { available: false, accounts: [] });
assert.equal(renders, 1, "CodeArts response cannot render over another tab");
context.activeKind = "codearts";
list.rows = [];
vm.runInContext("renderCodeArtsNativeAccounts(list)", context);
assert.equal(list.rows.length, 0, "missing credentials must clear the previous native account");
assert.match(source, /if \(activeKind === "codearts" && renderCodeArtsNativeAccounts\(list\)\) return;/);
console.log("CodeArts native account table: identity, backup, tab isolation and logout clearing passed");
