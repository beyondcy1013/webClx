import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const authManagerSource = readFileSync(
  new URL("../static/app-auth-manager.js", import.meta.url),
  "utf8",
);
const apiManagerSource = readFileSync(
  new URL("../static/app-api-manager.js", import.meta.url),
  "utf8",
);
const appSource = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const eventBindingsSource = readFileSync(
  new URL("../static/app-preset-form-event-bindings.js", import.meta.url),
  "utf8",
);

function functionSource(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

assert.match(
  indexHtml,
  /id="api-add-preset"[^>]*>\s*新增预设\s*<\/button>/,
);
assert.match(
  indexHtml,
  /id="api-account-import-file-button"[^>]*>\s*导入 JSON \/ 压缩包\s*<\/button>/,
);
assert.match(
  indexHtml,
  /id="api-account-import-file"[^>]*type="file"[^>]*multiple[^>]*accept="[^"]*\.json[^"]*\.zip[^"]*\.tar\.gz[^"]*"/,
);
assert.match(
  indexHtml,
  /id="api-account-import-text-button"[^>]*>\s*从文本导入\s*<\/button>/,
);
assert.match(indexHtml, /id="api-account-import-dialog"/);
assert.match(indexHtml, /id="api-account-import-text"/);
assert.match(
  appSource,
  /async function importAuthAccountFiles\(sourceFiles\)[\s\S]*new FormData\(\)[\s\S]*forEach[\s\S]*formData\.append\("file", sourceFile[\s\S]*requestJson\("\/api\/auth\/api-presets\/import-file"/,
);
assert.match(
  eventBindingsSource,
  /Array\.from\(apiAccountImportFileInputEl\.files \|\| \[\]\)[\s\S]*importApiAccountsFromFiles\(sourceFiles\)/,
);

const oauthSaveSource = functionSource(authManagerSource, "saveAuthPresetFromRawText");
assert.match(oauthSaveSource, /normalizeAuthInput\(trimmedText\)/);
assert.doesNotMatch(oauthSaveSource, /normalizeAuthInputs\(trimmedText\)/);

const authContext = {
  TextDecoder,
  Uint8Array,
  window: {
    atob(value) {
      return Buffer.from(value, "base64").toString("binary");
    },
  },
};
vm.runInNewContext(authManagerSource, authContext);
const authManager = authContext.WebClxAuthManager.create({
  firstFiniteNumber: (...values) => values.find(Number.isFinite) ?? null,
  elements: {},
});

const legacyOauthArray = [{
  id_token: "legacy-id",
  access_token: "legacy-access",
  refresh_token: "legacy-refresh",
  account_id: "legacy-account",
  last_refresh: "2026-07-14T23:00:00Z",
  email: "legacy@example.com",
  type: "codex",
}];
const normalizedLegacy = authManager.normalizeAuthInput(JSON.stringify(legacyOauthArray));
assert.deepEqual(JSON.parse(JSON.stringify(normalizedLegacy.tokens)), {
  access_token: "legacy-access",
  account_id: "legacy-account",
  id_token: "legacy-id",
  refresh_token: "legacy-refresh",
});

const importedTexts = [];
const uploadedFiles = [];
const statusMessages = [];
const dialog = {
  open: true,
  close() {
    this.open = false;
  },
};
const importTextEl = { value: "" };
const apiContext = {
  TextDecoder,
  window: {
    requestAnimationFrame(callback) {
      callback();
    },
  },
};
vm.runInNewContext(apiManagerSource, apiContext);
const apiManager = apiContext.WebClxApiManager.create({
  async importAuthAccounts(rawText) {
    importedTexts.push(rawText);
    return { saved_count: 2 };
  },
  async importAuthAccountFiles(sourceFiles) {
    uploadedFiles.push(sourceFiles);
    return { saved_count: 3, saved_names: ["one", "two", "three"], errors: [] };
  },
  updateStatus(_element, message) {
    statusMessages.push(message);
  },
  elements: {
    apiAccountImportDialogEl: dialog,
    apiAccountImportTextEl: importTextEl,
    apiAccountImportFileButton: { disabled: false },
    apiAccountImportTextButton: { disabled: false },
    apiAccountImportSubmitButton: { disabled: false },
    apiAccountImportStatusEl: {},
  },
});

assert.equal(await apiManager.importApiAccountsFromText("{\"accounts\":[]}"), true);
assert.equal(importedTexts[0], "{\"accounts\":[]}");
assert.equal(dialog.open, false);
assert.ok(statusMessages.some((message) => message.includes("已导入 2 个")));

assert.equal(await apiManager.importApiAccountsFromFile({
  name: "accounts.zip",
  type: "application/zip",
}), true);
assert.equal(uploadedFiles[0][0].name, "accounts.zip");
assert.ok(statusMessages.some((message) => message.includes("已导入 3 个")));

assert.equal(await apiManager.importApiAccountsFromFiles([
  { name: "cpa-one.json", size: 1024 },
  { name: "cpa-two.json", size: 2048 },
]), true);
assert.deepEqual(
  JSON.parse(JSON.stringify(uploadedFiles[1].map((file) => file.name))),
  ["cpa-one.json", "cpa-two.json"],
);

assert.equal(await apiManager.importApiAccountsFromFile({
  name: "too-large.zip",
  size: 32 * 1024 * 1024 + 1,
}), false);
assert.equal(uploadedFiles.length, 2);
assert.ok(statusMessages.some((message) => message.includes("32 MiB")));

console.log("api account import placement tests passed");
