import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const managerSource = readFileSync(
  new URL("../static/app-account-clipboard-manager.js", import.meta.url),
  "utf8",
);
const routesSource = readFileSync(new URL("../src/routes/workspace.rs", import.meta.url), "utf8");
const backendSource = readFileSync(new URL("../src/preset_sync.rs", import.meta.url), "utf8");
const tableSource = readFileSync(
  new URL("../static/app-preset-table.js", import.meta.url),
  "utf8",
);
const tableHeaderSource = readFileSync(
  new URL("../static/app-config-override.js", import.meta.url),
  "utf8",
);

for (const prefix of ["auth", "api", "claude"]) {
  assert.match(
    indexHtml,
    new RegExp(`id="${prefix}-clipboard-import"[^>]*>\\s*从剪贴板导入\\s*</button>`),
  );
  assert.match(
    indexHtml,
    new RegExp(
      `id="${prefix}-clipboard-export"[^>]*>\\s*${prefix === "api" ? "导出已选" : "导出到剪贴板"}\\s*</button>`,
    ),
  );
}

assert.match(
  routesSource,
  /\/api\/settings\/preset-config\/clipboard\/\{section\}\/export/,
  "the clipboard transfer endpoint should be scoped by account category",
);
assert.match(routesSource, /\/api\/settings\/preset-config\/clipboard\/\{section\}\/import/);
assert.match(backendSource, /pub async fn export_account_presets_to_clipboard/);
assert.match(backendSource, /pub async fn import_account_presets_from_clipboard/);
assert.match(tableSource, /function createPresetSelectionCell/);
assert.match(tableHeaderSource, /function createPresetSelectAllHeader/);
assert.match(managerSource, /document\.execCommand\?\.\("copy"\)/);
assert.match(managerSource, /function openManualImportDialog/);

function fakeButton() {
  return {
    disabled: false,
    listeners: new Map(),
    addEventListener(type, callback) {
      this.listeners.set(type, callback);
    },
  };
}

const clipboard = {
  text: "",
  async readText() {
    return this.text;
  },
  async writeText(value) {
    this.text = value;
  },
};
const requests = [];
const statuses = [];
let refreshCount = 0;
let selectedIds = ["oauth-1"];
const context = { globalThis: {}, navigator: { clipboard }, window: { confirm: () => true } };
vm.runInNewContext(managerSource, context);

const authImportButton = fakeButton();
const authExportButton = fakeButton();
const manager = context.globalThis.WebClxAccountClipboardManager.create({
  clipboard,
  confirmImport: () => true,
  async requestJson(url, options = {}) {
    requests.push({ url, options });
    if (url.endsWith("/export")) {
      return {
        format: "webclx-account-presets",
        version: 1,
        section: "auth_presets",
        accounts: [{ id: "oauth-1", auth: { tokens: { access_token: "secret" } } }],
      };
    }
    return { ok: true, imported_count: 1 };
  },
  async refreshAuthPanels() {
    refreshCount += 1;
  },
  updateStatus(_element, message, tone) {
    statuses.push({ message, tone });
  },
  sections: [{
    section: "auth_presets",
    label: "Codex_OAuth",
    getSelectedIds: () => selectedIds,
    importButton: authImportButton,
    exportButton: authExportButton,
    statusElement: {},
  }],
});

assert.equal(await manager.exportSection("auth_presets"), true);
assert.equal(requests[0].url, "/api/settings/preset-config/clipboard/auth_presets/export");
assert.equal(requests[0].options.method, "POST");
assert.deepEqual(JSON.parse(requests[0].options.body), { ids: ["oauth-1"] });
assert.equal(JSON.parse(clipboard.text).section, "auth_presets");
assert.equal(JSON.parse(clipboard.text).accounts[0].auth.tokens.access_token, "secret");

requests.length = 0;
selectedIds = [];
assert.equal(await manager.exportSection("auth_presets"), false);
assert.equal(requests.length, 0);
assert.ok(statuses.some(({ message }) => message.includes("请先勾选")));
selectedIds = ["oauth-1"];

requests.length = 0;
clipboard.text = JSON.stringify({
  format: "webclx-account-presets",
  version: 1,
  section: "auth_presets",
  accounts: [{ id: "oauth-2" }],
});
assert.equal(await manager.importSection("auth_presets"), true);
assert.equal(requests[0].url, "/api/settings/preset-config/clipboard/auth_presets/import");
assert.equal(requests[0].options.method, "POST");
assert.equal(requests[0].options.body, clipboard.text);
assert.equal(refreshCount, 1);
assert.ok(statuses.some(({ message, tone }) => tone === "ok" && message.includes("1 个")));

let fallbackCopiedText = "";
let manualImportSection = "";
const insecureManager = context.globalThis.WebClxAccountClipboardManager.create({
  clipboard: {},
  confirmImport: () => true,
  copyText: async (value) => {
    fallbackCopiedText = value;
    return true;
  },
  openManualImport(section) {
    manualImportSection = section;
  },
  async requestJson(url) {
    assert.match(url, /\/auth_presets\/export$/);
    return {
      format: "webclx-account-presets",
      version: 1,
      section: "auth_presets",
      accounts: [{ id: "oauth-insecure" }],
    };
  },
  async refreshAuthPanels() {},
  updateStatus(_element, message, tone) {
    statuses.push({ message, tone });
  },
  sections: [{
    section: "auth_presets",
    label: "Codex_OAuth",
    getSelectedIds: () => ["oauth-insecure"],
    importButton: fakeButton(),
    exportButton: fakeButton(),
    statusElement: {},
  }],
});

assert.equal(await insecureManager.exportSection("auth_presets"), true);
assert.equal(JSON.parse(fallbackCopiedText).accounts[0].id, "oauth-insecure");
assert.equal(await insecureManager.importSection("auth_presets"), false);
assert.equal(manualImportSection, "auth_presets");
assert.doesNotMatch(statuses.at(-1).message, /浏览器不支持/);

class FakeElement {
  constructor(tagName) {
    this.tagName = tagName;
    this.children = [];
    this.listeners = new Map();
  }

  appendChild(child) {
    this.children.push(child);
    return child;
  }

  addEventListener(type, callback) {
    this.listeners.set(type, callback);
  }

  setAttribute(name, value) {
    this[name] = value;
  }
}

function domContext() {
  return vm.createContext({
    console,
    Element: FakeElement,
    Intl,
    document: {
      createElement(tagName) {
        return new FakeElement(tagName);
      },
    },
  });
}

const rowContext = domContext();
vm.runInContext(tableSource, rowContext);
const rowSelectedIds = new Set();
const selectionCell = rowContext.createPresetSelectionCell(
  { id: "row-1", name: "Row one" },
  { selectedIds: rowSelectedIds },
);
const rowCheckbox = selectionCell.children[0];
rowCheckbox.checked = true;
rowCheckbox.listeners.get("change")();
assert.deepEqual(Array.from(rowSelectedIds), ["row-1"]);

const headerContext = domContext();
vm.runInContext(tableHeaderSource, headerContext);
const allSelectedIds = new Set(["row-1"]);
const selectAllHeader = headerContext.createPresetSelectAllHeader({
  label: "测试账号",
  presets: [{ id: "row-1" }, { id: "row-2" }],
  selectedIds: allSelectedIds,
});
const selectAllCheckbox = selectAllHeader.children[0];
assert.equal(selectAllCheckbox.indeterminate, true);
selectAllCheckbox.checked = true;
selectAllCheckbox.listeners.get("change")();
assert.deepEqual(Array.from(allSelectedIds), ["row-1", "row-2"]);

console.log("account preset clipboard tests passed");
