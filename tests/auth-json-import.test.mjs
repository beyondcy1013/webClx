import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const authManagerSource = readFileSync(
  new URL("../static/app-auth-manager.js", import.meta.url),
  "utf8",
);
const authStyles = readFileSync(
  new URL("../static/styles-auth.css", import.meta.url),
  "utf8",
);

const context = {
  TextDecoder,
  Uint8Array,
  window: {
    atob(value) {
      return Buffer.from(value, "base64").toString("binary");
    },
  },
};
vm.runInNewContext(authManagerSource, context);
const authManager = context.WebClxAuthManager.create({
  firstFiniteNumber: (...values) => values.find(Number.isFinite) ?? null,
  elements: {},
});

function fakeJwt(payload) {
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
  return `${encode({ alg: "none", typ: "JWT" })}.${encode(payload)}.signature`;
}

const sub2apiBundle = {
  exported_at: "2026-07-14T08:00:00Z",
  proxies: [],
  accounts: [
    {
      name: "Bundle account",
      platform: "openai",
      type: "oauth",
      credentials: {
        access_token: "bundle-access",
        expires_at: "2026-07-24T08:00:00Z",
        email: "bundle@example.com",
        chatgpt_account_id: "bundle-account-id",
        chatgpt_user_id: "bundle-user-id",
        plan_type: "team",
      },
    },
  ],
  type: "sub2api-data",
  version: 1,
};

const normalizedBundle = authManager.normalizeAuthInput(JSON.stringify(sub2apiBundle));
assert.equal(normalizedBundle.last_refresh, sub2apiBundle.exported_at);
assert.deepEqual(JSON.parse(JSON.stringify(normalizedBundle.tokens)), {
  access_token: "bundle-access",
  account_id: "bundle-account-id",
});
assert.equal(authManager.buildAuthPresetName(JSON.stringify(sub2apiBundle)), "Bundle account · bundle@example.com");

const multiAccountBundle = {
  exported_at: "2026-07-14T13:59:57Z",
  format: "sub2api",
  accounts: [
    {
      name: "Batch account 01",
      credentials: {
        access_token: fakeJwt({
          "https://api.openai.com/auth": {
            chatgpt_account_id: "jwt-account-01",
            chatgpt_plan_type: "team",
          },
        }),
        chatgpt_account_id: "",
        email: "batch-01@example.com",
        id_token: "",
        refresh_token: "",
      },
      extra: { last_refresh: "2026-07-14T13:55:01Z" },
    },
    {
      name: "Batch account 02",
      credentials: {
        access_token: fakeJwt({
          "https://api.openai.com/auth": {
            chatgpt_account_id: "jwt-account-02",
            chatgpt_plan_type: "plus",
          },
        }),
        chatgpt_account_id: "",
        email: "batch-02@example.com",
        id_token: "",
        refresh_token: "",
      },
      extra: { last_refresh: "2026-07-14T13:55:02Z" },
    },
  ],
};

const normalizedFirstAccount = authManager.normalizeAuthInput(JSON.stringify(multiAccountBundle));
assert.equal(
  normalizedFirstAccount.tokens.account_id,
  "jwt-account-01",
  "the existing single-account path should recover an empty explicit account id from the access token",
);

const normalizedMultiAccounts = authManager.normalizeAuthInputs(JSON.stringify(multiAccountBundle));
assert.equal(normalizedMultiAccounts.length, 2);
assert.deepEqual(
  JSON.parse(JSON.stringify(normalizedMultiAccounts.map((entry) => entry.auth.tokens.account_id))),
  ["jwt-account-01", "jwt-account-02"],
);
assert.deepEqual(
  JSON.parse(JSON.stringify(normalizedMultiAccounts.map((entry) => entry.name))),
  ["Batch account 01 · batch-01@example.com", "Batch account 02 · batch-02@example.com"],
);

const flatSession = {
  id_token: "",
  access_token: "flat-access",
  refresh_token: "",
  account_id: "flat-account-id",
  last_refresh: "2026-07-14T09:00:00Z",
  email: "flat@example.com",
  type: "codex",
  expired: "2026-07-24T09:00:00Z",
};
const normalizedFlat = authManager.normalizeAuthInput(JSON.stringify(flatSession));
assert.deepEqual(JSON.parse(JSON.stringify(normalizedFlat.tokens)), {
  access_token: "flat-access",
  account_id: "flat-account-id",
});
const editableFlat = JSON.parse(authManager.buildEditableAuthText(normalizedFlat));
assert.deepEqual(
  JSON.parse(JSON.stringify(editableFlat.tokens)),
  JSON.parse(JSON.stringify(normalizedFlat.tokens)),
);

const importedInput = {
  value: "",
  focus() {},
  scrollIntoView() {},
};
const importedName = { value: "" };
const importButton = { disabled: false };
const interactiveManager = context.WebClxAuthManager.create({
  state: { editingAuthPresetId: "" },
  firstFiniteNumber: (...values) => values.find(Number.isFinite) ?? null,
  updateStatus() {},
  renderConfigOverrideEditor() {},
  elements: {
    authPresetInputEl: importedInput,
    authPresetNameEl: importedName,
    authImportFileButton: importButton,
    authFormStatusEl: {},
    authSavePresetButton: {},
    authSaveAsNewPresetButton: {},
    authApplyEditedPresetButton: {},
    authClearInputButton: {},
    authApplyInputButton: {},
  },
});
const flatSessionText = JSON.stringify(flatSession);
assert.equal(
  await interactiveManager.importAuthJsonFile({
    name: "codex-account.json",
    type: "application/json",
    async text() {
      return flatSessionText;
    },
  }),
  true,
);
assert.equal(importedInput.value, flatSessionText);
assert.equal(importedName.value, "flat@example.com");
assert.equal(importButton.disabled, false);

assert.match(
  indexHtml,
  /id="auth-import-file"[^>]*type="file"[^>]*accept="application\/json,\.json"/,
  "Codex_OAuth form should expose a JSON file input",
);
assert.match(
  indexHtml,
  /id="auth-import-file-button"[^>]*>\s*导入 JSON 文件\s*<\/button>/,
  "Codex_OAuth form should expose a visible JSON import command",
);
assert.match(
  authManagerSource,
  /async function importAuthJsonFile\(sourceFile\)/,
  "Codex_OAuth manager should read selected JSON files through the shared parser",
);
assert.match(
  authStyles,
  /\.auth-actions\s*>\s*\.button\s*\{[^}]*flex:\s*0\s+0\s+auto;/,
  "Codex_OAuth action buttons should not collapse into narrow multi-line controls on mobile",
);
